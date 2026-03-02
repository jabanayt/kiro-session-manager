mod archive_store;
mod database;
mod json_store;
mod kiro_cli;
mod sqlite_store;

pub use archive_store::SqliteArchiveStore;
pub use database::DatabaseSource;
pub use json_store::JsonMetadataStore;
pub use kiro_cli::KiroCliSource;
pub use sqlite_store::SqliteMetadataStore;

use std::collections::HashMap;

use crate::error::Result;
use crate::models::{
    Archive, Chunk, ConversationData, NewArchive, NewChunk, SearchQuery, SearchResult, Session,
    SessionMetadata,
};

/// Read/write access to kiro-cli's session data.
///
/// Implementations: `DatabaseSource` (primary), `KiroCliSource` (fallback).
pub trait SessionSource {
    /// List all sessions for the current directory, ordered by updated_at DESC.
    fn list_sessions(&self) -> Result<Vec<Session>>;

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

/// Persistence layer for session metadata (names, tags, links).
///
/// Implementations: `JsonMetadataStore` (current), future `SqliteMetadataStore`.
pub trait MetadataStore {
    /// Load all metadata entries.
    fn load(&self) -> Result<HashMap<String, SessionMetadata>>;

    /// Save all metadata entries (full overwrite).
    fn save(&self, metadata: &HashMap<String, SessionMetadata>) -> Result<()>;
}

/// Persistence layer for session archives and full-text search.
///
/// Implementation: `SqliteArchiveStore` (ksm.db, FTS5).
pub trait ArchiveStore {
    /// Save a new archive with its content chunks.
    ///
    /// Inserts the archive record and all chunks in a single transaction.
    /// FTS5 index is updated automatically via triggers.
    fn save_archive(&self, archive: &NewArchive, chunks: &[NewChunk]) -> Result<i64>;

    /// Search archives using FTS5 full-text search.
    ///
    /// Returns matching chunks with archive context, ranked by relevance.
    fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>>;

    /// List all archives for a directory.
    fn list_archives(&self, directory: &str) -> Result<Vec<Archive>>;

    /// Get a single archive by name within a directory.
    fn get_archive(&self, name: &str, directory: &str) -> Result<Archive>;

    /// Get all chunks for an archive, ordered by exchange_index.
    fn get_chunks(&self, archive_id: i64) -> Result<Vec<Chunk>>;

    /// Delete an archive and all its chunks (cascading).
    ///
    /// Returns the number of chunks that were deleted.
    fn delete_archive(&self, archive_id: i64) -> Result<i64>;

    /// Check if a session is already archived.
    fn is_archived(&self, session_id: &str) -> Result<Option<String>>;
}

/// Hybrid session source: tries database first, falls back to CLI parsing.
///
/// Replaces the fallback logic from current `kiro.rs` `get_sessions()`.
pub struct HybridSource {
    database: DatabaseSource,
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
            database: DatabaseSource::new(),
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
