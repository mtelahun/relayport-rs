# relayport-rs
Fast and easy abstraction for proxying TCP and UDP ports.

![Rust](https://github.com/mtelahun/relayport-rs/actions/workflows/rust.yml/badge.svg)
[![codecov](https://codecov.io/gh/mtelahun/relayport-rs/branch/main/graph/badge.svg?token=A1P9I5E2LU)](https://codecov.io/gh/mtelahun/relayport-rs)
[![License](https://img.shields.io/badge/License-BSD_2--Clause-orange.svg)](https://opensource.org/licenses/BSD-2-Clause)
![crates.io](https://img.shields.io/crates/v/relayport-rs.svg)
![docs.rs](https://img.shields.io/docsrs/relayport-rs)

This library simplifies the creation of asynchronous TCP/UDP proxies from rust applications. The only limit on the number
of proxies are the resources available on the system on which it is run. This library depends on [tokio](https::/tokio.rs)
for its runtime.

## Use
To use this library in your own package add the following to your Cargo.toml:

```
[dependencies]
relayport_rs = "0.4.0"
```

## Example
A simple program to proxy web traffic to a server might look like this:
```
use std::error::Error;
use relayport_rs::command::RelayCommand;
use relayport_rs::RelayPortError;
use relayport_rs::RelayTcpSocket;
use tokio::sync::broadcast;

#[tokio::main]
pub async fn main() -> Result<(), RelayPortError> {
    // The relay expects a broadcast channel on which to listen for shutdown commands
    let (tx, rx) = broadcast::channel(16);

    // build a relay with a listener TCP socket
    let relay = RelaySocket::build()
        .set_so_reuseaddr(true)
        .set_tcp_nodelay(true)
        .bind("127.0.0.1:8080")?
        .listen()?;

    // this will never return unless it encounters an error
    relay
        .run("127.0.0.1:80", &rx)
        .await
}
```

This example proxies DNS ports, which use UDP:
```
use std::error::Error;
use relayport_rs::command::RelayCommand;
use relayport_rs::RelayPortError;
use relayport_rs::RelayUdpSocket;
use tokio::sync::broadcast;

#[tokio::main]
pub async fn main() -> Result<(), RelayPortError> {
    // The relay expects a broadcast channel on which to listen for shutdown commands
    let (tx, rx) = broadcast::channel(16);

    // build a relay with a UDP socket
    let udp_relay = RelayUdpSocket::build()
        .set_so_reuseaddr(true)
        .bind("127.0.0.1:10080")
        .await?;

    // this will never return unless it encounters an error
    udp_relay
        .run("1.1.1.1:53", &rx)
        .await
        .expect("failed to start relay");
}
```


## Pre-requisites
1. Git source code versioning system

`https://git-scm.com/book/en/v2/Getting-Started-Installing-Git`

2. Rust programming language [Official install guide](https://www.rust-lang.org/tools/install)

`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

To insure it was installed correctly type the following commands and make sure you get a successful output:
```
rustc --version
cargo --version
```
