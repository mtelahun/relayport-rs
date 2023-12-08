//! Abstractions that relay TCP communication

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use socket2::{Domain, Protocol, SockAddr, Type};
use tokio::net::UdpSocket;
use tokio::sync::broadcast::Receiver;
use tokio::sync::RwLock;
use tracing::debug;

use crate::command::RelayCommand;
use crate::RelayPortError;

/// Maximum packet size (default MTU on most systems)
const MAX_PKT_SIZE: usize = 1518;

/// Abstraction for initiating the relay builder.
#[derive(Debug)]
pub struct RelaySocket {}

/// Builder abstraction for composing a TCP relay.
#[derive(Copy, Clone, Debug)]
pub struct RelaySocketBuilder(RelayInner);

/// Abstraction containing a socket bound to a local address.
#[derive(Debug)]
pub struct BoundRelaySocket {
    socket: UdpRelaySocket,
    client_map: Arc<RwLock<HashMap<SocketAddr, UdpRelaySocket>>>,
    peer_map_by_client_addr: Arc<RwLock<HashMap<SocketAddr, UdpRelaySocket>>>,
    peer_map_by_port: Arc<RwLock<HashMap<u16, UdpRelaySocket>>>,
}

#[derive(Copy, Clone, Debug)]
struct RelayInner {
    so_reuseaddr: bool,
}

#[derive(Clone, Debug)]
struct UdpRelaySocket(Arc<UdpSocket>);

impl RelaySocket {
    /// Obtain a builder object to begin composing the relay socket.
    ///
    /// # Returns
    /// It returns a [RelaySocketBuilder] object.
    ///
    /// # Example
    /// ```
    /// use relayport_rs::RelaySocket;
    ///
    /// let builder = RelaySocket::build();
    ///
    /// // configure builder
    /// ```
    pub fn build() -> RelaySocketBuilder {
        RelaySocketBuilder(RelayInner {
            so_reuseaddr: false,
        })
    }
}

impl RelaySocketBuilder {
    /// Binds a TCP socket to addr.
    /// The addr argument must be string slice that can be converted into a local addresss and port. This
    /// method returns [BoundRelaySocket] wrapped in a [Result]. If the operation fails for any reason the
    /// Err() value will contain the reason for the failure.
    ///
    /// # Example
    /// ```
    /// use relayport_rs::RelaySocket;
    ///
    /// let bound_relay = RelaySocket::build()
    ///     .bind("127.0.0.1:10443")
    ///     .unwrap();
    ///
    /// // listen on the bound address
    /// ```
    pub async fn bind(self, addr: &str) -> Result<BoundRelaySocket, RelayPortError> {
        let addr = self.parse_address(addr)?;

        self.bind_addr(&addr).await
    }

    async fn bind_addr(self, addr: &SocketAddr) -> Result<BoundRelaySocket, RelayPortError> {
        let socket = self.create_bound_socket(addr)?;

        Ok(BoundRelaySocket {
            socket: UdpRelaySocket::new(socket),
            client_map: Arc::new(RwLock::new(HashMap::<SocketAddr, UdpRelaySocket>::new())),
            peer_map_by_client_addr: Arc::new(RwLock::new(
                HashMap::<SocketAddr, UdpRelaySocket>::new(),
            )),
            peer_map_by_port: Arc::new(RwLock::new(HashMap::<u16, UdpRelaySocket>::new())),
        })
    }

    /// Change the SO_REUSEADDR option on the TCP socket. If reuseaddr is `true` the option will
    /// be set on the created socket. If it is `false` the option will be disabled.
    /// # Returns
    /// A [RelaySocketBuilder] object configured according to the reuseaddr argument.
    ///
    /// ```
    /// use relayport_rs::{RelayPortError, RelaySocket};
    /// # #[tokio::main]
    /// # pub async fn main() -> Result<(), RelayPortError>  {
    ///     let mut builder = RelaySocket::build();
    ///     builder.set_so_reuseaddr(true);
    ///
    ///     assert_eq!(builder.so_reuseaddr(), true);
    ///
    /// #     Ok(())
    /// # }
    ///
    /// ```
    pub fn set_so_reuseaddr(&mut self, reuseaddr: bool) -> &mut Self {
        self.0.so_reuseaddr = reuseaddr;

        self
    }

    /// Get the builder configuration for setting SO_REUSEADDR on the TCP socket.
    pub fn so_reuseaddr(&self) -> bool {
        self.0.so_reuseaddr
    }

    fn parse_address(&self, addr: &str) -> Result<SocketAddr, RelayPortError> {
        addr.parse::<SocketAddr>()
            .map_err(RelayPortError::ParseAddrError)
    }

    fn create_bound_socket(&self, addr: &SocketAddr) -> Result<UdpSocket, RelayPortError> {
        let socket = if addr.is_ipv4() {
            socket2::Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?
        } else {
            socket2::Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?
        };
        if self.0.so_reuseaddr {
            debug!("set reuse_address={} on {addr} ", self.0.so_reuseaddr);
            socket.set_reuse_address(true)?;
        }
        debug!("set non-blocking on {addr}");
        socket.set_nonblocking(true)?;
        socket.bind(&SockAddr::from(*addr))?;
        let socket: std::net::UdpSocket = socket.into();

        Ok(UdpSocket::from_std(socket)?)
    }
}

impl BoundRelaySocket {
    /// Connects a TCP socket to a target peer denoted by addr. The addr
    /// argument must be a string slice that can be converted to a [`SocketAddr`] object. The
    /// bind_addr argument is an Option containing a string slice that specifies the
    /// local address the socket should bind to. If it is None the method will bind
    /// to a random local address.
    /// # Returns
    /// On success a [RelayStream] is returned. Otherwise a RelayPortError error is returned.
    ///
    /// # Example
    /// ```no_run
    /// use relayport_rs::{RelayPortError, RelaySocket, RelayStream};
    ///
    /// # #[tokio::main]
    /// # pub async fn main() -> Result<(), RelayPortError>  {
    ///     let relay_stream = RelaySocket::build()
    ///         .connect("127.0.0.1:443", None)
    ///         .await?;
    ///
    /// #     Ok(())
    /// # }
    ///
    /// ```
    async fn connect_to_client(
        &self,
        addr: &SocketAddr,
        bind_addr: Option<&SocketAddr>,
    ) -> Result<(), RelayPortError> {
        let mut client_map = self.client_map.write().await;
        if !client_map.contains_key(addr) {
            let socket;
            if let Some(local_addr) = bind_addr {
                debug!("attempting to bind to {local_addr}");
                socket = RelaySocket::build()
                    .set_so_reuseaddr(true)
                    .create_bound_socket(local_addr)?;
            } else {
                let any_addr = RelaySocket::build().parse_address("0.0.0.0:0")?;
                debug!("attempting to bind to {any_addr}");
                socket = RelaySocket::build()
                    .set_so_reuseaddr(true)
                    .create_bound_socket(&any_addr)?;
            }
            socket.connect(addr).await?;
            let socket = UdpRelaySocket::new(socket);
            client_map.insert(*addr, socket.clone());
        }

        Ok(())
    }

    // async fn connect_to_remote(&self, addr: &str, client_addr: &SocketAddr) -> Result<(), RelayPortError> {
    //     let socket = UdpSocket::bind("0.0.0.0:0").await?;
    //     socket.connect(addr).await?;
    //     let socket = UdpRelaySocket::new(socket);
    //     let local_addr = socket.0.local_addr()?;
    //     self.peer_map_by_client_addr.insert(*client_addr, socket.clone());
    //     self.peer_map_by_port.insert(local_addr.port(), socket);

    //     Ok(())
    // }

    /// Same as connect() but the caller provide the peer address as a SocketAddr.
    async fn connect_to_remote_addr(
        &self,
        addr: &SocketAddr,
        client_addr: &SocketAddr,
    ) -> Result<(), RelayPortError> {
        let mut peer_map_by_client_addr = self.peer_map_by_client_addr.write().await;
        if !peer_map_by_client_addr.contains_key(client_addr) {
            let mut peer_map_by_port = self.peer_map_by_port.write().await;
            let socket = UdpSocket::bind("0.0.0.0:0").await?;
            socket.connect(addr).await?;
            let socket = UdpRelaySocket::new(socket);
            let local_addr = socket.0.local_addr()?;
            peer_map_by_client_addr.insert(*client_addr, socket.clone());
            peer_map_by_port.insert(local_addr.port(), socket);
        }

        Ok(())
    }

    /// Start relaying incomming TCP traffic to the remote peer.
    ///
    /// Wait for a connection to come in and begin relaying traffic from
    /// the client to the remote peer and vice versa. After an incomming connection is
    /// accepted this method will spawn a [tokio::task] to handle relaying
    /// traffic. The spawned task will be a bi-directional relay that will handle traffic
    /// in both directions asynchronously. The caller is responsible for creating a
    /// [tokio::sync::broadcast] chanel and passing the receiver half to this method. The
    /// chanel can be used by the caller to send [RelayCommand] objects to the spawned task.
    /// Currently, the only command supported by the library is a shutdown command to instruct
    /// the task to close the TCP socket and exit cleanly.
    ///
    /// # Returns
    /// This method will not return unless a [RelayCommand::Shutdown] is sent by the caller or
    /// it encounters an error. If it encounters an error it will return a RelayPortError.
    ///
    /// # panics
    /// This method may panic if it is unable to create a connection to the remote peer.
    ///
    /// # Example
    /// ```no_run
    /// # use std::error::Error;
    /// use tokio::sync::broadcast;
    /// use relayport_rs::{RelayCommand, RelayPortError, RelaySocket};
    ///
    /// # #[tokio::main]
    /// # pub async fn main() -> Result<(), Box<dyn Error>>  {
    ///     // The relay expects a broadcast channel on which to listen for shutdown commands
    ///     let (tx, rx) = broadcast::channel(16);
    ///
    ///     let listener = RelaySocket::build()
    ///         .bind("127.0.0.1:10443")?
    ///         .listen()?;
    ///
    ///     // spawn a task to handle the acceptance and dispatch of a relay
    ///     let _ = tokio::task::spawn(async move {
    ///         listener
    ///             .serve("127.0.0.1:80", &rx)
    ///             .await
    ///             .expect("failed to start relay")
    ///     });
    ///
    ///     // Do other work
    ///     tokio::time::sleep(std::time::Duration::from_secs(60));
    ///
    ///     // send the task a shutdown command so it exits cleanly
    ///     tx.send(RelayCommand::Shutdown)?;
    ///
    /// #     Ok(())
    /// # }
    ///
    /// ```
    pub async fn serve(
        &self,
        remote: &str,
        cancel: &Receiver<RelayCommand>,
    ) -> Result<(), RelayPortError> {
        let remote_addr = remote.parse().map_err(RelayPortError::ParseAddrError)?;
        let my_addr = self.socket.0.local_addr()?;
        loop {
            let mut cancel = cancel.resubscribe();
            let mut buf = [0u8; MAX_PKT_SIZE];
            let (len, client_addr) = self.socket.0.recv_from(&mut buf).await?;
            debug!("received data from {}", client_addr);
            let remote_socket = self.peer_socket_by_client(&remote_addr, &client_addr).await;
            let client_socket = self.client_socket_by_addr(&client_addr, &my_addr).await;
            tokio::select! {
                biased;
                _ = self.process_connection(&client_socket, &remote_socket, cancel.resubscribe(), buf, len) => { continue }
                result = cancel.recv() => {match result {
                    Ok(cmd) => match cmd {
                        RelayCommand::Shutdown => {
                            debug!("received relay command: shutdown");
                            break
                        },
                    },
                    Err(e) => return Err(RelayPortError::InternalCommunicationError(e)),
                }}
            }
        }

        Ok(())
    }

    async fn peer_socket_by_client(
        &self,
        addr: &SocketAddr,
        client_addr: &SocketAddr,
    ) -> UdpRelaySocket {
        {
            let peer_map = self.peer_map_by_client_addr.read().await;
            if let Some(peer) = peer_map.get(client_addr) {
                return peer.clone();
            }
        }
        self.connect_to_remote_addr(addr, client_addr)
            .await
            .unwrap_or_else(|_| {
                panic!("failed to connect to remote peer on behalf of {client_addr}")
            });
        let peer_map = self.peer_map_by_client_addr.read().await;

        peer_map.get(client_addr).unwrap().clone()
    }

    async fn client_socket_by_addr(
        &self,
        addr: &SocketAddr,
        bind_addr: &SocketAddr,
    ) -> UdpRelaySocket {
        {
            let client_map = self.client_map.read().await;
            if let Some(client) = client_map.get(addr) {
                return client.clone();
            }
        }
        self.connect_to_client(addr, Some(bind_addr))
            .await
            .unwrap_or_else(|_| panic!("failed to bind to {bind_addr} on behalf of client {addr}"));

        let client_map = self.client_map.read().await;
        client_map.get(addr).unwrap().clone()
    }

    async fn process_connection(
        &self,
        client: &UdpRelaySocket,
        remote: &UdpRelaySocket,
        cancel: Receiver<RelayCommand>,
        buf: [u8; MAX_PKT_SIZE],
        len: usize,
    ) -> Result<(), RelayPortError> {
        let client = client.clone();
        let remote = remote.clone();
        let empty_buf = [0u8; MAX_PKT_SIZE];

        tokio::spawn(async move {
            let client_r = client.clone();
            let client_w = client.clone();
            let remote_r = remote.clone();
            let remote_w = remote.clone();
            let _ = tokio::join!(
                relay_inner(&client_r, &remote_w, cancel.resubscribe(), &buf, len),
                relay_inner(&remote_r, &client_w, cancel, &empty_buf, 0),
            );
        });

        Ok(())
    }
}

impl UdpRelaySocket {
    pub fn new(socket: UdpSocket) -> Self {
        Self(Arc::new(socket))
    }
}

#[tracing::instrument(level = "debug", skip_all, err, ret, fields(from_addr, to_addr))]
async fn relay_inner(
    read_sock: &UdpRelaySocket,
    write_sock: &UdpRelaySocket,
    mut rx: Receiver<RelayCommand>,
    buf: &[u8],
    len: usize,
) -> Result<usize, RelayPortError> {
    let from_addr = read_sock.0.peer_addr().unwrap();
    let to_addr = write_sock.0.peer_addr().unwrap();
    if len != 0 {
        let _ = write_sock.0.send(&buf[0..len]).await?;
        debug!("wrote initial udp packet from {from_addr} to {to_addr}");
    }

    let mut xfer_bytes = 0;
    let mut buf = [0u8; MAX_PKT_SIZE];
    let copy_reader_to_writer = async move {
        while let Ok(len) = read_sock.0.recv(&mut buf).await {
            debug!("received {len} bytes from {from_addr}");
            if len == 0 {
                break;
            }
            let result = write_sock.0.send(&buf[0..len]).await;
            if result.is_err() {
                let e = result.err().unwrap();
                eprintln!("failed to send packet to {to_addr}: {e}");
                return Err(e);
            } else {
                let len = result.ok().unwrap();
                debug!("wrote {len} bytes to {to_addr}");
                xfer_bytes += len;
            }
        }

        Ok(xfer_bytes)
    };
    let await_shutdown = rx.recv();
    let mut read_bytes = Ok(0);
    tokio::select! {
        biased;
            result = copy_reader_to_writer => {
                read_bytes = result
            }
            result = await_shutdown => { match result {
                Ok(cmd) => match cmd {
                    RelayCommand::Shutdown => {
                        debug!("received shutdown relay command");
                    },
                },
                Err(e) => return Err(RelayPortError::InternalCommunicationError(e)),
            }}
    }

    match read_bytes {
        Ok(bytes) => {
            debug!("Transferred {bytes} bytes");
            Ok(bytes)
        }
        Err(e) => Err(RelayPortError::IoError(e)),
    }
}
