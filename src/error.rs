use thiserror::Error;

#[derive(Error, Debug)]
pub enum GraftailError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("JSON parse error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Invalid timestamp: {0}")]
    Timestamp(String),

    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Connection lost after {0} retries")]
    MaxRetriesExceeded(usize),

    #[error("Interrupted by signal")]
    Interrupted,
}

/// Convenience type alias
pub type Result<T> = std::result::Result<T, GraftailError>;
