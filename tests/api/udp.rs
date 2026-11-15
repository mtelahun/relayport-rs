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
    assert!(
        !builder.so_reuseaddr(),
        "initial state of udp:SO_REUSEADDR is False"
    );

    let builder = builder.set_so_reuseaddr(true);
    assert!(builder.so_reuseaddr(), "state of udp:SO_REUSEADDR is True");
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

#[tokio::test]
async fn udp_relay_survives_client_setup_failure() {
    use std::time::Duration;
    use tokio::net::UdpSocket;

    // The remote socket is bound to 0.0.0.0 (IPv4), so connecting it to an IPv6
    // remote fails and setting up the client's relay errors out.
    let (_tx, rx) = broadcast::channel::<RelayCommand>(1);
    let relay = RelayUdpSocket::build()
        .bind("127.0.0.1:33099")
        .await
        .expect("failed to bind relay");
    let handle = tokio::spawn(async move { relay.run("[::1]:33100", &rx).await });

    let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(b"hello", "127.0.0.1:33099").await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        !handle.is_finished(),
        "relay stopped after one client's setup failed: {:?}",
        handle.await
    );
    handle.abort();
}

#[tokio::test]
async fn udp_relay_shuts_down_without_traffic() {
    use std::time::Duration;

    let (tx, rx) = broadcast::channel(1);
    let relay = RelayUdpSocket::build()
        .set_so_reuseaddr(true)
        .bind("127.0.0.1:34099")
        .await
        .expect("failed to bind relay");
    let handle = tokio::spawn(async move { relay.run("127.0.0.1:34100", &rx).await });
    // Let run() subscribe to the command channel before sending.
    tokio::time::sleep(Duration::from_millis(100)).await;

    tx.send(RelayCommand::Shutdown).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("relay did not shut down while idle")
        .unwrap();
    assert!(result.is_ok(), "relay returned an error: {result:?}");
}

#[tokio::test]
async fn udp_relay_works_with_default_options() {
    use std::time::Duration;
    use tokio::net::UdpSocket;

    // Echo server on the remote address.
    let server = UdpSocket::bind("127.0.0.1:35100").await.unwrap();
    tokio::spawn(async move {
        let mut buf = [0u8; 64];
        loop {
            let (len, peer) = server.recv_from(&mut buf).await.unwrap();
            server.send_to(&buf[..len], peer).await.unwrap();
        }
    });
    // No set_so_reuseaddr() call: the builder default.
    let (tx, rx) = broadcast::channel(1);
    let relay = RelayUdpSocket::build()
        .bind("127.0.0.1:35099")
        .await
        .expect("failed to bind relay");
    tokio::spawn(async move { relay.run("127.0.0.1:35100", &rx).await });

    let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.connect("127.0.0.1:35099").await.unwrap();
    client.send(b"hello").await.unwrap();
    let mut buf = [0u8; 64];
    let len = tokio::time::timeout(Duration::from_secs(2), client.recv(&mut buf))
        .await
        .expect("relay built with default options did not relay traffic")
        .unwrap();
    assert_eq!(&buf[..len], b"hello");

    tx.send(RelayCommand::Shutdown).unwrap();
}

#[tokio::test]
async fn udp_shutdown_with_active_client() {
    use std::time::Duration;
    use tokio::net::UdpSocket;

    // Echo server on the remote address.
    let server = UdpSocket::bind("127.0.0.1:36100").await.unwrap();
    tokio::spawn(async move {
        let mut buf = [0u8; 64];
        loop {
            let (len, peer) = server.recv_from(&mut buf).await.unwrap();
            server.send_to(&buf[..len], peer).await.unwrap();
        }
    });
    let (tx, rx) = broadcast::channel(1);
    let relay = RelayUdpSocket::build()
        .bind("127.0.0.1:36099")
        .await
        .expect("failed to bind relay");
    let handle = tokio::spawn(async move { relay.run("127.0.0.1:36100", &rx).await });

    // Make sure the client's relay task is up before shutting down.
    let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.connect("127.0.0.1:36099").await.unwrap();
    client.send(b"hello").await.unwrap();
    let mut buf = [0u8; 64];
    tokio::time::timeout(Duration::from_secs(2), client.recv(&mut buf))
        .await
        .expect("relay did not relay traffic")
        .unwrap();

    // run() waits for the client's relay task, which must also stop on Shutdown.
    tx.send(RelayCommand::Shutdown).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("relay did not shut down with an active client")
        .unwrap();
    assert!(result.is_ok(), "relay returned an error: {result:?}");
}
