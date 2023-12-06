use relayport_rs::RelayCommand;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{broadcast::Receiver, oneshot::Sender},
};

use crate::helpers::bufwrapper::BufWrapper;

#[derive(Debug)]
pub struct TestClient {
    remote: String,
}

impl TestClient {
    pub fn new(remote: &str) -> Self {
        Self {
            remote: remote.to_string(),
        }
    }

    pub async fn spawn_reader(
        self,
        mut buf: BufWrapper,
        mut rx: Receiver<RelayCommand>,
        output: Sender<BufWrapper>,
    ) {
        println!("START Client {}: spawn_reader()", self.remote);
        let remote = self.remote.clone();
        let _ = tokio::task::spawn(async move {
            println!("Client connecting to {remote}");
            let mut stream = TcpStream::connect(remote)
                .await
                .expect("client failed to connect to server");
            loop {
                let count;
                println!("Client reader: reading from socket");
                tokio::select! {
                    biased;
                    result = stream.read(buf.as_mut_ref()) => {
                        count = result.or_else(|e| match e.kind() {
                            _ => { println!("failed to read from tcp stream: {}", e); Err(e) }
                        }).expect("Server: failed to read from socket");
                    },
                    _ = rx.recv() => { println!("Server: recieved cancel signal"); break; }
                }

                if count == 0 {
                    println!("Client reader: received 0 bytes");
                    break;
                } else {
                    println!("Client reader: received {count} bytes");
                    break;
                }
            }
            output.send(buf).expect("failed to send received buffer");
        });
    }

    pub async fn spawn_writer(&self, buf: BufWrapper) {
        println!("START Server {}: spawn_writer()", self.remote);
        let remote = self.remote.clone();
        let _ = tokio::task::spawn(async move {
            println!("Client connecting to {remote}");
            let mut stream = TcpStream::connect(&remote)
                .await
                .expect("client failed to connect to server");
            let _ = stream
                .write_all(buf.as_ref())
                .await
                .expect("write to server socket failed");
            let _ = stream
                .flush()
                .await
                .expect("Client: failed to flush the buffer to the socket");
            println!("Client {remote}: wrote buffer");
            let _ = stream.shutdown().await;
        });
    }
}
