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
    assert!(
        !builder.so_reuseaddr(),
        "initial state of tcp:SO_REUSEADDR is False"
    );
    assert!(
        !builder.tcp_nodelay(),
        "initial state of tcp:TCP_NODELAY is False"
    );

    let builder = builder.set_so_reuseaddr(true).set_tcp_nodelay(true);
    assert!(builder.so_reuseaddr(), "state of tcp:SO_REUSEADDR is True");
    assert!(builder.tcp_nodelay(), "state of tcp:TCP_NODELAY is True");
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

#[tokio::test]
async fn tcp_relay_serves_concurrent_connections() {
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    // Echo server that accepts any number of connections.
    let server = TcpListener::bind("127.0.0.1:22100").await.unwrap();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = server.accept().await.unwrap();
            tokio::spawn(async move {
                let (mut r, mut w) = stream.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    let (tx, rx) = broadcast::channel(1);
    RelayTcp::new("127.0.0.1:12099", "127.0.0.1:22100")
        .spawn(rx)
        .await;

    // The first connection stays open for the rest of the test.
    let mut first = TcpStream::connect("127.0.0.1:12099").await.unwrap();
    first.write_all(b"first").await.unwrap();
    let mut buf = [0u8; 6];
    first.read_exact(&mut buf[..5]).await.unwrap();
    assert_eq!(&buf[..5], b"first");

    // A second connection must be relayed while the first is still open.
    let mut second = TcpStream::connect("127.0.0.1:12099").await.unwrap();
    second.write_all(b"second").await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), second.read_exact(&mut buf))
        .await
        .expect("second connection was not relayed while the first was open")
        .unwrap();
    assert_eq!(&buf, b"second");

    tx.send(RelayCommand::Shutdown).unwrap();
}

#[tokio::test]
async fn tcp_shutdown_closes_connection_waiting_on_remote() {
    use std::time::Duration;
    use tokio::io::AsyncReadExt;
    use tokio::net::{TcpSocket, TcpStream};

    // A remote whose accept queue is full: Linux drops further connection
    // attempts, so the relay's connect to it hangs.
    let remote = TcpSocket::new_v4().unwrap();
    remote.bind("127.0.0.1:23100".parse().unwrap()).unwrap();
    let _remote = remote.listen(1).unwrap();
    let mut fillers = Vec::new();
    while let Ok(stream) = tokio::time::timeout(
        Duration::from_millis(200),
        TcpStream::connect("127.0.0.1:23100"),
    )
    .await
    {
        fillers.push(stream.unwrap());
        assert!(fillers.len() < 16, "test setup: accept queue never filled");
    }

    let (tx, rx) = broadcast::channel(1);
    let relay = RelayTcpSocket::build()
        .set_so_reuseaddr(true)
        .bind("127.0.0.1:13099")
        .unwrap()
        .listen()
        .unwrap();
    let handle = tokio::spawn(async move { relay.run("127.0.0.1:23100", &rx).await });

    let mut client = TcpStream::connect("127.0.0.1:13099").await.unwrap();
    // Let the relay accept the client and start connecting to the remote.
    tokio::time::sleep(Duration::from_millis(100)).await;

    tx.send(RelayCommand::Shutdown).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("relay did not shut down")
        .unwrap();
    assert!(result.is_ok(), "relay returned an error: {result:?}");

    // run() has returned, so the relayed connection must already be closed.
    let mut buf = [0u8; 1];
    tokio::time::timeout(Duration::from_secs(1), client.read(&mut buf))
        .await
        .expect("client connection still open after the relay shut down")
        .ok();
}
