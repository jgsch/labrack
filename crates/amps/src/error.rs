use std::io;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    #[error("timeout: {0}")]
    Timeout(String),
    /// The amp accepted the connection and closed it at once: another controller is connected.
    #[error("amp is busy: {0}")]
    Busy(String),
    #[error("disconnected: {0}")]
    Disconnected(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("not supported by this amp: {0}")]
    Unsupported(String),
    #[error("invalid value: {0}")]
    InvalidValue(String),
    /// A write was not echoed / read back with the value that was sent.
    #[error("write not confirmed: {0}")]
    NotConfirmed(String),
    #[error("cannot parse: {0}")]
    Parse(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}
