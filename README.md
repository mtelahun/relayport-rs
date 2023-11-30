# relayport-rs
Fast and easy abstraction for proxying TCP ports.

[![codecov](https://codecov.io/gh/mtelahun/relayport-rs/branch/main/graph/badge.svg?token=A1P9I5E2LU)](https://codecov.io/gh/mtelahun/relayport-rs)
[![License](https://img.shields.io/badge/License-BSD_2--Clause-orange.svg)](https://opensource.org/licenses/BSD-2-Clause)

## Example
This library simplifies the creation of asynchronous TCP proxies from rust applications. The only limit on the number
of proxies are the resources available on the system on which it is run. This library depends on [tokio](https::/tokio.rs)
for its runtime. A simple program to proxy web traffic to a server might look like this:
```
use std::error::Error;
use tokio::sync::broadcast;
use relayport_rs::RelaySocket;
use relayport_rs::command::RelayCommand;

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    // The relay expects a broadcast channel on which to listen for shutdown commands
    let (tx, rx) = broadcast::channel(16);

    // build a relay with a listener TCP socket
    let relay = RelaySocket::build()
        .set_so_reuseaddr(true)?
        .set_tcp_nodelay(true)?
        .bind("0.0.0.0:8080)?
        .listen()?;

    // spawn a task to handle the acceptance and dispatch of a relay connection
    let _ = tokio::task::spawn(async move {
        relay
            .accept_and_relay("www.example.com:80", &rx)
            .await
            .expect("failed to start relay")
    });

    // send the task a shutdown command so it exits cleanly
    tx.send(RelayCommand::Shutdown)?;

    Ok(())
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

## Use
To use this library in your own package add the following to your Cargo.toml:

```
[dependencies]
relayport_rs = "0.1.0"
```


Build
-----
From a terminal:
1. Clone this repository

```git clone https://github.com/mtelahun/sppg.git```

2. Change into the cloned directory and type:

```cargo run --release```

Installation
------------
