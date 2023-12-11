use helpers::{client::TestUdpClient, relay::RelayUdp, server::TestUdpServer};
use relayport_rs::RelayCommand;
use tokio::sync::{broadcast, oneshot};

use crate::helpers::{
    bufwrapper::BufWrapper, client::TestTcpClient, relay::RelayTcp, server::TestTcpServer,
};

pub mod helpers;

#[tokio::test]
async fn tcp_client_to_server() {
    let (tx, rx) = broadcast::channel(1);
    let mut buf_write = BufWrapper::new();
    buf_write.write(56u8, 100);
    let buf_read = BufWrapper::new();

    let output_receiver = spawn_all_tcp(
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
async fn tcp_server_to_client() {
    let (tx, rx) = broadcast::channel(1);
    let mut buf_write = BufWrapper::new();
    buf_write.write(56u8, 100);
    let buf_read = BufWrapper::new();

    let output_receiver = spawn_all_tcp2(
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

#[tokio::test]
async fn udp_client_to_server() {
    let (tx, rx) = broadcast::channel(1);
    let mut buf_write = BufWrapper::new();
    buf_write.write(56u8, 100);
    let buf_read = BufWrapper::new();

    let output_receiver = spawn_all_udp(
        "127.0.0.1:30099",
        "127.0.0.1:30100",
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
async fn udp_server_to_client() {
    tracing_subscriber::fmt::init();
    let (tx, rx) = broadcast::channel(1);
    let (server_sender, server_receiver) = oneshot::channel::<BufWrapper>();
    let (client_sender, client_receiver) = oneshot::channel::<BufWrapper>();
    let mut buf_write = BufWrapper::new();
    buf_write.write(56u8, 100);
    let buf_read = BufWrapper::new();
    let (client, server) =
        spawn_all_udp2("127.0.0.1:32099", "127.0.0.1:32100", rx.resubscribe()).await;

    server
        .spawn(buf_read, rx.resubscribe(), server_sender)
        .await;
    client.spawn(buf_write.clone(), rx, client_sender).await;

    let buf_result_server = server_receiver
        .await
        .expect("failed to receive output buffer");

    let buf_result_client = client_receiver
        .await
        .expect("failed to receive output buffer");

    println!("server_to_client: Sending shutdown cmd ...");
    tx.send(RelayCommand::Shutdown)
        .expect("failed to send shutdown command to relay");

    assert_eq!(
        buf_result_server.as_ref(),
        buf_write.as_ref(),
        "The contents of the buffer received by the client are identical to that sent by the server");
    assert_eq!(
        buf_result_server.as_ref(),
        buf_result_client.as_ref(),
        "The buffers received from the client and server are identical"
    );
}

async fn spawn_all_tcp(
    relay_addr: &str,
    server_addr: &str,
    buf_read: BufWrapper,
    buf_write: BufWrapper,
    rx: tokio::sync::broadcast::Receiver<RelayCommand>,
) -> tokio::sync::oneshot::Receiver<BufWrapper> {
    let (output_sender, output_receiver) = oneshot::channel::<BufWrapper>();
    let client = TestTcpClient::new(relay_addr);
    let relay = RelayTcp::new(relay_addr, server_addr);
    let server = TestTcpServer::new(server_addr);
    server
        .spawn_reader(buf_read, rx.resubscribe(), output_sender)
        .await;
    relay.spawn(rx).await;
    client.spawn_writer(buf_write).await;

    output_receiver
}

async fn spawn_all_tcp2(
    relay_addr: &str,
    server_addr: &str,
    buf_read: BufWrapper,
    buf_write: BufWrapper,
    rx: tokio::sync::broadcast::Receiver<RelayCommand>,
) -> tokio::sync::oneshot::Receiver<BufWrapper> {
    let (output_sender, output_receiver) = oneshot::channel::<BufWrapper>();
    let client = TestTcpClient::new(relay_addr);
    let relay = RelayTcp::new(relay_addr, server_addr);
    let server = TestTcpServer::new(server_addr);
    client
        .spawn_reader(buf_read, rx.resubscribe(), output_sender)
        .await;
    relay.spawn(rx).await;
    server.spawn_writer(buf_write).await;

    output_receiver
}

async fn spawn_all_udp(
    relay_addr: &str,
    server_addr: &str,
    buf_read: BufWrapper,
    buf_write: BufWrapper,
    rx: tokio::sync::broadcast::Receiver<RelayCommand>,
) -> tokio::sync::oneshot::Receiver<BufWrapper> {
    let (output_sender, output_receiver) = oneshot::channel::<BufWrapper>();
    let client = TestUdpClient::new(relay_addr);
    let relay = RelayUdp::new(relay_addr, server_addr);
    let server = TestUdpServer::new(server_addr);
    server
        .spawn_reader(buf_read, rx.resubscribe(), output_sender)
        .await;
    relay.spawn(rx).await;
    client.spawn_writer(buf_write).await;

    output_receiver
}

async fn spawn_all_udp2(
    relay_addr: &str,
    server_addr: &str,
    rx: tokio::sync::broadcast::Receiver<RelayCommand>,
) -> (TestUdpClient, TestUdpServer) {
    let client = TestUdpClient::new(relay_addr);
    let relay = RelayUdp::new(relay_addr, server_addr);
    let server = TestUdpServer::new(server_addr);
    // server
    //     .spawn_reader(buf_read, rx.resubscribe(), output_sender)
    //     .await;
    relay.spawn(rx).await;
    // client.spawn_writer(buf_write).await;

    (client, server)
}
