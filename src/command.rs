//! Command definitions

/// Command definitions used by the caller to communicate with tasks spawned by the library.
#[derive(Copy, Clone, Debug)]
pub enum RelayCommand {
    /// Close the socket and end the task.
    Shutdown,
}
