use relayport_rs::{RelayCommand, RelayTcpSocket, RelayUdpSocket};
use tokio::sync::broadcast::Receiver;

#[derive(Clone, Debug)]
pub struct RelayTcp {
    bind_addr: String,
    remote: String,
}

impl RelayTcp {
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
        let relay = RelayTcpSocket::build()
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
                .run(&relay_addr, &rx)
                .await
                .expect("failed to start relay")
        });
    }
}

#[derive(Clone, Debug)]
pub struct RelayUdp {
    bind_addr: String,
    remote: String,
}

impl RelayUdp {
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
        let relay = RelayUdpSocket::build()
            .set_so_reuseaddr(true)
            .bind(&listen_addr)
            .await
            .expect(&format!("failed to bind address: {}", listen_addr));
        let _ = tokio::task::spawn(async move {
            relay
                .run(&relay_addr, &rx)
                .await
                .expect("failed to start relay")
        });
    }
}
