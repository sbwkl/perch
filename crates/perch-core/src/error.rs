#[derive(Debug, thiserror::Error)]
pub enum PerchError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Message(String),
}

impl PerchError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

pub type Result<T> = std::result::Result<T, PerchError>;
