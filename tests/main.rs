use relayport_rs::{command::RelayCommand, RelaySocket};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{
        broadcast::{self, Receiver},
        oneshot,
    },
};
use tracing::{debug, info};
use tracing_subscriber;

#[derive(Clone, Debug)]
pub struct BufWrapper {
    inner: Box<[u8]>,
}

impl BufWrapper {
    pub fn new() -> Self {
        Self {
            inner: Box::new([0u8; 1518]),
        }
    }

    pub fn as_ref(&self) -> &[u8] {
        &self.inner.as_ref()
    }

    pub fn as_mut_ref(&mut self) -> &mut [u8] {
        self.inner.as_mut()
    }

    pub fn write(&mut self, byte: u8, count: usize) {
        for i in 0..count {
            self.inner[i] = byte;
        }
    }
}

async fn spawn_server(
    mut buf: BufWrapper,
    mut rx: Receiver<RelayCommand>,
    output: tokio::sync::oneshot::Sender<BufWrapper>,
) {
    debug!("START spawn_server()");
    let addr = "127.0.0.1:20100";
    tokio::task::spawn(async move {
        debug!("Server listening on {addr}");
        let listener = TcpListener::bind(&addr)
            .await
            .expect(&format!("test server failed to bind to {}", addr));
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("listening server failed to accept incomming connection");
        debug!("Server: accepted connection");
        loop {
            let count;
            debug!("Server: reading from socket");
            tokio::select! {
                biased;
                result = stream.read(buf.as_mut_ref()) => {
                    count = result.or_else(|e| match e.kind() {
                        _ => { info!("failed to read from tcp stream: {}", e); Err(e) }
                    }).expect("Server: failed to read from socket");
                },
                _ = rx.recv() => { debug!("Server: recieved cancel signal"); break; }
            }

            if count == 0 {
                debug!("Server: received 0 bytes");
                break;
            } else {
                debug!("Server: received {count} bytes");
                break;
            }
        }
        output.send(buf).expect("failed to send received buffer");
    });
}

async fn spawn_relay(rx: Receiver<RelayCommand>) {
    debug!("START spawn_relay()");
    let listen_addr = "127.0.0.1:10099";
    let relay_addr = "127.0.0.1:20100";
    let relay = RelaySocket::build()
        .set_so_reuseaddr(true)
        .set_tcp_nodelay(true)
        .bind(listen_addr)
        .expect(&format!("failed to bind to {}", listen_addr))
        .listen()
        .expect(&format!(
            "failed to listen to bound address: {}",
            listen_addr
        ));
    let _ = tokio::task::spawn(async move {
        relay
            .accept_and_relay(relay_addr, &rx)
            .await
            .expect("failed to start relay")
    });
}

async fn spawn_client(buf: BufWrapper) {
    debug!("START spawn_client()");
    let remote = "127.0.0.1:10099";
    let _ = tokio::task::spawn(async move {
        debug!("Client connecting to {remote}");
        let mut stream = TcpStream::connect(remote)
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
        debug!("Client: wrote buffer");
        let _ = stream.shutdown().await;
    });
}

#[tokio::test]
async fn client_to_server() {
    tracing_subscriber::fmt::init();
    let (tx, rx) = broadcast::channel(1);
    let (output_sender, output_receiver) = oneshot::channel::<BufWrapper>();
    let mut buf_client = BufWrapper::new();
    buf_client.write(56u8, 100);
    let buf_server = BufWrapper::new();
    spawn_server(buf_server, tx.subscribe(), output_sender).await;
    spawn_relay(rx).await;
    spawn_client(buf_client.clone()).await;

    let buf_server = output_receiver
        .await
        .expect("failed to receive server buffer");

    println!("Sending shutdown cmd ...");
    tx.send(RelayCommand::Shutdown)
        .expect("failed to send shutdown command to relay");

    assert_eq!(
        buf_server.as_ref(),
        buf_client.as_ref(),
        "The contents of the buffer received by the server are identical to that sent by the client");
}
