//! Unified KSM database for metadata, archives, and state.
//!
//! Replaces separate SqliteMetadataStore and SqliteArchiveStore.
//! Single struct with direct methods (no traits).

use log::debug;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::{Config, load_config, metadata_path};
use crate::error::{KsmError, Result};
use crate::models::{
    Archive, ArchiveStatus, Chunk, NewArchive, NewChunk, SearchQuery, SearchResult, SessionMetadata,
};

/// State key for pending reindex tracking.
const STATE_PENDING_REINDEX: &str = "pending_reindex";

/// Unified database for all KSM data (metadata, archives, state).
pub struct KsmDatabase {
    path: PathBuf,
    config: Config,
}

impl KsmDatabase {
    /// Create database using path from config.
    ///
    /// Runs schema migrations.
    pub fn from_config() -> Result<Self> {
        let config = load_config()?;
        let path = crate::config::ksm_db_path()?;
        let db = KsmDatabase { path, config };
        db.ensure_schema()?;
        Ok(db)
    }

    /// Create database with explicit path (for testing).
    pub fn new(path: PathBuf) -> Result<Self> {
        let config = Config::default();
        let db = KsmDatabase { path, config };
        db.ensure_schema()?;
        Ok(db)
    }

    /// Get the auto_update config setting.
    pub fn auto_update_enabled(&self) -> bool {
        self.config.index.auto_update
    }

    /// Open read-write connection to ksm.db.
    fn open(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path).map_err(|e| KsmError::Storage {
            message: format!("Failed to open database: {}", e),
            path: Some(self.path.clone()),
        })?;

        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| KsmError::Storage {
                message: format!("Failed to enable WAL mode: {}", e),
                path: Some(self.path.clone()),
            })?;

        conn.pragma_update(None, "foreign_keys", true)
            .map_err(|e| KsmError::Storage {
                message: format!("Failed to enable foreign keys: {}", e),
                path: Some(self.path.clone()),
            })?;

        Ok(conn)
    }

    /// Ensure database schema is at current version.
    fn ensure_schema(&self) -> Result<()> {
        let conn = self.open()?;

        let table_exists: bool = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='schema_version'")?
            .exists([])?;

        if !table_exists {
            Self::create_v1_schema(&conn)?;
            Self::migrate_v1_to_v2(&conn)?;
            self.migrate_v2_to_v3(&conn)?;
            self.migrate_v3_to_v4(&conn)?;
        } else {
            let version: i64 = conn
                .prepare("SELECT version FROM schema_version")?
                .query_row([], |row| row.get(0))?;

            match version {
                1 => {
                    Self::migrate_v1_to_v2(&conn)?;
                    self.migrate_v2_to_v3(&conn)?;
                    self.migrate_v3_to_v4(&conn)?;
                }
                2 => {
                    self.migrate_v2_to_v3(&conn)?;
                    self.migrate_v3_to_v4(&conn)?;
                }
                3 => {
                    self.migrate_v3_to_v4(&conn)?;
                }
                4 => {} // Current version
                _ => {
                    return Err(KsmError::SchemaVersionMismatch {
                        expected: 4,
                        found: version,
                    });
                }
            }
        }

        Ok(())
    }

    fn create_v1_schema(conn: &Connection) -> Result<()> {
        let tx = conn.unchecked_transaction()?;

        tx.execute("CREATE TABLE schema_version (version INTEGER NOT NULL)", [])?;
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

    fn migrate_v1_to_v2(conn: &Connection) -> Result<()> {
        let tx = conn.unchecked_transaction()?;

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
        debug!("Migrated schema from version 1 to version 2");
        Ok(())
    }

    /// Migrate v2 to v3: add is_indexed column, state table, sparse config.
    fn migrate_v2_to_v3(&self, conn: &Connection) -> Result<()> {
        let tx = conn.unchecked_transaction()?;

        // Add is_indexed column
        tx.execute(
            "ALTER TABLE archives ADD COLUMN is_indexed BOOLEAN NOT NULL DEFAULT FALSE",
            [],
        )?;

        // Create state table
        tx.execute(
            "CREATE TABLE IF NOT EXISTS state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        tx.execute("UPDATE schema_version SET version = 3", [])?;
        tx.commit()?;

        // Determine is_indexed for existing archives
        self.migrate_archive_indexed_status()?;

        // Migrate config to sparse format
        self.migrate_config_to_sparse()?;

        debug!("Migrated schema from version 2 to version 3");
        Ok(())
    }

    /// Migrate v3 to v4: re-run config migration for consistency.
    fn migrate_v3_to_v4(&self, conn: &Connection) -> Result<()> {
        conn.execute("UPDATE schema_version SET version = 4", [])?;

        // Re-run config migration (idempotent, catches verbose configs)
        self.migrate_config_to_sparse()?;

        debug!("Migrated schema from version 3 to version 4");
        Ok(())
    }

    /// Set is_indexed based on whether session still exists in Kiro.
    fn migrate_archive_indexed_status(&self) -> Result<()> {
        let conn = self.open()?;
        let kiro_db = crate::data::KiroDatabase::new();

        let mut stmt = conn.prepare("SELECT id, session_id FROM archives")?;
        let archives: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        if archives.is_empty() {
            return Ok(());
        }

        let mut indexed_count = 0;
        let mut archived_count = 0;

        for (id, session_id) in &archives {
            let exists = kiro_db.session_exists(session_id).unwrap_or(false);
            conn.execute(
                "UPDATE archives SET is_indexed = ? WHERE id = ?",
                rusqlite::params![exists, id],
            )?;
            if exists {
                indexed_count += 1;
            } else {
                archived_count += 1;
            }
        }

        if indexed_count > 0 || archived_count > 0 {
            println!(
                "✓ Converted {} archives ({} indexed, {} archived)",
                archives.len(),
                indexed_count,
                archived_count
            );
        }

        Ok(())
    }

    /// Migrate config.toml to sparse format (comment out defaults).
    fn migrate_config_to_sparse(&self) -> Result<()> {
        let config_path = crate::config::config_path()?;
        if !config_path.exists() {
            return Ok(());
        }

        let config = load_config()?;

        // Build sparse config - only include non-default values
        let mut lines = Vec::new();

        lines.push("# KSM Configuration".to_string());
        lines.push("# Values shown in comments are defaults. Uncomment to override.".to_string());
        lines.push(String::new());

        // metadata_storage - default is "global"
        if config.metadata_storage != "global" {
            lines.push(format!(
                "metadata_storage = \"{}\"",
                config.metadata_storage
            ));
        } else {
            lines.push("# metadata_storage = \"global\"".to_string());
        }

        // custom_path
        if let Some(ref path) = config.custom_path {
            lines.push(format!("custom_path = \"{}\"", path));
        } else {
            lines.push("# custom_path = \"/path/to/ksm.db\"".to_string());
        }

        lines.push(String::new());

        // auto_detect_continuations - default is false
        if config.auto_detect_continuations {
            lines.push("auto_detect_continuations = true".to_string());
        } else {
            lines.push("# auto_detect_continuations = false".to_string());
        }

        // auto_clean - default is true
        if !config.auto_clean {
            lines.push("auto_clean = false".to_string());
        } else {
            lines.push("# auto_clean = true".to_string());
        }

        lines.push(String::new());
        lines.push("[index]".to_string());

        // auto_update - default is true
        if !config.index.auto_update {
            lines.push("auto_update = false".to_string());
        } else {
            lines.push("# auto_update = true".to_string());
        }

        std::fs::write(&config_path, lines.join("\n") + "\n")?;
        println!("✓ Migrated config.toml to sparse format");

        Ok(())
    }

    // ========== Metadata Methods (row-level) ==========

    /// Get metadata for a single session.
    pub fn get_metadata(&self, session_id: &str) -> Result<Option<SessionMetadata>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT name, tags, directory, parent_session_id, manually_unlinked
             FROM metadata WHERE session_id = ?",
        )?;

        match stmt.query_row([session_id], |row| {
            let name: Option<String> = row.get::<_, Option<String>>(0)?.filter(|s| !s.is_empty());
            let tags_json: Option<String> =
                row.get::<_, Option<String>>(1)?.filter(|s| !s.is_empty());
            let directory: Option<String> =
                row.get::<_, Option<String>>(2)?.filter(|s| !s.is_empty());
            let parent_session_id: Option<String> =
                row.get::<_, Option<String>>(3)?.filter(|s| !s.is_empty());
            let manually_unlinked: bool = row.get::<_, i64>(4)? != 0;

            let tags = if let Some(json) = tags_json {
                serde_json::from_str(&json).unwrap_or_default()
            } else {
                std::collections::HashSet::new()
            };

            Ok(SessionMetadata {
                name,
                tags,
                directory,
                parent_session_id,
                manually_unlinked,
            })
        }) {
            Ok(meta) => Ok(Some(meta)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Set metadata for a single session (insert or update).
    pub fn set_metadata(&self, session_id: &str, metadata: &SessionMetadata) -> Result<()> {
        let conn = self.open()?;

        let tags_json = if metadata.tags.is_empty() {
            None
        } else {
            let tags_vec: Vec<String> = metadata.tags.iter().cloned().collect();
            Some(serde_json::to_string(&tags_vec)?)
        };

        conn.execute(
            "INSERT INTO metadata (session_id, name, tags, directory, parent_session_id, manually_unlinked)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(session_id) DO UPDATE SET
                name = ?2, tags = ?3, directory = ?4, parent_session_id = ?5, manually_unlinked = ?6",
            rusqlite::params![
                session_id,
                metadata.name.as_deref().unwrap_or(""),
                tags_json.as_deref().unwrap_or(""),
                metadata.directory.as_deref().unwrap_or(""),
                metadata.parent_session_id.as_deref().unwrap_or(""),
                metadata.manually_unlinked,
            ],
        )?;

        Ok(())
    }

    /// Delete metadata for a single session.
    pub fn delete_metadata(&self, session_id: &str) -> Result<()> {
        let conn = self.open()?;
        conn.execute("DELETE FROM metadata WHERE session_id = ?", [session_id])?;
        Ok(())
    }

    /// List all metadata entries for a directory.
    pub fn list_metadata_for_directory(
        &self,
        directory: &str,
    ) -> Result<HashMap<String, SessionMetadata>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT session_id, name, tags, directory, parent_session_id, manually_unlinked
             FROM metadata WHERE directory = ? OR directory IS NULL OR directory = ''",
        )?;

        let current_dir = directory;
        let rows = stmt.query_map([current_dir], |row| {
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
                serde_json::from_str(&json).unwrap_or_default()
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

    /// Load all metadata (for backwards compatibility during transition).
    pub fn load_all_metadata(&self) -> Result<HashMap<String, SessionMetadata>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT session_id, name, tags, directory, parent_session_id, manually_unlinked
             FROM metadata",
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
                serde_json::from_str(&json).unwrap_or_default()
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

    /// Save all metadata (for backwards compatibility during transition).
    pub fn save_all_metadata(&self, metadata: &HashMap<String, SessionMetadata>) -> Result<()> {
        let conn = self.open()?;
        let tx = conn.unchecked_transaction()?;

        tx.execute("DELETE FROM metadata", [])?;

        {
            let mut stmt = tx.prepare(
                "INSERT INTO metadata (session_id, name, tags, directory, parent_session_id, manually_unlinked)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )?;

            for (session_id, meta) in metadata {
                let tags_json = if meta.tags.is_empty() {
                    None
                } else {
                    let tags_vec: Vec<String> = meta.tags.iter().cloned().collect();
                    Some(serde_json::to_string(&tags_vec)?)
                };

                stmt.execute(rusqlite::params![
                    session_id,
                    meta.name.as_deref().unwrap_or(""),
                    tags_json.as_deref().unwrap_or(""),
                    meta.directory.as_deref().unwrap_or(""),
                    meta.parent_session_id.as_deref().unwrap_or(""),
                    meta.manually_unlinked,
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    // ========== State Methods ==========

    /// Get a state value by key.
    pub fn get_state(&self, key: &str) -> Result<Option<String>> {
        let conn = self.open()?;
        match conn
            .prepare("SELECT value FROM state WHERE key = ?")?
            .query_row([key], |row| row.get(0))
        {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Set a state value (insert or update).
    pub fn set_state(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.open()?;
        conn.execute(
            "INSERT INTO state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            [key, value],
        )?;
        Ok(())
    }

    /// Clear a state value.
    pub fn clear_state(&self, key: &str) -> Result<()> {
        let conn = self.open()?;
        conn.execute("DELETE FROM state WHERE key = ?", [key])?;
        Ok(())
    }

    /// Set pending reindex for a session.
    pub fn set_pending_reindex(&self, session_id: &str) -> Result<()> {
        self.set_state(STATE_PENDING_REINDEX, session_id)
    }

    /// Clear pending reindex.
    pub fn clear_pending_reindex(&self) -> Result<()> {
        self.clear_state(STATE_PENDING_REINDEX)
    }

    /// Get pending reindex session ID.
    pub fn get_pending_reindex(&self) -> Result<Option<String>> {
        self.get_state(STATE_PENDING_REINDEX)
    }

    // ========== Archive Methods ==========

    /// Check archive/index status of a session.
    pub fn get_archive_status(&self, session_id: &str) -> Result<Option<ArchiveStatus>> {
        let conn = self.open()?;
        match conn
            .prepare("SELECT id, name, is_indexed FROM archives WHERE session_id = ?")?
            .query_row([session_id], |row| {
                let id: i64 = row.get(0)?;
                let name: String = row.get(1)?;
                let is_indexed: bool = row.get::<_, i64>(2)? != 0;
                Ok((id, name, is_indexed))
            }) {
            Ok((id, name, is_indexed)) => {
                if is_indexed {
                    Ok(Some(ArchiveStatus::Indexed {
                        name,
                        archive_id: id,
                    }))
                } else {
                    Ok(Some(ArchiveStatus::Archived {
                        name,
                        archive_id: id,
                    }))
                }
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Save a new archive with its content chunks.
    pub fn save_archive(
        &self,
        archive: &NewArchive,
        chunks: &[NewChunk],
        is_indexed: bool,
    ) -> Result<i64> {
        let conn = self.open()?;
        let tx = conn.unchecked_transaction()?;

        let tags_json = if archive.tags.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&archive.tags)?)
        };

        let archive_id = {
            let insert_result = tx.execute(
                "INSERT INTO archives (session_id, name, directory, message_count, session_created_at, archived_at, tags, pruned, is_indexed)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                rusqlite::params![
                    archive.session_id,
                    archive.name,
                    archive.directory,
                    archive.message_count,
                    archive.session_created_at,
                    archive.archived_at,
                    tags_json,
                    archive.pruned,
                    is_indexed,
                ],
            );

            match insert_result {
                Ok(_) => tx.last_insert_rowid(),
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    let existing_name: String = conn
                        .prepare("SELECT name FROM archives WHERE session_id = ?")?
                        .query_row([&archive.session_id], |row| row.get(0))
                        .unwrap_or_else(|_| "unknown".to_string());
                    return Err(KsmError::AlreadyArchived(existing_name));
                }
                Err(e) => return Err(e.into()),
            }
        };

        {
            let mut stmt = tx.prepare(
                "INSERT INTO chunks (archive_id, exchange_index, user_content, assistant_content, tool_summary)
                 VALUES (?, ?, ?, ?, ?)",
            )?;

            for chunk in chunks {
                stmt.execute(rusqlite::params![
                    archive_id,
                    chunk.exchange_index,
                    chunk.user_content,
                    chunk.assistant_content,
                    chunk.tool_summary,
                ])?;
            }
        }

        tx.commit()?;
        Ok(archive_id)
    }

    /// Update an existing archive's chunks (for reindex).
    pub fn update_archive(
        &self,
        archive_id: i64,
        message_count: u32,
        chunks: &[NewChunk],
    ) -> Result<()> {
        let conn = self.open()?;
        let tx = conn.unchecked_transaction()?;

        // Delete existing chunks
        tx.execute("DELETE FROM chunks WHERE archive_id = ?", [archive_id])?;

        // Insert new chunks
        {
            let mut stmt = tx.prepare(
                "INSERT INTO chunks (archive_id, exchange_index, user_content, assistant_content, tool_summary)
                 VALUES (?, ?, ?, ?, ?)",
            )?;

            for chunk in chunks {
                stmt.execute(rusqlite::params![
                    archive_id,
                    chunk.exchange_index,
                    chunk.user_content,
                    chunk.assistant_content,
                    chunk.tool_summary,
                ])?;
            }
        }

        // Update message count
        tx.execute(
            "UPDATE archives SET message_count = ? WHERE id = ?",
            rusqlite::params![message_count, archive_id],
        )?;

        tx.commit()?;
        Ok(())
    }

    /// Set the is_indexed flag on an archive.
    pub fn set_indexed(&self, archive_id: i64, is_indexed: bool) -> Result<()> {
        let conn = self.open()?;
        conn.execute(
            "UPDATE archives SET is_indexed = ? WHERE id = ?",
            rusqlite::params![is_indexed, archive_id],
        )?;
        Ok(())
    }

    /// Search archives using FTS5 full-text search.
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let conn = self.open()?;

        let mut stmt = conn
            .prepare(
                "SELECT
                    a.name,
                    a.session_id,
                    c.exchange_index,
                    snippet(chunks_fts, 0, '>>>', '<<<', '...', 32) as user_snippet,
                    snippet(chunks_fts, 1, '>>>', '<<<', '...', 32) as assistant_snippet,
                    snippet(chunks_fts, 2, '>>>', '<<<', '...', 32) as tool_snippet,
                    rank
                FROM chunks_fts
                JOIN chunks c ON c.id = chunks_fts.rowid
                JOIN archives a ON a.id = c.archive_id
                WHERE chunks_fts MATCH ?1
                    AND a.directory = ?2
                ORDER BY rank
                LIMIT ?3",
            )
            .map_err(|e| KsmError::SearchError(format!("Invalid search query: {}", e)))?;

        let rows = stmt
            .query_map(
                rusqlite::params![query.query, query.directory, query.limit],
                |row| {
                    Ok(SearchResult {
                        archive_name: row.get(0)?,
                        archive_session_id: row.get(1)?,
                        exchange_index: row.get(2)?,
                        user_snippet: row.get(3)?,
                        assistant_snippet: row.get(4)?,
                        tool_snippet: row.get::<_, Option<String>>(5)?,
                        rank: row.get(6)?,
                    })
                },
            )
            .map_err(|e| KsmError::SearchError(format!("Search failed: {}", e)))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Get a single archive by name within a directory.
    pub fn get_archive(&self, name: &str, directory: &str) -> Result<Archive> {
        let conn = self.open()?;
        conn.prepare(
            "SELECT id, session_id, name, directory, message_count, session_created_at, archived_at, tags, pruned, is_indexed
             FROM archives WHERE name = ? AND directory = ?",
        )?
        .query_row(rusqlite::params![name, directory], |row| {
            Self::row_to_archive(row)
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => KsmError::ArchiveNotFound(name.to_string()),
            other => other.into(),
        })
    }

    /// Get archive by database ID.
    pub fn get_archive_by_id(&self, archive_id: i64) -> Result<Archive> {
        let conn = self.open()?;
        conn.prepare(
            "SELECT id, session_id, name, directory, message_count, session_created_at, archived_at, tags, pruned, is_indexed
             FROM archives WHERE id = ?",
        )?
        .query_row([archive_id], Self::row_to_archive)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                KsmError::ArchiveNotFound(format!("id={}", archive_id))
            }
            other => other.into(),
        })
    }

    /// Get all chunks for an archive, ordered by exchange_index.
    pub fn get_chunks(&self, archive_id: i64) -> Result<Vec<Chunk>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, archive_id, exchange_index, user_content, assistant_content, tool_summary
             FROM chunks WHERE archive_id = ? ORDER BY exchange_index",
        )?;

        let rows = stmt.query_map([archive_id], |row| {
            Ok(Chunk {
                id: row.get(0)?,
                archive_id: row.get(1)?,
                exchange_index: row.get(2)?,
                user_content: row.get(3)?,
                assistant_content: row.get(4)?,
                tool_summary: row.get(5)?,
            })
        })?;

        let mut chunks = Vec::new();
        for row in rows {
            chunks.push(row?);
        }
        Ok(chunks)
    }

    /// List archives for a directory (is_indexed = false only).
    pub fn list_archives(&self, directory: &str) -> Result<Vec<Archive>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, name, directory, message_count, session_created_at, archived_at, tags, pruned, is_indexed
             FROM archives WHERE directory = ? AND is_indexed = FALSE ORDER BY archived_at DESC",
        )?;

        let rows = stmt.query_map([directory], Self::row_to_archive)?;

        let mut archives = Vec::new();
        for row in rows {
            archives.push(row?);
        }
        Ok(archives)
    }

    /// List indexed sessions for a directory (is_indexed = true only).
    pub fn list_indexed(&self, directory: &str) -> Result<Vec<Archive>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, name, directory, message_count, session_created_at, archived_at, tags, pruned, is_indexed
             FROM archives WHERE directory = ? AND is_indexed = TRUE ORDER BY archived_at DESC",
        )?;

        let rows = stmt.query_map([directory], Self::row_to_archive)?;

        let mut archives = Vec::new();
        for row in rows {
            archives.push(row?);
        }
        Ok(archives)
    }

    /// Delete an archive and all its chunks.
    pub fn delete_archive(&self, archive_id: i64) -> Result<i64> {
        let conn = self.open()?;

        let chunk_count: i64 = conn
            .prepare("SELECT COUNT(*) FROM chunks WHERE archive_id = ?")?
            .query_row([archive_id], |row| row.get(0))?;

        conn.execute("DELETE FROM archives WHERE id = ?", [archive_id])?;

        Ok(chunk_count)
    }

    /// Helper to convert a row to Archive.
    fn row_to_archive(row: &rusqlite::Row) -> rusqlite::Result<Archive> {
        let tags_json: Option<String> = row.get(7)?;
        let tags: Vec<String> = tags_json
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();

        Ok(Archive {
            id: row.get(0)?,
            session_id: row.get(1)?,
            name: row.get(2)?,
            directory: row.get(3)?,
            message_count: row.get(4)?,
            session_created_at: row.get(5)?,
            archived_at: row.get(6)?,
            tags,
            pruned: row.get::<_, i64>(8)? != 0,
            is_indexed: row.get::<_, i64>(9)? != 0,
        })
    }

    // ========== JSON Migration ==========

    /// One-time migration from metadata.json to SQLite.
    pub fn migrate_from_json(&self) -> Result<usize> {
        let json_path = metadata_path()?;

        if !json_path.exists() {
            return Ok(0);
        }

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
                 VALUES (?, ?, ?, ?, ?, ?)",
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
        };

        tx.commit()?;
        debug!("Migrated {} metadata entries from JSON to SQLite", count);
        Ok(count)
    }
}
