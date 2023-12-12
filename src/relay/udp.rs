//! Abstractions that relay UDP ports

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use socket2::{Domain, Protocol, SockAddr, Type};
use tokio::net::UdpSocket;
use tokio::sync::broadcast::Receiver;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::debug;

use crate::command::RelayCommand;
use crate::RelayPortError;

/// Maximum UDP buffer size
const MAX_PKT_SIZE: usize = 10240;

/// Maximum unidirectional link idle time
const UDP_TTL: Duration = Duration::from_secs(180);

/// Abstraction for initiating the relay builder.
#[derive(Debug)]
pub struct RelaySocket {}

/// Builder abstraction for composing a UDP relay.
#[derive(Copy, Clone, Debug)]
pub struct RelaySocketBuilder(RelayInner);

/// Abstraction containing a socket bound to a local address.
#[derive(Debug)]
pub struct BoundRelaySocket {
    socket: InnerUdpSocket,
    client_connections: Arc<RwLock<HashMap<SocketAddr, InnerUdpSocket>>>,
    remote_connections: Arc<RwLock<HashMap<SocketAddr, InnerUdpSocket>>>,
}

#[derive(Copy, Clone, Debug)]
struct RelayInner {
    so_reuseaddr: bool,
}

#[derive(Clone, Debug)]
struct InnerUdpSocket(Arc<UdpSocket>);

impl RelaySocket {
    /// Obtain a builder object to begin composing the relay socket.
    ///
    /// # Returns
    /// It returns a [RelaySocketBuilder] object.
    ///
    /// # Example
    /// ```
    /// use relayport_rs::RelayUdpSocket;
    ///
    /// let builder = RelayUdpSocket::build();
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
    /// Binds a UDP socket to addr.
    ///
    /// The addr argument must be a string slice that can be converted into a local address and port.
    /// Unlike its [RelayTcpSock] counterpart this method is asynchronous.
    ///
    /// # Returns
    /// This method returns a [BoundRelaySocket] wrapped in a [Result]. If the operation fails for
    /// any reason the Err() value will contain the reason for the failure.
    ///
    /// # Example
    /// ```
    /// use relayport_rs::RelayUdpSocket;
    /// # use relayport_rs::RelayPortError;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), RelayPortError> {
    ///
    ///     let bound_relay = RelayUdpSocket::build()
    ///        .bind("127.0.0.1:10443")
    ///       .await?;
    ///
    /// #   Ok(())
    /// # }
    /// // listen on the bound address
    /// ```
    pub async fn bind(self, addr: &str) -> Result<BoundRelaySocket, RelayPortError> {
        let addr = self.parse_address(addr)?;

        self.bind_addr(&addr).await
    }

    async fn bind_addr(self, addr: &SocketAddr) -> Result<BoundRelaySocket, RelayPortError> {
        let socket = self.create_bound_socket(addr)?;

        Ok(BoundRelaySocket {
            socket: InnerUdpSocket::new(socket),
            client_connections: Arc::new(RwLock::new(HashMap::<SocketAddr, InnerUdpSocket>::new())),
            remote_connections: Arc::new(RwLock::new(HashMap::<SocketAddr, InnerUdpSocket>::new())),
        })
    }

    /// Change the SO_REUSEADDR option on the UDP socket.
    /// If reuseaddr is `true` the option will
    /// be set on the created socket. If it is `false` the option will be disabled.
    /// # Returns
    /// A [RelaySocketBuilder] object configured according to the reuseaddr argument.
    ///
    /// ```
    /// use relayport_rs::{RelayPortError, RelayUdpSocket};
    /// # #[tokio::main]
    /// # pub async fn main() -> Result<(), RelayPortError>  {
    ///     let mut builder = RelayUdpSocket::build();
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

    /// Get the builder configuration for setting SO_REUSEADDR on the UDP socket.
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
    /// Start relaying incomming UDP traffic to the remote peer.
    ///
    /// Wait for a connection to come in and begin relaying traffic from
    /// the client to the remote peer and vice versa. After an incomming connection is
    /// accepted this method will spawn a [tokio::task] to handle relaying
    /// traffic. The spawned task will be a bi-directional relay that will handle traffic
    /// in both directions asynchronously. The caller is responsible for creating a
    /// [tokio::sync::broadcast] chanel and passing the receiver half to this method. The
    /// chanel can be used by the caller to send [RelayCommand] objects to the spawned task.
    /// Currently, the only command supported by the library is a shutdown command to instruct
    /// the task to exit cleanly.
    ///
    /// # Returns
    /// This method will not return unless a [RelayCommand::Shutdown] is sent by the caller or
    /// it encounters an error. If it encounters an error it will return a RelayPortError.
    ///
    /// # Example
    /// ```no_run
    /// # use std::error::Error;
    /// use tokio::sync::broadcast;
    /// use relayport_rs::{RelayCommand, RelayPortError, RelayUdpSocket};
    ///
    /// # #[tokio::main]
    /// # pub async fn main() -> Result<(), Box<dyn Error>>  {
    ///     // The relay expects a broadcast channel on which to listen for shutdown commands
    ///     let (tx, rx) = broadcast::channel(16);
    ///
    ///     let listener = RelayUdpSocket::build()
    ///         .set_so_reuseaddr(true)
    ///         .bind("127.0.0.1:10443")
    ///         .await?;
    ///
    ///     // spawn a task to handle the acceptance and dispatch of a relay
    ///     let _ = tokio::task::spawn(async move {
    ///         listener
    ///             .run("127.0.0.1:80", &rx)
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
    pub async fn run(
        &self,
        remote: &str,
        command: &Receiver<RelayCommand>,
    ) -> Result<(), RelayPortError> {
        let remote_addr = remote.parse().map_err(RelayPortError::ParseAddrError)?;
        let my_addr = self.socket.0.local_addr()?;

        let (cancel_tx, cancel_rx) = tokio::sync::mpsc::channel(24);
        self.spawn_connection_cancel(cancel_rx).await;
        loop {
            let mut command = command.resubscribe();
            let mut buf = [0u8; MAX_PKT_SIZE];
            let (len, client_addr) = self.socket.0.recv_from(&mut buf).await?;

            // There is a delay between when we receive a packet and when we setup the bidirectional
            // link (creating a race-condition). Catch any packets that arrive during this race.
            if let Some(sock) = self.remote_connections.read().await.get(&client_addr) {
                sock.0.send(&buf[0..len]).await?;
                continue;
            } else if let Some(sock) = self.client_connections.read().await.get(&client_addr) {
                sock.0.send(&buf[0..len]).await?;
                continue;
            }

            // This is a new connection. Send the initial packet to the remote and
            // setup the bidirectional link.
            let client_socket = self
                .new_client_connection(&client_addr, Some(&my_addr))
                .await?;
            let remote_socket = self
                .new_remote_connection(&remote_addr, &client_addr)
                .await?;
            let _ = remote_socket.0.send(&buf[0..len]).await?;
            tokio::select! {
                biased;
                _ = self.spawn_bidirectional_relay(client_socket, remote_socket, command.resubscribe(), cancel_tx.clone()) => { continue }
                result = command.recv() => {match result {
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

    /// Connects a UDP socket to the remote peer, denoted by addr, to which we are relaying traffic.
    /// The addr argument must be a string slice that can be converted to a [`SocketAddr`]
    /// object. The bind_addr argument is an Option containing a string slice that specifies the
    /// local address the socket should bind to. If it is None the method will bind
    /// to a random local address.
    /// # Returns
    /// On success the unit type () is returned. Otherwise a RelayPortError error is returned.
    ///
    async fn new_remote_connection(
        &self,
        addr: &SocketAddr,
        client_addr: &SocketAddr,
    ) -> Result<InnerUdpSocket, RelayPortError> {
        let mut peer_map_by_client_addr = self.remote_connections.write().await;
        if !peer_map_by_client_addr.contains_key(client_addr) {
            let udpsock = UdpSocket::bind("0.0.0.0:0").await?;
            udpsock.connect(addr).await?;
            let udpsock = InnerUdpSocket::new(udpsock);
            peer_map_by_client_addr.insert(*client_addr, udpsock.clone());

            return Ok(udpsock);
        }

        Ok(peer_map_by_client_addr.get(client_addr).unwrap().clone())
    }

    /// Connects a UDP socket to a client peer denoted by addr. The addr
    /// argument must be a string slice that can be converted to a [`SocketAddr`] object. The
    /// bind_addr argument is an Option containing a string slice that specifies the
    /// local address the socket should bind to. If it is None the method will bind
    /// to a random local address.
    /// # Returns
    /// On success the unit type () is returned. Otherwise a RelayPortError error is returned.
    ///
    async fn new_client_connection(
        &self,
        addr: &SocketAddr,
        bind_addr: Option<&SocketAddr>,
    ) -> Result<InnerUdpSocket, RelayPortError> {
        let mut client_map = self.client_connections.write().await;
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
            let socket = InnerUdpSocket::new(socket);
            client_map.insert(*addr, socket.clone());

            return Ok(socket);
        }

        Ok(client_map.get(addr).unwrap().clone())
    }

    async fn spawn_bidirectional_relay(
        &self,
        client: InnerUdpSocket,
        remote: InnerUdpSocket,
        command: Receiver<RelayCommand>,
        cancel: tokio::sync::mpsc::Sender<InnerUdpSocket>,
    ) {
        let client = client.clone();
        let remote = remote.clone();

        tokio::spawn(async move {
            let _ = tokio::join!(
                unidirectional_relay(
                    client.clone(),
                    remote.clone(),
                    command.resubscribe(),
                    cancel.clone()
                ),
                unidirectional_relay(remote.clone(), client.clone(), command, cancel),
            );
        });
    }

    /// Remove an entry each from the global client and remote connection lists.
    ///
    /// When a [tokio::sync::mpsc] sender sends a [tokio::sync::mpsc::Receiver] message containing
    /// a socket this method will obtain the associated remote address and use it to
    /// remove the associated socket from our list of client and remote connections.
    async fn spawn_connection_cancel(
        &self,
        mut socket: tokio::sync::mpsc::Receiver<InnerUdpSocket>,
    ) {
        let c_connections = self.client_connections.clone();
        let r_connections = self.remote_connections.clone();
        tokio::task::spawn(async move {
            while let Some(socket) = socket.recv().await {
                let mut connections = c_connections.write().await;
                if let Ok(peer_addr) = socket.0.peer_addr() {
                    debug!("cancel connection with: {peer_addr}");
                    if connections.contains_key(&peer_addr) {
                        connections.remove(&peer_addr);
                    }
                }

                let mut connections = r_connections.write().await;
                if let Ok(peer_addr) = socket.0.peer_addr() {
                    if connections.contains_key(&peer_addr) {
                        connections.remove(&peer_addr);
                    }
                }
            }
        });
    }
}

impl InnerUdpSocket {
    pub fn new(socket: UdpSocket) -> Self {
        Self(Arc::new(socket))
    }
}

#[tracing::instrument(level = "debug", skip_all, err, ret, fields(from_addr, to_addr))]
async fn unidirectional_relay(
    src: InnerUdpSocket,
    dst: InnerUdpSocket,
    mut command: Receiver<RelayCommand>,
    cancel: tokio::sync::mpsc::Sender<InnerUdpSocket>,
) -> Result<usize, RelayPortError> {
    let from_addr = src.0.peer_addr().unwrap();
    let to_addr = dst.0.peer_addr().unwrap();

    let mut xfer_bytes = 0;
    let mut buf = [0u8; MAX_PKT_SIZE];
    let src_ = src.clone();
    let dst_ = dst.clone();
    let copy_reader_to_writer = async move {
        while let Ok(len) = src_.0.recv(&mut buf).await {
            if len == 0 {
                break;
            }
            let result = dst_.0.send(&buf[0..len]).await;
            if result.is_err() {
                let e = result.err().unwrap();
                eprintln!("failed to relay packet: {from_addr} -> {to_addr}: {e}");
                return Err(e);
            } else {
                let len = result.ok().unwrap();
                xfer_bytes += len;
            }
        }

        Ok(xfer_bytes)
    };
    let await_shutdown = command.recv();
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
        _ = sleep(UDP_TTL) => {
            // The peer address from either src or dst will match to enable us to
            // remove the socket from the client/remote connections lists. So, send the
            // cancel command using both.
            cancel
                .send(src)
                .await
                .map_err(|e| RelayPortError::Unknown(format!("{e}")))?;
            cancel
                .send(dst)
                .await
                .map_err(|e| RelayPortError::Unknown(format!("{e}")))?;
        }
    }

    match read_bytes {
        Ok(bytes) => {
            debug!("Transferred {bytes} bytes: {from_addr} -> {to_addr}");
            Ok(bytes)
        }
        Err(e) => Err(RelayPortError::IoError(e)),
    }
}
