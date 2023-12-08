use relayport_rs::RelayCommand;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UdpSocket},
    sync::{broadcast::Receiver, oneshot::Sender},
};

use crate::helpers::bufwrapper::BufWrapper;

#[derive(Debug)]
pub struct TestTcpServer {
    bind_addr: String,
}

impl TestTcpServer {
    pub fn new(addr: &str) -> Self {
        Self {
            bind_addr: addr.to_string(),
        }
    }

    pub async fn spawn_reader(
        &self,
        mut buf: BufWrapper,
        mut rx: Receiver<RelayCommand>,
        output: Sender<BufWrapper>,
    ) {
        println!("START Server {}: spawn_reader()", self.bind_addr);
        let addr = self.bind_addr.clone();
        tokio::task::spawn(async move {
            println!("Server listening on {addr}");
            let listener = TcpListener::bind(&addr)
                .await
                .expect(&format!("test server failed to bind to {}", addr));
            let (mut stream, _) = listener
                .accept()
                .await
                .expect("listening server failed to accept incomming connection");
            println!("Server {addr}: accepted connection");
            loop {
                let count;
                println!("Server {addr}: reading from socket");
                tokio::select! {
                    biased;
                    result = stream.read(buf.as_mut_ref()) => {
                        count = result.or_else(|e| match e.kind() {
                            _ => { println!("failed to read from tcp stream: {}", e); Err(e) }
                        }).expect("Server: failed to read from socket");
                    },
                    _ = rx.recv() => { println!("Server {addr}: recieved cancel signal"); break; }
                }

                if count == 0 {
                    println!("Server {addr}: received 0 bytes");
                    break;
                } else {
                    println!("Server {addr}: received {count} bytes");
                    break;
                }
            }
            output.send(buf).expect("failed to send received buffer");
        });
    }

    pub async fn spawn_writer(&self, buf: BufWrapper) {
        println!("START Server {}: spawn_writer()", self.bind_addr);
        let addr = self.bind_addr.clone();
        tokio::task::spawn(async move {
            println!("Server listening on {addr}");
            let listener = TcpListener::bind(&addr)
                .await
                .expect(&format!("test server failed to bind to {}", addr));
            let (mut stream, _) = listener
                .accept()
                .await
                .expect("listening server failed to accept incomming connection");
            println!("Server {addr}: accepted connection");
            let _ = stream
                .write_all(buf.as_ref())
                .await
                .expect("write to server socket failed");
            println!("Server {addr}: wrote buffer");
            let _ = stream.shutdown().await;
        });
    }
}

#[derive(Debug)]
pub struct TestUdpServer {
    bind_addr: String,
}

impl TestUdpServer {
    pub fn new(addr: &str) -> Self {
        Self {
            bind_addr: addr.to_string(),
        }
    }

    pub async fn spawn_reader(
        &self,
        mut buf_read: BufWrapper,
        mut rx: Receiver<RelayCommand>,
        output: Sender<BufWrapper>,
    ) {
        println!("START UDP Server {}: spawn_reader()", self.bind_addr);
        let addr = self.bind_addr.clone();
        tokio::task::spawn(async move {
            println!("UDP Server listening on {addr}");
            let listener = UdpSocket::bind(&addr)
                .await
                .expect(&format!("test udp server failed to bind to {}", addr));
            println!("UDP Server {addr}: received a connection");
            loop {
                let count;
                println!("Server {addr}: reading from socket");
                tokio::select! {
                    biased;
                    result = listener.recv(buf_read.as_mut_ref()) => {
                        count = result.or_else(|e| match e.kind() {
                            _ => { println!("failed to read from UDP: {}", e); Err(e) }
                        }).expect("UDP Server: failed to read from socket");
                    },
                    _ = rx.recv() => { println!("Server {addr}: recieved cancel signal"); break; }
                }

                if count == 0 {
                    println!("Server {addr}: received 0 bytes");
                    break;
                } else {
                    println!("Server {addr}: received {count} bytes");
                    break;
                }
            }
            output
                .send(buf_read)
                .expect("failed to send received buffer");
        });
    }

    pub async fn spawn_writer(&self, buf: BufWrapper) {
        println!("START Server {}: spawn_writer()", self.bind_addr);
        let addr = self.bind_addr.clone();
        tokio::task::spawn(async move {
            println!("Server listening on {addr}");
            let listener = TcpListener::bind(&addr)
                .await
                .expect(&format!("test server failed to bind to {}", addr));
            let (mut stream, _) = listener
                .accept()
                .await
                .expect("listening server failed to accept incomming connection");
            println!("Server {addr}: accepted connection");
            let _ = stream
                .write_all(buf.as_ref())
                .await
                .expect("write to server socket failed");
            println!("Server {addr}: wrote buffer");
            let _ = stream.shutdown().await;
        });
    }

    pub async fn spawn(
        &self,
        mut buf: BufWrapper,
        mut rx: Receiver<RelayCommand>,
        output: Sender<BufWrapper>,
    ) {
        println!("START UDP Server {}: spawn()", self.bind_addr);
        let addr = self.bind_addr.clone();
        tokio::task::spawn(async move {
            println!("UDP Server listening on {addr}");
            let listener = UdpSocket::bind(&addr)
                .await
                .expect(&format!("test udp server failed to bind to {}", addr));
            loop {
                let count;
                let peer_addr;
                println!("Server {addr}: reading from socket...");
                tokio::select! {
                    biased;
                    result = listener.recv_from(buf.as_mut_ref()) => {
                        (count, peer_addr) = result.or_else(|e| match e.kind() {
                            _ => { println!("failed to read from UDP: {}", e); Err(e) }
                        }).expect("UDP Server: failed to read from socket");
                    },
                    _ = rx.recv() => { println!("Server {addr}: recieved cancel signal"); break; }
                }

                if count == 0 {
                    println!("Server {addr}: received 0 bytes");
                    break;
                } else {
                    println!("UDP Server {addr}: received {count} bytes");
                    let count = listener
                        .send_to(&buf.as_ref()[0..count], peer_addr)
                        .await
                        .expect("udp server failed to send input back to client");
                    println!("UDP Server {addr}: wrote back to client {count} bytes");
                    output.send(buf).expect("failed to send received buffer");
                    break;
                }
            }
        });
    }
}
