mod json_store;
mod kiro_cli;
mod kiro_database;
mod ksm_database;

pub use json_store::JsonMetadataStore;
pub use kiro_cli::KiroCliSource;
pub use kiro_database::KiroDatabase;
pub use ksm_database::KsmDatabase;

use crate::error::Result;
use crate::models::{ConversationData, Session};

/// Read/write access to kiro-cli's session data.
///
/// Implementations: `KiroDatabase` (primary), `KiroCliSource` (fallback).
pub trait SessionSource {
    /// List all sessions for the current directory, ordered by updated_at DESC.
    fn list_sessions(&self) -> Result<Vec<Session>>;

    /// List session IDs and timestamps only (lightweight, for cache checks).
    /// Returns (id, created_at, updated_at) tuples.
    fn list_session_timestamps(&self) -> Result<Vec<(String, i64, i64)>>;

    /// Get full conversation JSON for a session.
    fn get_conversation(&self, session_id: &str) -> Result<ConversationData>;

    /// Extract message IDs from a session's history.
    fn get_message_ids(&self, session_id: &str) -> Result<Vec<String>>;

    /// Check if a session has the Compact tag in its summary.
    fn has_compact_tag(&self, session_id: &str) -> Result<bool>;

    /// Get (created_at, updated_at) timestamps in milliseconds.
    fn get_timestamps(&self, session_id: &str) -> Result<(i64, i64)>;

    /// Update a session's updated_at timestamp (for resume).
    fn update_timestamp(&self, session_id: &str, timestamp: i64) -> Result<()>;

    /// Delete a session via kiro-cli.
    fn delete_session(&self, session_id: &str) -> Result<()>;
}

/// Hybrid session source: tries database first, falls back to CLI parsing.
///
/// Replaces the fallback logic from current `kiro.rs` `get_sessions()`.
pub struct HybridSource {
    database: KiroDatabase,
    cli_fallback: KiroCliSource,
}

impl Default for HybridSource {
    fn default() -> Self {
        Self::new()
    }
}

impl HybridSource {
    pub fn new() -> Self {
        HybridSource {
            database: KiroDatabase::new(),
            cli_fallback: KiroCliSource::new(),
        }
    }
}

impl SessionSource for HybridSource {
    fn list_sessions(&self) -> Result<Vec<Session>> {
        match self.database.list_sessions() {
            Ok(sessions) => Ok(sessions),
            Err(e) => {
                // Use eprintln! (not log::warn!) so users always see the fallback warning
                // Matches current kiro.rs output exactly
                eprintln!("⚠ Database access failed: {}", e);
                eprintln!("⚠ Falling back to CLI parsing...\n");
                self.cli_fallback.list_sessions()
            }
        }
    }

    fn list_session_timestamps(&self) -> Result<Vec<(String, i64, i64)>> {
        // No fallback - CLI source doesn't support this
        self.database.list_session_timestamps()
    }

    fn get_conversation(&self, session_id: &str) -> Result<ConversationData> {
        self.database
            .get_conversation(session_id)
            .or_else(|_| self.cli_fallback.get_conversation(session_id))
    }

    fn get_message_ids(&self, session_id: &str) -> Result<Vec<String>> {
        self.database
            .get_message_ids(session_id)
            .or_else(|_| self.cli_fallback.get_message_ids(session_id))
    }

    fn has_compact_tag(&self, session_id: &str) -> Result<bool> {
        self.database
            .has_compact_tag(session_id)
            .or_else(|_| self.cli_fallback.has_compact_tag(session_id))
    }

    fn get_timestamps(&self, session_id: &str) -> Result<(i64, i64)> {
        self.database
            .get_timestamps(session_id)
            .or_else(|_| self.cli_fallback.get_timestamps(session_id))
    }

    fn update_timestamp(&self, session_id: &str, timestamp: i64) -> Result<()> {
        self.database
            .update_timestamp(session_id, timestamp)
            .or_else(|_| self.cli_fallback.update_timestamp(session_id, timestamp))
    }

    fn delete_session(&self, session_id: &str) -> Result<()> {
        // Both impls call kiro-cli, so delegate to either
        self.database.delete_session(session_id)
    }
}
