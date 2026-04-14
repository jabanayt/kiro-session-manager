mod acp;
mod json_store;
mod kiro_cli;
mod kiro_database;
mod ksm_database;

pub use acp::AcpSource;
pub use json_store::JsonMetadataStore;
pub use kiro_cli::KiroCliSource;
pub use kiro_database::KiroDatabase;
pub use ksm_database::KsmDatabase;

use crate::error::Result;
use crate::models::{ConversationData, Session, SourceType};

/// Read/write access to kiro-cli's session data.
///
/// Implementations: `KiroDatabase` (primary), `KiroCliSource` (fallback), `AcpSource`.
pub trait SessionSource {
    /// List all sessions for the current directory, ordered by updated_at DESC.
    fn list_sessions(&self) -> Result<Vec<Session>>;

    /// List session IDs with updated_at for cache validation.
    /// Returns (id, updated_at) tuples. Uses index-only scan.
    fn list_session_updates(&self) -> Result<Vec<(String, i64)>>;

    /// Get full conversation JSON for a session.
    fn get_conversation(&self, session_id: &str) -> Result<ConversationData>;

    /// Get conversation data and created_at timestamp in one query.
    /// Used by cache service on cache miss.
    fn get_conversation_with_created_at(&self, session_id: &str)
    -> Result<(ConversationData, i64)>;

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

    /// The source type for sessions produced by this source.
    fn source_type(&self) -> SourceType {
        SourceType::Legacy
    }

    /// The source type for a specific session ID (for routing sources like HybridSource).
    fn session_source_type(&self, _session_id: &str) -> SourceType {
        self.source_type()
    }

    /// Get ACP directory mtime for cache validation.
    /// Returns None for sources without ACP sessions.
    fn acp_dir_mtime(&self) -> Option<std::time::SystemTime> {
        None
    }

    /// List session updates from legacy (v1) storage only.
    /// Used by cache when ACP mtime unchanged.
    fn list_session_updates_v1(&self) -> Result<Vec<(String, i64)>> {
        self.list_session_updates() // Default: same as full list
    }

    /// List session IDs from ACP (v2) storage only.
    /// Returns just IDs (not timestamps) for cache key tracking.
    fn list_acp_session_ids(&self) -> Result<Vec<String>> {
        Ok(Vec::new()) // Default: no ACP sessions
    }
}

/// Hybrid session source: tries database first, falls back to CLI parsing.
/// Also merges in ACP/TUI sessions from ~/.kiro/sessions/cli/.
pub struct HybridSource {
    database: KiroDatabase,
    cli_fallback: KiroCliSource,
    acp: AcpSource,
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
            acp: AcpSource::new(),
        }
    }

    /// Determine which source owns a session ID by checking ACP first (file existence),
    /// then falling back to legacy.
    fn route(&self, session_id: &str) -> &dyn SessionSource {
        if self.acp.has_session(session_id) {
            &self.acp
        } else {
            &self.database
        }
    }
}

impl SessionSource for HybridSource {
    fn list_sessions(&self) -> Result<Vec<Session>> {
        let mut sessions = match self.database.list_sessions() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("⚠ Database access failed: {}", e);
                eprintln!("⚠ Falling back to CLI parsing...\n");
                self.cli_fallback.list_sessions()?
            }
        };

        // Merge ACP sessions
        if let Ok(acp_sessions) = self.acp.list_sessions() {
            sessions.extend(acp_sessions);
        }

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    fn list_session_updates(&self) -> Result<Vec<(String, i64)>> {
        let mut updates = self.database.list_session_updates()?;
        if let Ok(acp_updates) = self.acp.list_session_updates() {
            updates.extend(acp_updates);
        }
        Ok(updates)
    }

    fn get_conversation(&self, session_id: &str) -> Result<ConversationData> {
        self.route(session_id).get_conversation(session_id)
    }

    fn get_conversation_with_created_at(
        &self,
        session_id: &str,
    ) -> Result<(ConversationData, i64)> {
        self.route(session_id)
            .get_conversation_with_created_at(session_id)
    }

    fn get_message_ids(&self, session_id: &str) -> Result<Vec<String>> {
        self.route(session_id).get_message_ids(session_id)
    }

    fn has_compact_tag(&self, session_id: &str) -> Result<bool> {
        self.route(session_id).has_compact_tag(session_id)
    }

    fn get_timestamps(&self, session_id: &str) -> Result<(i64, i64)> {
        self.route(session_id).get_timestamps(session_id)
    }

    fn update_timestamp(&self, session_id: &str, timestamp: i64) -> Result<()> {
        self.route(session_id)
            .update_timestamp(session_id, timestamp)
    }

    fn delete_session(&self, session_id: &str) -> Result<()> {
        self.route(session_id).delete_session(session_id)
    }

    fn session_source_type(&self, session_id: &str) -> SourceType {
        self.route(session_id).source_type()
    }

    fn acp_dir_mtime(&self) -> Option<std::time::SystemTime> {
        self.acp.dir_mtime().ok()
    }

    fn list_session_updates_v1(&self) -> Result<Vec<(String, i64)>> {
        self.database.list_session_updates()
    }

    fn list_acp_session_ids(&self) -> Result<Vec<String>> {
        let current_dir = std::env::current_dir()?.display().to_string();
        let mut ids = Vec::new();
        for id in self.acp.all_ids() {
            if let Ok(meta) = self.acp.read_meta(&id)
                && meta.cwd == current_dir
            {
                ids.push(meta.session_id);
            }
        }
        Ok(ids)
    }
}
