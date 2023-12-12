use relayport_rs::{RelayCommand, RelayUdpSocket};
use tokio::sync::{broadcast, oneshot};

use crate::helpers::{
    bufwrapper::BufWrapper, client::TestUdpClient, relay::RelayUdp, server::TestUdpServer, TestOp,
};

#[tokio::test]
async fn udp_client_to_server() {
    let (tx, rx) = broadcast::channel(1);
    let (server_sender, server_receiver) = oneshot::channel::<BufWrapper>();
    let (client_sender, _) = oneshot::channel::<BufWrapper>();
    let mut buf_write = BufWrapper::new();
    buf_write.write(56u8, 100);
    let buf_read = BufWrapper::new();

    let (client, server) =
        spawn_udp_relay("127.0.0.1:31099", "127.0.0.1:31100", rx.resubscribe()).await;

    server
        .spawn(TestOp::Read, buf_read, rx.resubscribe(), server_sender)
        .await;
    client
        .spawn(TestOp::Write, buf_write.clone(), rx, client_sender)
        .await;

    let buf_result_server = server_receiver
        .await
        .expect("failed to receive output buffer");
    println!("client_to_server: Sending shutdown cmd ...");
    tx.send(RelayCommand::Shutdown)
        .expect("failed to send shutdown command to relay");

    assert_eq!(
        buf_result_server.as_ref(),
        buf_write.as_ref(),
        "The contents of the buffer received by the server are identical to that sent by the client");
}

#[tokio::test]
async fn udp_server_to_client() {
    tracing_subscriber::fmt::init();
    let (tx, rx) = broadcast::channel(1);
    let (server_sender, server_receiver) = oneshot::channel::<BufWrapper>();
    let (client_sender, client_receiver) = oneshot::channel::<BufWrapper>();
    let mut buf_server = BufWrapper::new();
    buf_server.write(56u8, 100);
    let mut buf_client = BufWrapper::new();
    buf_client.write(57u8, 100);

    let (client, server) =
        spawn_udp_relay("127.0.0.1:32099", "127.0.0.1:32100", rx.resubscribe()).await;
    server
        .spawn(
            TestOp::Write,
            buf_server.clone(),
            rx.resubscribe(),
            server_sender,
        )
        .await;
    client
        .spawn(TestOp::Write, buf_client.clone(), rx, client_sender)
        .await;

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
        buf_client.as_ref(),
        "The content received by the server == the content sent by the client"
    );
    assert_eq!(
        buf_result_client.as_ref(),
        buf_server.as_ref(),
        "The content received by the client == the content sent by the server"
    );
}

#[tokio::test]
async fn set_udp_builder_options() {
    let mut builder = RelayUdpSocket::build();
    assert_eq!(
        builder.so_reuseaddr(),
        false,
        "initial state of udp:SO_REUSEADDR is False"
    );

    let builder = builder.set_so_reuseaddr(true);
    assert_eq!(
        builder.so_reuseaddr(),
        true,
        "state of udp:SO_REUSEADDR is True"
    );
}

/// Spawn a UDP relay.
///
/// # Returns
/// This method returns a tuple containing the client and server test objects.
async fn spawn_udp_relay(
    relay_addr: &str,
    server_addr: &str,
    rx: tokio::sync::broadcast::Receiver<RelayCommand>,
) -> (TestUdpClient, TestUdpServer) {
    let client = TestUdpClient::new(relay_addr);
    let relay = RelayUdp::new(relay_addr, server_addr);
    let server = TestUdpServer::new(server_addr);
    relay.spawn(rx).await;

    (client, server)
}
