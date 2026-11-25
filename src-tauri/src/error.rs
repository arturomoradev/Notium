use serde::Serialize;

/// Structured, serializable error so the frontend can branch on `code`.
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum CommandError {
    #[error("vault is not open")]
    VaultClosed,
    #[error("note not found: {0}")]
    NotFound(String),
    #[error("concurrent modification: another writer saved this note")]
    Conflict,
    #[error("invalid format: {0}")]
    InvalidFormat(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("database error: {0}")]
    Sql(String),
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        CommandError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for CommandError {
    fn from(e: serde_json::Error) -> Self {
        CommandError::Io(e.to_string())
    }
}
