use relayport_rs::RelayCommand;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{broadcast::Receiver, oneshot::Sender},
};

use crate::helpers::bufwrapper::BufWrapper;

#[derive(Debug)]
pub struct TestServer {
    bind_addr: String,
}

impl TestServer {
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
