use relayport_rs::{RelayCommand, RelayTcpSocket};
use tokio::sync::{broadcast, oneshot};

use crate::helpers::{
    bufwrapper::BufWrapper, client::TestTcpClient, relay::RelayTcp, server::TestTcpServer, TestOp,
};

#[tokio::test]
async fn tcp_client_to_server() {
    let (tx, rx) = broadcast::channel(1);
    let (server_sender, server_receiver) = oneshot::channel::<BufWrapper>();
    let (client_sender, _) = oneshot::channel::<BufWrapper>();
    let mut buf_write = BufWrapper::new();
    buf_write.write(56u8, 100);
    let buf_read = BufWrapper::new();

    let (client, server) =
        spawn_tcp_relay("127.0.0.1:10099", "127.0.0.1:20100", rx.resubscribe()).await;
    server
        .spawn(TestOp::Read, buf_read, rx.resubscribe(), server_sender)
        .await;
    client
        .spawn(TestOp::Write, buf_write.clone(), rx, client_sender)
        .await;

    let buf_result = server_receiver
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
    let (server_sender, server_receiver) = oneshot::channel::<BufWrapper>();
    let (client_sender, client_receiver) = oneshot::channel::<BufWrapper>();
    let mut buf_write_server = BufWrapper::new();
    buf_write_server.write(56u8, 100);
    let mut buf_write_client = BufWrapper::new();
    buf_write_client.write(57u8, 100);
    // let buf_read = BufWrapper::new();

    let (client, server) =
        spawn_tcp_relay("127.0.0.1:11099", "127.0.0.1:21100", rx.resubscribe()).await;
    server
        .spawn(
            TestOp::Write,
            buf_write_server.clone(),
            rx.resubscribe(),
            server_sender,
        )
        .await;
    client
        .spawn(TestOp::Write, buf_write_client.clone(), rx, client_sender)
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
        buf_write_client.as_ref(),
        "The buffer received by the server == the buffer written by the client"
    );
    assert_eq!(
        buf_result_client.as_ref(),
        buf_write_server.as_ref(),
        "The buffer recieved by the client == the buffer written by the server"
    );
}

#[tokio::test]
async fn set_tcp_builder_options() {
    let mut builder = RelayTcpSocket::build();
    assert_eq!(
        builder.so_reuseaddr(),
        false,
        "initial state of tcp:SO_REUSEADDR is False"
    );
    assert_eq!(
        builder.tcp_nodelay(),
        false,
        "initial state of tcp:TCP_NODELAY is False"
    );

    let builder = builder.set_so_reuseaddr(true).set_tcp_nodelay(true);
    assert_eq!(
        builder.so_reuseaddr(),
        true,
        "state of tcp:SO_REUSEADDR is True"
    );
    assert_eq!(
        builder.tcp_nodelay(),
        true,
        "state of tcp:TCP_NODELAY is True"
    );
}

/// Spawn a TCP relay.
///
/// # Returns
/// This method returns a tuple containing the client and server test objects.
async fn spawn_tcp_relay(
    relay_addr: &str,
    server_addr: &str,
    rx: tokio::sync::broadcast::Receiver<RelayCommand>,
) -> (TestTcpClient, TestTcpServer) {
    let client = TestTcpClient::new(relay_addr);
    let relay = RelayTcp::new(relay_addr, server_addr);
    let server = TestTcpServer::new(server_addr);
    relay.spawn(rx).await;

    (client, server)
}
