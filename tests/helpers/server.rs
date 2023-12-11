use relayport_rs::RelayCommand;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UdpSocket},
    sync::{broadcast::Receiver, oneshot::Sender},
};

use crate::helpers::bufwrapper::BufWrapper;

use super::TestOp;

#[derive(Debug)]
pub struct TestTcpServer {
    bind_addr: String,
}

#[derive(Debug)]
pub struct TestUdpServer {
    bind_addr: String,
}

impl TestTcpServer {
    pub fn new(addr: &str) -> Self {
        Self {
            bind_addr: addr.to_string(),
        }
    }

    pub async fn spawn(
        &self,
        op: TestOp,
        write_buf: BufWrapper,
        mut rx: Receiver<RelayCommand>,
        output: Sender<BufWrapper>,
    ) {
        println!("START Server {}: spawn_reader()", self.bind_addr);
        let mut read_buf = BufWrapper::new();
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
                    result = stream.read(read_buf.as_mut_ref()) => {
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
                    if op == TestOp::Write {
                        let count = stream
                            .write(&write_buf.as_ref()[0..count])
                            .await
                            .expect("udp server failed to send input back to client");
                        println!("UDP Server {addr}: wrote back to client {count} bytes");
                    }
                    break;
                }
            }
            output
                .send(read_buf)
                .expect("failed to send received buffer");
        });
    }
}

impl TestUdpServer {
    pub fn new(addr: &str) -> Self {
        Self {
            bind_addr: addr.to_string(),
        }
    }

    pub async fn spawn(
        &self,
        op: TestOp,
        write_buf: BufWrapper,
        mut rx: Receiver<RelayCommand>,
        output: Sender<BufWrapper>,
    ) {
        println!("START UDP Server {}: spawn()", self.bind_addr);
        let addr = self.bind_addr.clone();
        let mut read_buf = BufWrapper::new();
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
                    result = listener.recv_from(read_buf.as_mut_ref()) => {
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
                    if op == TestOp::Write {
                        let count = listener
                            .send_to(&write_buf.as_ref()[0..count], peer_addr)
                            .await
                            .expect("udp server failed to send input back to client");
                        println!("UDP Server {addr}: wrote back to client {count} bytes");
                    }
                    output
                        .send(read_buf)
                        .expect("failed to send received buffer");
                    break;
                }
            }
        });
    }
}
