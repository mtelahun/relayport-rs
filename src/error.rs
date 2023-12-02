//! Errors returned by the library

use std::net::AddrParseError;
use tokio::sync::broadcast::error::RecvError;

/// The methods of this library may return any of the errors defined here.
#[derive(Debug)]
pub enum RelayPortError {
    InternalCommunicationError(RecvError),
    IoError(std::io::Error),
    ParseAddrError(AddrParseError),
    Unknown(String),
}

impl std::fmt::Display for RelayPortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            RelayPortError::IoError(e) => format!("io error: {e}"),
            RelayPortError::ParseAddrError(e) => format!("unable to parse address: {e}"),
            RelayPortError::Unknown(s) => format!("unknown error: {s}"),
            RelayPortError::InternalCommunicationError(s) => {
                format!("internal communication error: {s}")
            }
        };

        write!(f, "{msg}")
    }
}

impl std::error::Error for RelayPortError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RelayPortError::IoError(e) => Some(e),
            RelayPortError::ParseAddrError(e) => Some(e),
            RelayPortError::Unknown(_) => None,
            RelayPortError::InternalCommunicationError(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for RelayPortError {
    fn from(value: std::io::Error) -> Self {
        Self::IoError(value)
    }
}

impl From<RecvError> for RelayPortError {
    fn from(value: RecvError) -> Self {
        Self::InternalCommunicationError(value)
    }
}
