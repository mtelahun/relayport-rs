//! Proxy TCP and UDP ports
pub mod tcp;
pub mod udp;

use tokio::task::JoinSet;

use crate::RelayPortError;

/// Called when a relay's receive loop ends. After a clean shutdown every connection task has
/// been told to stop, so wait for them all to finish. After an error, leave them running as
/// before; they still stop when the caller sends [crate::RelayCommand::Shutdown].
pub(crate) async fn wait_for_connections(
    mut connections: JoinSet<()>,
    result: Result<(), RelayPortError>,
) -> Result<(), RelayPortError> {
    if result.is_ok() {
        while connections.join_next().await.is_some() {}
    } else {
        connections.detach_all();
    }
    result
}
