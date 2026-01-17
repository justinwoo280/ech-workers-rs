use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TLS error: {0}")]
    Tls(String),
    
    #[error("TLS handshake failed")]
    TlsHandshakeFailed,
    
    #[error("Connection failed")]
    ConnectionFailed,
    
    #[error("ECH not accepted (possible downgrade attack)")]
    EchNotAccepted,
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Out of memory")]
    OutOfMemory,

    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tungstenite::Error),

    #[error("Yamux error: {0}")]
    Yamux(#[from] yamux::ConnectionError),

    #[error("DNS error: {0}")]
    Dns(String),

    #[error("ECH error: {0}")]
    Ech(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Timeout")]
    Timeout,

    #[error("{0}")]
    Other(String),
}

impl From<rustls::Error> for Error {
    fn from(e: rustls::Error) -> Self {
        Error::Tls(e.to_string())
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Other(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Other(e.to_string())
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for Error {
    fn from(e: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Error::Other(e.to_string())
    }
}
