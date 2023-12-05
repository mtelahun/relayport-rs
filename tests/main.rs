use relayport_rs::RelayCommand;
use tokio::sync::{broadcast, oneshot};

use crate::helpers::{
    bufwrapper::BufWrapper, client::TestClient, relay::Relay, server::TestServer,
};

pub mod helpers;

#[tokio::test]
async fn client_to_server() {
    let (tx, rx) = broadcast::channel(1);
    let mut buf_write = BufWrapper::new();
    buf_write.write(56u8, 100);
    let buf_read = BufWrapper::new();

    let output_receiver = spawn_all(
        "127.0.0.1:10099",
        "127.0.0.1:20100",
        buf_read,
        buf_write.clone(),
        rx,
    )
    .await;

    let buf_result = output_receiver
        .await
        .expect("failed to receive server buffer");
    println!("client_to_server: Sending shutdown cmd ...");
    tx.send(RelayCommand::Shutdown)
        .expect("failed to send shutdown command to relay");

    assert_eq!(
        buf_result.as_ref(),
        buf_write.as_ref(),
        "The contents of the buffer received by the server are identical to that sent by the client");
}

#[tokio::test]
async fn server_to_client() {
    let (tx, rx) = broadcast::channel(1);
    let mut buf_write = BufWrapper::new();
    buf_write.write(56u8, 100);
    let buf_read = BufWrapper::new();

    let output_receiver = spawn_all(
        "127.0.0.1:11099",
        "127.0.0.1:21100",
        buf_read,
        buf_write.clone(),
        rx,
    )
    .await;

    let buf_result = output_receiver
        .await
        .expect("failed to receive output buffer");

    println!("server_to_client: Sending shutdown cmd ...");
    tx.send(RelayCommand::Shutdown)
        .expect("failed to send shutdown command to relay");

    assert_eq!(
        buf_result.as_ref(),
        buf_write.as_ref(),
        "The contents of the buffer received by the client are identical to that sent by the server");
}

async fn spawn_all(
    relay_addr: &str,
    server_addr: &str,
    buf_read: BufWrapper,
    buf_write: BufWrapper,
    rx: tokio::sync::broadcast::Receiver<RelayCommand>,
) -> tokio::sync::oneshot::Receiver<BufWrapper> {
    let (output_sender, output_receiver) = oneshot::channel::<BufWrapper>();
    let client = TestClient::new(relay_addr);
    let relay = Relay::new(relay_addr, server_addr);
    let server = TestServer::new(server_addr);
    server
        .spawn_reader(buf_read, rx.resubscribe(), output_sender)
        .await;
    relay.spawn(rx).await;
    client.spawn_writer(buf_write).await;

    output_receiver
}
