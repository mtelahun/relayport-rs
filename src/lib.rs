//! Fast and easy abstraction for proxying TCP ports.
//!
//! This library simplifies the creation of asynchronous TCP proxies from rust applications. The only limit on the number
//! of proxies are the resources available on the system on which it is run. This library depends on [tokio](https::/tokio.rs)
//! for its runtime.
//!
//! # Example
//! A simple program to proxy web traffic to a server might look like this:
//! ```no_run
//! use std::error::Error;
//! use tokio::sync::broadcast;
//! use relayport_rs::RelaySocket;
//! use relayport_rs::command::RelayCommand;
//! use tokio::signal::unix::{signal, SignalKind};
//!
//! #[tokio::main]
//! pub async fn main() -> Result<(), Box<dyn Error>> {
//!     // The relay expects a broadcast channel on which to listen for shutdown commands
//!     let (tx, rx) = broadcast::channel(16);
//!
//!     // build a relay with a listener TCP socket
//!     let relay = RelaySocket::build()
//!         .set_so_reuseaddr(true)
//!         .set_tcp_nodelay(true)
//!         .bind("0.0.0.0:8080")?
//!         .listen()?;
//!
//!     // spawn a task to handle the acceptance and dispatch of a relay connection
//!     let _ = tokio::task::spawn(async move {
//!         relay
//!             .serve("127.0.0.1:80", &rx)
//!             .await
//!             .expect("failed to start relay")
//!     });
//!
//!
//!    // Wait for Ctrl-C to send the shutdown command
//!    let mut sigint = signal(SignalKind::interrupt())?;
//!    match sigint.recv().await {
//!        Some(()) => { tx.send(RelayCommand::Shutdown)?; {} },
//!        None => {},
//!    }
//!
//!    Ok(())
//! }
//! ```
//!
//! This is the same program again, but we don't care about catching a SIGINT:
//! ```no_run
//! use std::error::Error;
//! use tokio::sync::broadcast;
//! use relayport_rs::command::RelayCommand;
//! use relayport_rs::RelayPortError;
//! use relayport_rs::RelaySocket;
//!
//! #[tokio::main]
//! pub async fn main() -> Result<(), RelayPortError> {
//!     // The relay expects a broadcast channel on which to listen for shutdown commands
//!     let (tx, rx) = broadcast::channel(16);
//!
//!     // build a relay with a listener TCP socket
//!     let relay = RelaySocket::build()
//!         .set_so_reuseaddr(true)
//!         .set_tcp_nodelay(true)
//!         .bind("0.0.0.0:8080")?
//!         .listen()?;
//!
//!     // spawn a task to handle the acceptance and dispatch of a relay connection
//!     relay
//!         .serve("127.0.0.1:80", &rx)
//!         .await
//!
//! }
//! ```

pub mod command;
pub mod error;
pub mod relay;
pub use command::RelayCommand;
pub use error::RelayPortError;
pub use relay::tcp::{RelaySocket, RelayStream};
