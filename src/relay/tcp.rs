//! Abstractions that relay TCP ports

use std::io::ErrorKind;
use std::net::SocketAddr;

use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio::sync::broadcast::Receiver;
use tracing::debug;

use crate::command::RelayCommand;
use crate::RelayPortError;

/// Abstraction for initiating the relay builder.
#[derive(Debug)]
pub struct RelaySocket {}

/// Builder abstraction for composing a TCP relay.
#[derive(Copy, Clone, Debug)]
pub struct RelaySocketBuilder(RelayInner);

/// Abstraction containing a socket bound to a local address.
#[derive(Debug)]
pub struct BoundRelaySocket(TcpSocket);

/// Abstraction representing a socket listening for connections on a local address and port.
#[derive(Debug)]
pub struct RelayListener(TcpListener);

/// Abstraction representing a TCP stream.
/// Represents a TCP stream from a BoundRelaySocket connected to a remote peer.
#[derive(Debug)]
pub struct RelayStream(TcpStream);

#[derive(Copy, Clone, Debug)]
struct RelayInner {
    tcp_nodelay: bool,
    so_reuseaddr: bool,
}

impl RelaySocket {
    /// Obtain a builder object to begin composing the relay socket.
    ///
    /// # Returns
    /// It returns a [RelaySocketBuilder] object.
    ///
    /// # Example
    /// ```
    /// use relayport_rs::RelayTcpSocket;
    ///
    /// let builder = RelayTcpSocket::build();
    ///
    /// // configure builder
    /// ```
    pub fn build() -> RelaySocketBuilder {
        RelaySocketBuilder(RelayInner {
            tcp_nodelay: false,
            so_reuseaddr: false,
        })
    }
}

impl RelaySocketBuilder {
    /// Binds a TCP socket to addr.
    ///
    /// The addr argument must be string slice that can be converted into a local addresss and port. Unlike
    /// its [RelayUdpSocket] counterpart this method is synchronous.
    ///
    /// # Returns
    /// This method returns a [BoundRelaySocket] wrapped in a [Result]. If the operation fails for any reason the
    /// Err() value will contain the reason for the failure.
    ///
    /// # Example
    /// ```
    /// use relayport_rs::RelayTcpSocket;
    ///
    /// let bound_relay = RelayTcpSocket::build()
    ///     .bind("127.0.0.1:10443")
    ///     .unwrap();
    /// ```
    pub fn bind(self, addr: &str) -> Result<BoundRelaySocket, RelayPortError> {
        let addr = self.parse_address(addr)?;
        let socket = self.create_socket(&addr)?;
        socket.bind(addr)?;

        Ok(BoundRelaySocket(socket))
    }

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
    /// use relayport_rs::{RelayPortError, RelayTcpSocket, RelayTcpStream};
    ///
    /// # #[tokio::main]
    /// # pub async fn main() -> Result<(), RelayPortError>  {
    ///     let relay_stream = RelayTcpSocket::build()
    ///         .connect("127.0.0.1:443", None)
    ///         .await?;
    ///
    /// #     Ok(())
    /// # }
    ///
    /// ```
    pub async fn connect(
        self,
        addr: &str,
        bind_addr: Option<&str>,
    ) -> Result<RelayStream, RelayPortError> {
        let local_addr: Option<SocketAddr>;
        if let Some(str_addr) = bind_addr {
            local_addr = Some(self.parse_address(str_addr)?);
        } else {
            local_addr = None;
        }
        let addr = self.parse_address(addr)?;
        let socket = self.create_socket(&addr)?;
        if let Some(local_addr) = local_addr {
            socket.bind(local_addr)?;
        }
        let stream = socket.connect(addr).await?;

        Ok(RelayStream(stream))
    }

    /// Same as connect() but the caller provide the peer address as a SocketAddr.
    pub async fn connect_addr(
        self,
        addr: SocketAddr,
        bind_addr: Option<SocketAddr>,
    ) -> Result<RelayStream, RelayPortError> {
        let socket = self.create_socket(&addr)?;
        if let Some(local_addr) = bind_addr {
            socket.bind(local_addr)?;
        }
        let stream = socket.connect(addr).await?;

        Ok(RelayStream(stream))
    }

    /// Change the SO_REUSEADDR option on the TCP socket. If reuseaddr is `true` the option will
    /// be set on the created socket. If it is `false` the option will be disabled.
    /// # Returns
    /// The [RelayTcpSocketBuilder] obeject with the configuration set according to the reuseaddr argument.
    ///
    /// ```
    /// use relayport_rs::{RelayPortError, RelayTcpSocket};
    /// # #[tokio::main]
    /// # pub async fn main() -> Result<(), RelayPortError>  {
    ///     let mut builder = RelayTcpSocket::build();
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

    /// Change the TCP_NODELAY option on the TCP socket. If nodelay is `true` the option will
    /// be set on the created socket. If it is `false` the option will be disabled.
    /// # Returns
    /// The [RelayTcpSocketBuilder] obeject with the configuration set according to the nodelay argument.
    ///
    /// ```
    /// use relayport_rs::{RelayPortError, RelayTcpSocket};
    /// # #[tokio::main]
    /// # pub async fn main() -> Result<(), RelayPortError>  {
    ///     let mut builder = RelayTcpSocket::build();
    ///     builder.set_tcp_nodelay(true);
    ///
    ///     assert_eq!(builder.tcp_nodelay(), true);
    ///
    /// #     Ok(())
    /// # }
    ///
    /// ```
    pub fn set_tcp_nodelay(&mut self, nodelay: bool) -> &mut Self {
        self.0.tcp_nodelay = nodelay;

        self
    }

    /// Get the builder configuration for setting SO_REUSEADDR on the TCP socket.
    pub fn so_reuseaddr(&self) -> bool {
        self.0.so_reuseaddr
    }

    /// Get the builder configuratioin for setting TCP_NODELAY on the TCP socket.
    pub fn tcp_nodelay(&self) -> bool {
        self.0.tcp_nodelay
    }

    fn create_socket(&self, addr: &SocketAddr) -> Result<TcpSocket, RelayPortError> {
        let socket = if addr.is_ipv4() {
            TcpSocket::new_v4()?
        } else {
            TcpSocket::new_v6()?
        };
        if self.0.so_reuseaddr {
            socket.set_reuseaddr(true)?;
        }
        if self.0.tcp_nodelay {
            socket.set_nodelay(true)?;
        }

        Ok(socket)
    }

    fn parse_address(&self, addr: &str) -> Result<SocketAddr, RelayPortError> {
        addr.parse::<SocketAddr>()
            .map_err(RelayPortError::ParseAddrError)
    }
}

impl BoundRelaySocket {
    /// Listen on a socket already bound to a local Ip address.
    /// # Returns
    /// If successful a [RelayListener] object is returned. If it encountered an error a [RelayPortError]
    /// is returned.
    ///
    /// # Example
    /// ```no_run
    /// use relayport_rs::{RelayPortError, RelayTcpSocket};
    ///
    /// # #[tokio::main]
    /// # pub async fn main() -> Result<(), RelayPortError>  {
    ///     let listener = RelayTcpSocket::build()
    ///         .bind("127.0.0.1:10443")?
    ///         .listen()?;
    ///
    ///     // Spawn a task to accept and dispatch a connection
    /// #     Ok(())
    /// # }
    ///
    /// ```
    pub fn listen(self) -> Result<RelayListener, RelayPortError> {
        let listener = self.0.listen(1024)?;

        Ok(RelayListener(listener))
    }
}

impl RelayListener {
    /// Start relaying incomming TCP traffic to the remote peer.
    ///
    /// Wait for a connection to come in on a RelayListener, accept the connection, and
    /// begin relaying traffic from the client to the remote peer and vice versa. After an
    /// incomming connection is accepted this method will spawn a [tokio::task] to handle relaying
    /// traffic. The spawned task will be a bi-directional relay that will handle traffic
    /// in both directions asynchronously. The caller is responsible for creating a
    /// [tokio::sync::broadcast] chanel and passing the receiver half to this method. The
    /// chanel can be used by the caller to send [RelayCommand] objects to the spawned task.
    /// Currently, the only command supported by the library is a shutdown command to instruct
    /// the task to close the TCP socket and exit cleanly.
    ///
    /// # Returns
    /// This method will not return unless a [RelayCommand::Shutdown] is sent by the caller or
    /// it encounters an error. If it encounters an error it will return it as a RelayPortError.
    ///
    /// # Example
    /// ```no_run
    /// # use std::error::Error;
    /// use tokio::sync::broadcast;
    /// use relayport_rs::{RelayCommand, RelayPortError, RelayTcpSocket};
    ///
    /// # #[tokio::main]
    /// # pub async fn main() -> Result<(), Box<dyn Error>>  {
    ///     // The relay expects a broadcast channel on which to listen for shutdown commands
    ///     let (tx, rx) = broadcast::channel(16);
    ///
    ///     let listener = RelayTcpSocket::build()
    ///         .bind("127.0.0.1:10443")?
    ///         .listen()?;
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
        peer: &str,
        cancel: &Receiver<RelayCommand>,
    ) -> Result<(), RelayPortError> {
        let relay_addr = peer.parse().map_err(RelayPortError::ParseAddrError)?;
        loop {
            let mut cancel = cancel.resubscribe();
            let (client_stream, client_addr) = self.0.accept().await?;
            debug!("accepted connection from {}", client_addr);
            tokio::select! {
                biased;
                _ = self.spawn_bidirectional_relay(client_stream, relay_addr, cancel.resubscribe()) => { continue }
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

    async fn spawn_bidirectional_relay(
        &self,
        mut client_stream: TcpStream,
        relay_addr: SocketAddr,
        cancel: Receiver<RelayCommand>,
    ) -> Result<(), RelayPortError> {
        let mut relay = RelaySocket::build()
            .set_so_reuseaddr(true)
            .set_tcp_nodelay(true)
            .connect_addr(relay_addr, None)
            .await?;
        let handle = tokio::spawn(async move {
            let (mut client_r, mut client_w) = client_stream.split();
            let (mut remote_r, mut remote_w) = relay.0.split();
            let (xfer_client, xfer_relay) = tokio::join!(
                single_direction_relay(&mut client_r, &mut remote_w, cancel.resubscribe()),
                single_direction_relay(&mut remote_r, &mut client_w, cancel),
            );
            match xfer_client {
                Ok(count) => debug!("{count} bytes relayed from client to remote"),
                Err(e) => return Err(e),
            }
            match xfer_relay {
                Ok(count) => debug!("{count} bytes relayed from remote to client"),
                Err(e) => return Err(e),
            }

            Ok(())
        });

        match tokio::join!(handle) {
            (Ok(_),) => Ok(()),
            (Err(e),) => Err(RelayPortError::Unknown(e.to_string())),
        }
    }
}

#[tracing::instrument(level = "debug", skip_all, err, ret, fields(_from_addr, _to_addr))]
async fn single_direction_relay(
    src: &mut ReadHalf<'_>,
    dst: &mut WriteHalf<'_>,
    mut rx: Receiver<RelayCommand>,
) -> Result<usize, RelayPortError> {
    let _from_addr = src.peer_addr().unwrap();
    let _to_addr = dst.peer_addr().unwrap();
    let copy_reader_to_writer = tokio::io::copy(src, dst);
    let await_shutdown = rx.recv();
    let mut read_bytes = Ok(0);
    tokio::select! {
        biased;
            result = copy_reader_to_writer => {
                read_bytes = result.or_else(|e| match e.kind() {
                    ErrorKind::ConnectionReset => Ok(0),
                    _ => Err(e),
                })
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
            let _ = dst.shutdown().await;
            Ok(bytes as usize)
        }
        Err(e) => Err(RelayPortError::IoError(e)),
    }
}
