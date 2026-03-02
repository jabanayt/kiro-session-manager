use log::debug;
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;

use crate::data::SessionSource;
use crate::error::{KsmError, Result};
use crate::models::{ConversationData, Session};

/// Session source backed by kiro-cli's SQLite database.
pub struct DatabaseSource {
    db_path: PathBuf,
}

impl Default for DatabaseSource {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabaseSource {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        DatabaseSource {
            db_path: PathBuf::from(home).join(".local/share/kiro-cli/data.sqlite3"),
        }
    }

    /// Open read-only connection to kiro-cli's database.
    fn open_readonly(&self) -> Result<Connection> {
        let conn = Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| KsmError::Database(format!("Failed to open database: {}", e)))?;

        // Verify conversations_v2 table exists
        let table_exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='conversations_v2'",
            [],
            |row| row.get(0),
        )?;

        if !table_exists {
            return Err(KsmError::Database(
                "conversations_v2 table not found. Please update kiro-cli.".to_string(),
            ));
        }

        Ok(conn)
    }

    /// Open read-write connection to kiro-cli's database.
    fn open_readwrite(&self) -> Result<Connection> {
        Connection::open(&self.db_path)
            .map_err(|e| KsmError::Database(format!("Failed to open database for writing: {}", e)))
    }

    /// Convert database row to Session with raw data.
    fn row_to_session(
        &self,
        id: String,
        data: &ConversationData,
        created_at: i64,
        updated_at: i64,
    ) -> Session {
        let preview = data.preview();
        let msg_count = data.history.len() as u32;

        Session {
            id,
            created_at,
            updated_at,
            preview,
            msg_count,
        }
    }
}

impl SessionSource for DatabaseSource {
    fn list_sessions(&self) -> Result<Vec<Session>> {
        let conn = self.open_readonly()?;
        let current_dir = std::env::current_dir()?.display().to_string();

        let mut stmt = conn.prepare(
            "SELECT conversation_id, value, created_at, updated_at
             FROM conversations_v2
             WHERE key = ?
             ORDER BY updated_at DESC",
        )?;

        let rows = stmt
            .query_map([&current_dir], |row| {
                let conversation_id: String = row.get(0)?;
                let value_json: String = row.get(1)?;
                let created_at: i64 = row.get(2)?;
                let updated_at: i64 = row.get(3)?;
                Ok((conversation_id, value_json, created_at, updated_at))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut sessions = Vec::new();
        for (id, json, created_at, updated_at) in rows {
            let data: ConversationData = serde_json::from_str(&json).map_err(|e| {
                KsmError::Database(format!("Failed to parse conversation {}: {}", id, e))
            })?;
            sessions.push(self.row_to_session(id, &data, created_at, updated_at));
        }

        debug!("Loaded {} sessions from database", sessions.len());
        Ok(sessions)
    }

    fn get_conversation(&self, session_id: &str) -> Result<ConversationData> {
        let conn = self.open_readonly()?;
        let value_json: String = conn
            .query_row(
                "SELECT value FROM conversations_v2 WHERE conversation_id = ?",
                [session_id],
                |row| row.get(0),
            )
            .map_err(|_| KsmError::SessionNotFound(session_id.to_string()))?;

        let data: ConversationData = serde_json::from_str(&value_json)?;
        Ok(data)
    }

    fn get_message_ids(&self, session_id: &str) -> Result<Vec<String>> {
        let data = self.get_conversation(session_id)?;
        let mut message_ids = Vec::new();
        for entry in &data.history {
            if let Some(metadata) = &entry.request_metadata
                && let Some(msg_id) = &metadata.message_id
            {
                message_ids.push(msg_id.clone());
            }
        }
        Ok(message_ids)
    }

    fn has_compact_tag(&self, session_id: &str) -> Result<bool> {
        let data = self.get_conversation(session_id)?;
        if let Some(summary) = data.latest_summary
            && summary.len() > 1
            && let Some(tags) = summary[1].get("message_meta_tags")
            && let Some(tags_arr) = tags.as_array()
        {
            return Ok(tags_arr.iter().any(|t| t.as_str() == Some("Compact")));
        }
        Ok(false)
    }

    fn get_timestamps(&self, session_id: &str) -> Result<(i64, i64)> {
        let conn = self.open_readonly()?;
        conn.query_row(
            "SELECT created_at, updated_at FROM conversations_v2 WHERE conversation_id = ?",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| KsmError::SessionNotFound(session_id.to_string()))
    }

    fn update_timestamp(&self, session_id: &str, timestamp: i64) -> Result<()> {
        let current_dir = std::env::current_dir()?.display().to_string();
        let conn = self.open_readwrite()?;
        conn.execute(
            "UPDATE conversations_v2 SET updated_at = ? WHERE key = ? AND conversation_id = ?",
            [&timestamp.to_string(), &current_dir, session_id],
        )?;
        debug!("Updated timestamp for session {}", session_id);
        Ok(())
    }

    fn delete_session(&self, session_id: &str) -> Result<()> {
        let output = std::process::Command::new("kiro-cli")
            .args(["chat", "--delete-session", session_id])
            .output()?;

        if !output.status.success() {
            return Err(KsmError::KiroCli(format!(
                "Failed to delete session {}: {}",
                session_id,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }
}
