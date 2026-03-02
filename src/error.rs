use std::path::PathBuf;

/// Structured error types for KSM.
///
/// Used throughout the library crate. The binary (main.rs) wraps
/// with anyhow for top-level error reporting.
#[derive(Debug, thiserror::Error)]
pub enum KsmError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Storage error: {message}")]
    Storage {
        message: String,
        path: Option<PathBuf>,
    },

    #[error("Index {index} out of range (max: {max})")]
    IndexOutOfRange { index: usize, max: usize },

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Chain conflict: {0}")]
    ChainConflict(String),

    #[error("Metadata conflict: child {child_id} has different metadata from parent {parent_id}")]
    MetadataConflict { child_id: String, parent_id: String },

    #[error("kiro-cli error: {0}")]
    KiroCli(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("No sessions found")]
    NoSessions,

    #[error("Archive not found: {0}")]
    ArchiveNotFound(String),

    #[error("Session already archived as '{0}'")]
    AlreadyArchived(String),

    #[error("Search error: {0}")]
    SearchError(String),

    #[error("Schema version mismatch: expected {expected}, found {found}")]
    SchemaVersionMismatch { expected: i64, found: i64 },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error(transparent)]
    Toml(#[from] toml::de::Error),
}

/// Library-wide Result type alias.
pub type Result<T> = std::result::Result<T, KsmError>;
