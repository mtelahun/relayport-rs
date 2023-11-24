use std::io::ErrorKind;
use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio::sync::broadcast::{Receiver, Sender};
use tracing::{debug, trace};

use crate::command::RelayCommand;
use crate::error::RelayPortError;

const MTU: usize = 1518;

#[derive(Debug)]
pub struct RelaySocket {}

#[derive(Copy, Clone, Debug)]
pub struct RelaySocketBuilder(RelayInner);

#[derive(Debug)]
pub struct BoundRelaySocket(TcpSocket);

#[derive(Debug)]
pub struct RelayListener(TcpListener);

#[derive(Debug)]
pub struct RelayStream(TcpStream);

#[derive(Copy, Clone, Debug)]
struct RelayInner {
    tcp_nodelay: bool,
    so_reuseaddr: bool,
}

impl RelaySocket {
    pub fn build() -> RelaySocketBuilder {
        RelaySocketBuilder(RelayInner {
            tcp_nodelay: false,
            so_reuseaddr: false,
        })
    }
}

impl RelaySocketBuilder {
    pub fn bind(self, addr: &str) -> Result<BoundRelaySocket, RelayPortError> {
        let addr = self.parse_address(addr)?;
        let socket = self.create_socket(&addr)?;
        socket.bind(addr)?;

        Ok(BoundRelaySocket(socket))
    }

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

    pub fn set_so_reuseaddr(&mut self, reuseaddr: bool) -> Result<&mut Self, RelayPortError> {
        self.0.so_reuseaddr = reuseaddr;

        Ok(self)
    }

    pub fn set_tcp_nodelay(&mut self, nodelay: bool) -> Result<&mut Self, RelayPortError> {
        self.0.tcp_nodelay = nodelay;

        Ok(self)
    }

    pub fn so_reuseaddr(&self) -> bool {
        self.0.so_reuseaddr
    }

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
    pub fn listen(self) -> Result<RelayListener, RelayPortError> {
        let listener = self.0.listen(1024)?;

        Ok(RelayListener(listener))
    }
}

impl RelayListener {
    pub async fn accept_and_relay(
        &self,
        target: &str,
        cancel_sender: &Sender<RelayCommand>,
    ) -> Result<(), RelayPortError> {
        let relay_addr = target.parse().map_err(RelayPortError::ParseAddrError)?;
        loop {
            let mut cancel = cancel_sender.subscribe();
            let (client_stream, client_addr) = self.0.accept().await?;
            debug!("accepted connection from {}", client_addr);
            tokio::select! {
                biased;
                _ = self.process_connection(client_stream, relay_addr, cancel_sender.subscribe()) => { continue }
                result = cancel.recv() => {match result {
                    Ok(cmd) => match cmd {
                        RelayCommand::Shutdown => {
                            debug!("received shutdown relay command");
                            break
                        },
                    },
                    Err(e) => return Err(RelayPortError::InternalCommunicationError(e)),
                }}
            }
        }

        Ok(())
    }

    async fn process_connection(
        &self,
        mut client_stream: TcpStream,
        relay_addr: SocketAddr,
        cancel: Receiver<RelayCommand>,
    ) -> Result<(), RelayPortError> {
        let mut relay = RelaySocket::build()
            .set_so_reuseaddr(true)?
            .set_tcp_nodelay(true)?
            .connect_addr(relay_addr, None)
            .await?;
        let handle = tokio::spawn(async move {
            let (mut client_r, mut client_w) = client_stream.split();
            let (mut relay_r, mut relay_w) = relay.0.split();
            let (xfer_client, xfer_relay) = tokio::join!(
                relay_inner(&mut client_r, &mut relay_w, cancel.resubscribe()),
                relay_inner(&mut relay_r, &mut client_w, cancel),
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
async fn relay_inner(
    read_sock: &mut ReadHalf<'_>,
    write_sock: &mut WriteHalf<'_>,
    mut rx: Receiver<RelayCommand>,
) -> Result<usize, RelayPortError> {
    let _from_addr = read_sock.peer_addr().unwrap();
    let _to_addr = write_sock.peer_addr().unwrap();
    let mut buf = vec![0u8; MTU];
    let mut relay_bytes = 0;
    loop {
        let read_bytes;
        tokio::select! {
            biased;
            result = read_sock.read(buf.as_mut()) => {
                read_bytes = result.or_else(|e| match e.kind() {
                    ErrorKind::ConnectionReset => Ok(0),
                    _ => Err(e),
                })
            }
            result = rx.recv() => { match result {
                Ok(cmd) => match cmd {
                    RelayCommand::Shutdown => {
                        debug!("received shutdown relay command");
                        break
                    },
                },
                Err(e) => return Err(RelayPortError::InternalCommunicationError(e)),
            }}
        }
        match read_bytes {
            Ok(bytes) => {
                write_sock.write_all(&buf[0..bytes]).await?;
                relay_bytes += bytes;
                trace!(bytes, total_bytes = relay_bytes);
            }
            Err(e) => return Err(RelayPortError::IoError(e)),
        }
    }

    Ok(relay_bytes)
}
