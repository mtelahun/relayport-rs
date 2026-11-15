use relayport_rs::RelayCommand;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
    sync::{broadcast::Receiver, oneshot::Sender},
};

use crate::helpers::bufwrapper::BufWrapper;

use super::TestOp;

#[derive(Debug)]
pub struct TestUdpClient {
    remote: String,
}

#[derive(Debug)]
pub struct TestTcpClient {
    remote: String,
}

impl TestTcpClient {
    pub fn new(remote: &str) -> Self {
        Self {
            remote: remote.to_string(),
        }
    }

    pub async fn spawn(
        self,
        op: TestOp,
        write_buf: BufWrapper,
        mut command: Receiver<RelayCommand>,
        output: Sender<BufWrapper>,
    ) {
        println!("START Test TCP Client -> {}: spawn()", self.remote);
        let mut read_buf = BufWrapper::new();
        let remote = self.remote.clone();
        tokio::task::spawn(async move {
            println!("Client connecting to {remote}");
            let mut stream = TcpStream::connect(remote.clone())
                .await
                .expect("client failed to connect to server");
            if op == TestOp::Write {
                let len = stream
                    .write(write_buf.exact_slice())
                    .await
                    .expect("tcp write to server socket failed");
                println!("TCP {remote}: wrote {len} bytes to buffer");
            }
            'read: {
                let count;
                println!("Client reader: reading from socket");
                tokio::select! {
                    biased;
                    result = stream.read(read_buf.as_mut_ref()) => {
                        count = result.map_err(|e| { println!("failed to read from tcp stream: {}", e); e }).expect("Server: failed to read from socket");
                    },
                    _ = command.recv() => { println!("Server: recieved cancel signal"); break 'read; }
                }

                if count == 0 {
                    println!("Client reader: received 0 bytes");
                    break 'read;
                } else {
                    println!("Client reader: received {count} bytes");
                    output
                        .send(read_buf)
                        .expect("failed to send received buffer");
                    break 'read;
                }
            }
        });
    }
}

impl TestUdpClient {
    pub fn new(remote: &str) -> Self {
        Self {
            remote: remote.to_string(),
        }
    }

    pub async fn spawn(
        &self,
        op: TestOp,
        write_buf: BufWrapper,
        mut command: Receiver<RelayCommand>,
        output: Sender<BufWrapper>,
    ) {
        println!("START UDP Client {}: spawn()", self.remote);
        let remote = self.remote.clone();
        let mut read_buf = BufWrapper::new();
        tokio::task::spawn(async move {
            println!("UDP Client connecting to {remote}");
            let socket = UdpSocket::bind("0.0.0.0:0")
                .await
                .expect("udp client failed to connect to server");
            socket
                .connect(&remote)
                .await
                .unwrap_or_else(|_| panic!("udp client failed to connect to {remote}"));
            if op == TestOp::Write {
                let len = socket
                    .send(write_buf.exact_slice())
                    .await
                    .expect("udp write to server socket failed");
                println!("UDP Client {remote}: wrote {len} bytes to buffer");
            }
            'read: {
                let count;
                println!("UDP Client: reading from socket");
                tokio::select! {
                    biased;
                    result = socket.recv(read_buf.as_mut_ref()) => {
                        count = result.map_err(|e| { println!("failed to read from tcp stream: {}", e); e }).expect("Server: failed to read from socket");
                    },
                    _ = command.recv() => { println!("Server: recieved cancel signal"); break 'read; }
                }

                if count == 0 {
                    println!("Client reader: received 0 bytes");
                    break 'read;
                } else {
                    println!("Client reader: received {count} bytes");
                    output
                        .send(read_buf)
                        .expect("failed to send received buffer");
                    break 'read;
                }
            }
        });
    }
}
