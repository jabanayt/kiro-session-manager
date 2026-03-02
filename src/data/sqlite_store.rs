//! SQLite-backed metadata store using ksm.db.
//!
//! Replaces JsonMetadataStore. Introduces schema versioning for future
//! migrations (archive tables in Phase 1, etc.).

use log::debug;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::{ksm_db_path, metadata_path};
use crate::data::MetadataStore;
use crate::error::{KsmError, Result};
use crate::models::SessionMetadata;

/// Metadata store backed by SQLite (ksm.db).
pub struct SqliteMetadataStore {
    path: PathBuf,
}

impl SqliteMetadataStore {
    /// Create store using path from config.
    pub fn from_config() -> Result<Self> {
        let path = ksm_db_path()?;
        let store = SqliteMetadataStore { path };
        store.ensure_schema()?;
        Ok(store)
    }

    /// Create store with explicit path (for testing).
    pub fn new(path: PathBuf) -> Result<Self> {
        let store = SqliteMetadataStore { path };
        store.ensure_schema()?;
        Ok(store)
    }

    /// Open read-write connection to ksm.db.
    fn open(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path).map_err(|e| KsmError::Storage {
            message: format!("Failed to open database: {}", e),
            path: Some(self.path.clone()),
        })?;

        // Enable WAL mode for concurrent reads
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| KsmError::Storage {
                message: format!("Failed to enable WAL mode: {}", e),
                path: Some(self.path.clone()),
            })?;

        // Enable foreign keys
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(|e| KsmError::Storage {
                message: format!("Failed to enable foreign keys: {}", e),
                path: Some(self.path.clone()),
            })?;

        Ok(conn)
    }

    /// Ensure database schema is at the current version.
    ///
    /// Creates tables if database is new. Runs migrations if schema
    /// version is behind.
    fn ensure_schema(&self) -> Result<()> {
        let conn = self.open()?;

        let table_exists: bool = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='schema_version'")?
            .exists([])?;

        if !table_exists {
            Self::create_v1_schema(&conn)?;
            Self::migrate_v1_to_v2(&conn)?;
        } else {
            let version: i64 = conn
                .prepare("SELECT version FROM schema_version")?
                .query_row([], |row| row.get(0))?;

            if version == 1 {
                Self::migrate_v1_to_v2(&conn)?;
            }
            // version == 2: nothing to do (current)
        }

        Ok(())
    }

    /// Create version 1 schema (metadata tables).
    fn create_v1_schema(connection: &Connection) -> Result<()> {
        let tx = connection.unchecked_transaction()?;

        tx.execute(
            "CREATE TABLE schema_version (
                version INTEGER NOT NULL
            )",
            [],
        )?;

        tx.execute("INSERT INTO schema_version (version) VALUES (1)", [])?;

        tx.execute(
            "CREATE TABLE metadata (
                session_id TEXT PRIMARY KEY,
                name TEXT,
                tags TEXT,
                directory TEXT,
                parent_session_id TEXT,
                manually_unlinked BOOLEAN NOT NULL DEFAULT FALSE
            )",
            [],
        )?;

        tx.commit()?;
        Ok(())
    }

    /// Migrate from schema version 1 to version 2 (add archive tables).
    fn migrate_v1_to_v2(connection: &Connection) -> Result<()> {
        let tx = connection.unchecked_transaction()?;

        tx.execute_batch(
            "CREATE TABLE archives (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                directory TEXT NOT NULL,
                message_count INTEGER NOT NULL,
                session_created_at INTEGER NOT NULL,
                archived_at INTEGER NOT NULL,
                tags TEXT,
                pruned BOOLEAN NOT NULL DEFAULT FALSE
            );

            CREATE INDEX idx_archives_directory ON archives(directory);
            CREATE INDEX idx_archives_name ON archives(name);

            CREATE TABLE chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                archive_id INTEGER NOT NULL REFERENCES archives(id) ON DELETE CASCADE,
                exchange_index INTEGER NOT NULL,
                user_content TEXT NOT NULL,
                assistant_content TEXT NOT NULL,
                tool_summary TEXT,
                UNIQUE(archive_id, exchange_index)
            );

            CREATE VIRTUAL TABLE chunks_fts USING fts5(
                user_content,
                assistant_content,
                tool_summary,
                content='chunks',
                content_rowid='id',
                tokenize='porter unicode61'
            );

            CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
                INSERT INTO chunks_fts(rowid, user_content, assistant_content, tool_summary)
                VALUES (new.id, new.user_content, new.assistant_content, new.tool_summary);
            END;

            CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
                INSERT INTO chunks_fts(chunks_fts, rowid, user_content, assistant_content, tool_summary)
                VALUES ('delete', old.id, old.user_content, old.assistant_content, old.tool_summary);
            END;

            UPDATE schema_version SET version = 2;",
        )?;

        tx.commit()?;
        debug!("Migrated schema from version 1 to version 2 (archive tables)");
        Ok(())
    }

    /// One-time migration from metadata.json to SQLite.
    ///
    /// Called on first run after upgrade. Reads existing JSON file,
    /// inserts all entries into SQLite, logs the migration.
    /// Does NOT delete the JSON file (kept as safety net).
    pub fn migrate_from_json(&self) -> Result<usize> {
        let json_path = metadata_path()?;

        if !json_path.exists() {
            return Ok(0);
        }

        // Read and parse JSON file
        let content = std::fs::read_to_string(&json_path)?;
        let metadata: HashMap<String, SessionMetadata> = serde_json::from_str(&content)?;

        if metadata.is_empty() {
            return Ok(0);
        }

        let conn = self.open()?;
        let tx = conn.unchecked_transaction()?;

        let count = {
            let mut stmt = tx.prepare(
                "INSERT INTO metadata (session_id, name, tags, directory, parent_session_id, manually_unlinked)
                 VALUES (?, ?, ?, ?, ?, ?)"
            )?;

            let mut count = 0;
            for (session_id, meta) in &metadata {
                let tags_json = if meta.tags.is_empty() {
                    None
                } else {
                    let tags_vec: Vec<String> = meta.tags.iter().cloned().collect();
                    Some(serde_json::to_string(&tags_vec)?)
                };

                stmt.execute([
                    session_id,
                    meta.name.as_deref().unwrap_or(""),
                    tags_json.as_deref().unwrap_or(""),
                    meta.directory.as_deref().unwrap_or(""),
                    meta.parent_session_id.as_deref().unwrap_or(""),
                    &(if meta.manually_unlinked { 1i64 } else { 0i64 }).to_string(),
                ])?;
                count += 1;
            }
            count
        }; // stmt is dropped here

        tx.commit()?;

        debug!("Migrated {} metadata entries from JSON to SQLite", count);
        Ok(count)
    }
}

impl MetadataStore for SqliteMetadataStore {
    fn load(&self) -> Result<HashMap<String, SessionMetadata>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT session_id, name, tags, directory, parent_session_id, manually_unlinked FROM metadata"
        )?;

        let rows = stmt.query_map([], |row| {
            let session_id: String = row.get(0)?;
            let name: Option<String> = row.get::<_, Option<String>>(1)?.filter(|s| !s.is_empty());
            let tags_json: Option<String> =
                row.get::<_, Option<String>>(2)?.filter(|s| !s.is_empty());
            let directory: Option<String> =
                row.get::<_, Option<String>>(3)?.filter(|s| !s.is_empty());
            let parent_session_id: Option<String> =
                row.get::<_, Option<String>>(4)?.filter(|s| !s.is_empty());
            let manually_unlinked: bool = row.get::<_, i64>(5)? != 0;

            let tags = if let Some(json) = tags_json {
                let tags_vec: Vec<String> = serde_json::from_str(&json).unwrap_or_default();
                tags_vec.into_iter().collect()
            } else {
                std::collections::HashSet::new()
            };

            Ok((
                session_id,
                SessionMetadata {
                    name,
                    tags,
                    directory,
                    parent_session_id,
                    manually_unlinked,
                },
            ))
        })?;

        let mut metadata = HashMap::new();
        for row in rows {
            let (session_id, meta) = row?;
            metadata.insert(session_id, meta);
        }

        Ok(metadata)
    }

    fn save(&self, metadata: &HashMap<String, SessionMetadata>) -> Result<()> {
        let conn = self.open()?;
        let tx = conn.unchecked_transaction()?;

        // Full overwrite - delete all then insert
        tx.execute("DELETE FROM metadata", [])?;

        {
            let mut stmt = tx.prepare(
                "INSERT INTO metadata (session_id, name, tags, directory, parent_session_id, manually_unlinked)
                 VALUES (?, ?, ?, ?, ?, ?)"
            )?;

            for (session_id, meta) in metadata {
                let tags_json = if meta.tags.is_empty() {
                    None
                } else {
                    let tags_vec: Vec<String> = meta.tags.iter().cloned().collect();
                    Some(serde_json::to_string(&tags_vec)?)
                };

                stmt.execute([
                    session_id.as_str(),
                    meta.name.as_deref().unwrap_or(""),
                    tags_json.as_deref().unwrap_or(""),
                    meta.directory.as_deref().unwrap_or(""),
                    meta.parent_session_id.as_deref().unwrap_or(""),
                    &(if meta.manually_unlinked { 1i64 } else { 0i64 }).to_string(),
                ])?;
            }
        } // stmt is dropped here

        tx.commit()?;
        Ok(())
    }
}
