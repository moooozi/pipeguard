use thiserror::Error;

#[derive(Error, Debug)]
pub enum NamedPipeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Pipe not connected")]
    NotConnected,

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Invalid pipe name: {0}")]
    InvalidPipeName(String),

    #[error("Timeout occurred")]
    Timeout,

    #[error("Server already running on pipe: {0}")]
    ServerAlreadyRunning(String),
}

pub type Result<T> = std::result::Result<T, NamedPipeError>;
