use relayport_rs::{RelayCommand, RelaySocket};
use tokio::sync::broadcast::Receiver;

#[derive(Clone, Debug)]
pub struct Relay {
    bind_addr: String,
    remote: String,
}

impl Relay {
    pub fn new(bind_addr: &str, remote: &str) -> Self {
        Self {
            bind_addr: bind_addr.to_string(),
            remote: remote.to_string(),
        }
    }

    pub async fn spawn(&self, rx: Receiver<RelayCommand>) {
        println!("START spawn_relay()");
        let listen_addr = self.bind_addr.clone();
        let relay_addr = self.remote.clone();
        let relay = RelaySocket::build()
            .set_so_reuseaddr(true)
            .set_tcp_nodelay(true)
            .bind(&listen_addr)
            .expect(&format!("failed to bind to {}", listen_addr))
            .listen()
            .expect(&format!(
                "failed to listen to bound address: {}",
                listen_addr
            ));
        let _ = tokio::task::spawn(async move {
            relay
                .serve(&relay_addr, &rx)
                .await
                .expect("failed to start relay")
        });
    }
}
