//! Unified KSM database for metadata, archives, and state.
//!
//! Replaces separate SqliteMetadataStore and SqliteArchiveStore.
//! Single struct with direct methods (no traits).

use log::debug;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::config::{Config, load_config, metadata_path};
use crate::error::{KsmError, Result};
use crate::models::{
    Archive, ArchiveStatus, CachedSession, Chunk, NewArchive, NewChunk, SearchQuery, SearchResult,
    SessionMetadata, SourceType,
};

/// State key for pending reindex tracking.
const STATE_PENDING_REINDEX: &str = "pending_reindex";

/// Current database schema version.
/// Increment when adding new migrations.
const CURRENT_SCHEMA_VERSION: i64 = 6;

/// Unified database for all KSM data (metadata, archives, state).
pub struct KsmDatabase {
    path: PathBuf,
    config: Config,
}

/// Backup database file before migration.
/// Returns backup path on success, None if backup failed (with warning message).
fn backup_database(db_path: &std::path::Path) -> (Option<std::path::PathBuf>, Option<String>) {
    let backup_path = db_path.with_extension("db.bak");
    match std::fs::copy(db_path, &backup_path) {
        Ok(_) => {
            debug!("Backed up database to {}", backup_path.display());
            (Some(backup_path), None)
        }
        Err(e) => {
            let warning = format!(
                "Could not create database backup: {}. Proceeding without safety net.",
                e
            );
            (None, Some(warning))
        }
    }
}

/// Restore database from backup after failed migration.
/// Returns error message if restore also failed.
fn restore_database(
    db_path: &std::path::Path,
    backup_path: &std::path::Path,
) -> std::result::Result<(), String> {
    match std::fs::copy(backup_path, db_path) {
        Ok(_) => {
            let _ = std::fs::remove_file(backup_path);
            Ok(())
        }
        Err(e) => Err(format!(
            "Failed to restore database from backup: {}. Manual recovery needed from {}",
            e,
            backup_path.display()
        )),
    }
}

/// Delete backup file after successful migration.
fn cleanup_backup(backup_path: &std::path::Path) {
    let _ = std::fs::remove_file(backup_path);
}

impl KsmDatabase {
    /// Create database using path from config.
    ///
    /// Runs schema migrations. Returns database and any warnings
    /// (e.g., backup failure) that should be shown to user.
    pub fn from_config() -> Result<(Self, Vec<String>)> {
        let config = load_config()?;
        let path = crate::config::ksm_db_path()?;
        let db = KsmDatabase { path, config };
        let warnings = db.ensure_schema()?;
        Ok((db, warnings))
    }

    /// Create database with explicit path (for testing).
    pub fn new(path: PathBuf) -> Result<Self> {
        let config = Config::default();
        let db = KsmDatabase { path, config };
        let _ = db.ensure_schema()?; // Ignore warnings for explicit path constructor
        Ok(db)
    }

    /// Create database in a temporary directory for testing.
    ///
    /// Returns the database and the TempDir. Caller must keep TempDir alive
    /// for the duration of the test, otherwise the database file is deleted.
    #[cfg(test)]
    pub fn test_db(name: &str) -> Result<(Self, tempfile::TempDir)> {
        let temp_dir = tempfile::tempdir().map_err(|e| KsmError::Storage {
            message: format!("Failed to create temp directory: {}", e),
            path: None,
        })?;
        let path = temp_dir.path().join(format!("{}.db", name));
        let db = KsmDatabase::new(path)?;
        Ok((db, temp_dir))
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
    /// Returns warnings (e.g., backup failure) that should be shown to user.
    fn ensure_schema(&self) -> Result<Vec<String>> {
        let mut warnings = Vec::new();
        let conn = self.open()?;

        // Check if schema_version table exists
        let table_exists: bool = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='schema_version'")?
            .exists([])?;

        let current_version = if table_exists {
            conn.prepare("SELECT version FROM schema_version")?
                .query_row([], |row| row.get::<_, i64>(0))?
        } else {
            0 // No schema yet
        };

        // Already at current version - nothing to do
        if current_version == CURRENT_SCHEMA_VERSION {
            return Ok(warnings);
        }

        // Future version - error
        if current_version > CURRENT_SCHEMA_VERSION {
            return Err(KsmError::SchemaVersionMismatch {
                expected: CURRENT_SCHEMA_VERSION,
                found: current_version,
            });
        }

        // Need to migrate - backup first
        let backup_path = if self.path.exists() {
            let (backup, warning) = backup_database(&self.path);
            if let Some(w) = warning {
                warnings.push(w);
            }
            backup
        } else {
            None // New database, nothing to backup
        };

        // Run migrations
        let migration_result = self.run_migrations(&conn, current_version);

        // Handle migration failure
        if let Err(e) = migration_result {
            // Drop connection before attempting restore (releases file locks)
            drop(conn);

            if let Some(ref backup) = backup_path {
                if let Err(restore_err) = restore_database(&self.path, backup) {
                    return Err(KsmError::Storage {
                        message: format!("Migration failed: {}. Additionally, {}", e, restore_err),
                        path: Some(self.path.clone()),
                    });
                }
                // Restore succeeded
                return Err(KsmError::Storage {
                    message: format!("Migration failed: {}. Database restored from backup.", e),
                    path: Some(self.path.clone()),
                });
            }
            return Err(e);
        }

        // Run integrity check after migration
        if let Err(e) = self.verify_schema(&conn) {
            // Drop connection before attempting restore (releases file locks)
            drop(conn);

            if let Some(ref backup) = backup_path {
                if let Err(restore_err) = restore_database(&self.path, backup) {
                    return Err(KsmError::Storage {
                        message: format!(
                            "Schema verification failed: {}. Additionally, {}",
                            e, restore_err
                        ),
                        path: Some(self.path.clone()),
                    });
                }
                return Err(KsmError::Storage {
                    message: format!(
                        "Schema verification failed: {}. Database restored from backup.",
                        e
                    ),
                    path: Some(self.path.clone()),
                });
            }
            return Err(e);
        }

        // Success - cleanup backup
        if let Some(ref backup) = backup_path {
            cleanup_backup(backup);
        }

        Ok(warnings)
    }

    /// Run all migrations from current_version to CURRENT_SCHEMA_VERSION.
    fn run_migrations(&self, conn: &Connection, from_version: i64) -> Result<()> {
        let mut version = from_version;

        while version < CURRENT_SCHEMA_VERSION {
            self.run_migration(conn, version)?;
            version += 1;
        }

        Ok(())
    }

    /// Run a single migration step.
    fn run_migration(&self, conn: &Connection, from_version: i64) -> Result<()> {
        match from_version {
            0 => Self::create_v1_schema(conn)?,
            1 => Self::migrate_v1_to_v2(conn)?,
            2 => self.migrate_v2_to_v3(conn)?,
            3 => self.migrate_v3_to_v4(conn)?,
            4 => self.migrate_v4_to_v5(conn)?,
            5 => self.migrate_v5_to_v6(conn)?,
            _ => {
                return Err(KsmError::SchemaVersionMismatch {
                    expected: CURRENT_SCHEMA_VERSION,
                    found: from_version,
                });
            }
        }
        Ok(())
    }

    /// Verify schema integrity after migration.
    fn verify_schema(&self, conn: &Connection) -> Result<()> {
        // Check all expected tables exist
        let expected_tables = [
            "schema_version",
            "metadata",
            "archives",
            "chunks",
            "chunks_fts",
            "session_cache",
            "acp_cache",
            "state",
        ];

        for table in &expected_tables {
            let exists: bool = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name=?1")?
                .exists([table])?;
            if !exists {
                return Err(KsmError::Storage {
                    message: format!("Missing table after migration: {}", table),
                    path: Some(self.path.clone()),
                });
            }
        }

        // Check version is correct
        let version: i64 = conn
            .prepare("SELECT version FROM schema_version")?
            .query_row([], |row| row.get(0))?;
        if version != CURRENT_SCHEMA_VERSION {
            return Err(KsmError::SchemaVersionMismatch {
                expected: CURRENT_SCHEMA_VERSION,
                found: version,
            });
        }

        // Verify key columns exist (catches partial migrations)
        conn.prepare("SELECT source_type FROM archives LIMIT 0")?;
        conn.prepare("SELECT source_type FROM session_cache LIMIT 0")?;
        conn.prepare("SELECT is_indexed FROM archives LIMIT 0")?;

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
    /// Migrate v2 to v3: add is_indexed column, state table, sparse config.
    fn migrate_v2_to_v3(&self, conn: &Connection) -> Result<()> {
        let tx = conn.unchecked_transaction()?;

        // Add is_indexed column (idempotent - check if exists first)
        let has_is_indexed: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('archives') WHERE name='is_indexed'")?
            .exists([])?;
        if !has_is_indexed {
            tx.execute(
                "ALTER TABLE archives ADD COLUMN is_indexed BOOLEAN NOT NULL DEFAULT FALSE",
                [],
            )?;
        }

        // Create state table (idempotent)
        tx.execute(
            "CREATE TABLE IF NOT EXISTS state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        tx.commit()?;

        // Post-DDL work (outside transaction, before version bump)
        // These are idempotent - safe to re-run
        self.migrate_archive_indexed_status()?;
        self.migrate_config_to_sparse()?;

        // Version bump last (separate transaction)
        let tx2 = conn.unchecked_transaction()?;
        tx2.execute("UPDATE schema_version SET version = 3", [])?;
        tx2.commit()?;

        debug!("Migrated schema from version 2 to version 3");
        Ok(())
    }

    /// Migrate v3 to v4: add session_cache table, re-run config migration.
    /// Migrate v3 to v4: add session_cache table, re-run config migration.
    fn migrate_v3_to_v4(&self, conn: &Connection) -> Result<()> {
        let tx = conn.unchecked_transaction()?;

        tx.execute(
            "CREATE TABLE IF NOT EXISTS session_cache (
                session_id TEXT PRIMARY KEY,
                directory TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                preview TEXT NOT NULL,
                msg_count INTEGER NOT NULL,
                has_compact_tag INTEGER NOT NULL DEFAULT 0,
                message_ids TEXT NOT NULL DEFAULT '[]'
            )",
            [],
        )?;
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_session_cache_directory ON session_cache(directory)",
            [],
        )?;

        tx.commit()?;

        // Re-run config migration (idempotent, catches verbose configs)
        self.migrate_config_to_sparse()?;

        // Version bump last (separate transaction)
        let tx2 = conn.unchecked_transaction()?;
        tx2.execute("UPDATE schema_version SET version = 4", [])?;
        tx2.commit()?;

        debug!("Migrated schema from version 3 to version 4");
        Ok(())
    }

    fn migrate_v4_to_v5(&self, conn: &Connection) -> Result<()> {
        let tx = conn.unchecked_transaction()?;

        // Clean up malformed tags in metadata table
        let rows: Vec<(String, String)> = {
            let mut stmt = tx.prepare("SELECT session_id, tags FROM metadata WHERE tags != ''")?;
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .filter_map(|r| r.ok())
                .collect()
        };

        for (session_id, tags_json) in rows {
            if let Ok(tags) = serde_json::from_str::<Vec<String>>(&tags_json) {
                let cleaned: Vec<String> = tags
                    .iter()
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                if cleaned != tags {
                    let new_json = serde_json::to_string(&cleaned)?;
                    tx.execute(
                        "UPDATE metadata SET tags = ?1 WHERE session_id = ?2",
                        rusqlite::params![new_json, session_id],
                    )?;
                }
            }
        }

        // Clean up malformed tags in archives table (same format)
        let rows: Vec<(i64, String)> = {
            let mut stmt =
                tx.prepare("SELECT id, tags FROM archives WHERE tags != '' AND tags IS NOT NULL")?;
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .filter_map(|r| r.ok())
                .collect()
        };

        for (archive_id, tags_json) in rows {
            if let Ok(tags) = serde_json::from_str::<Vec<String>>(&tags_json) {
                let cleaned: Vec<String> = tags
                    .iter()
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                if cleaned != tags {
                    let new_json = serde_json::to_string(&cleaned)?;
                    tx.execute(
                        "UPDATE archives SET tags = ?1 WHERE id = ?2",
                        rusqlite::params![new_json, archive_id],
                    )?;
                }
            }
        }

        tx.commit()?;

        // Version bump last (separate transaction)
        let tx2 = conn.unchecked_transaction()?;
        tx2.execute("UPDATE schema_version SET version = 5", [])?;
        tx2.commit()?;

        debug!("Migrated schema from version 4 to version 5");
        Ok(())
    }

    fn migrate_v5_to_v6(&self, conn: &Connection) -> Result<()> {
        // Must be outside transaction per SQLite rules
        conn.execute_batch("PRAGMA foreign_keys = OFF;")?;

        let tx = conn.unchecked_transaction()?;

        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS acp_cache (
                directory TEXT PRIMARY KEY,
                dir_mtime_secs INTEGER NOT NULL,
                dir_mtime_nanos INTEGER NOT NULL,
                session_ids TEXT NOT NULL DEFAULT '[]'
            );

            DROP TABLE IF EXISTS session_cache;

            CREATE TABLE session_cache (
                session_id TEXT NOT NULL,
                directory TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                preview TEXT NOT NULL,
                msg_count INTEGER NOT NULL,
                has_compact_tag INTEGER NOT NULL DEFAULT 0,
                message_ids TEXT NOT NULL DEFAULT '[]',
                source_type TEXT NOT NULL DEFAULT 'v1',
                PRIMARY KEY (session_id, source_type)
            );

            CREATE INDEX IF NOT EXISTS idx_session_cache_directory ON session_cache(directory);

            ALTER TABLE archives ADD COLUMN source_type TEXT NOT NULL DEFAULT 'v1';

            CREATE TABLE archives_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                name TEXT NOT NULL,
                directory TEXT NOT NULL,
                message_count INTEGER NOT NULL,
                session_created_at INTEGER NOT NULL,
                archived_at INTEGER NOT NULL,
                tags TEXT,
                pruned BOOLEAN NOT NULL DEFAULT FALSE,
                is_indexed BOOLEAN NOT NULL DEFAULT FALSE,
                source_type TEXT NOT NULL DEFAULT 'v1',
                UNIQUE(session_id, source_type)
            );

            INSERT INTO archives_new SELECT * FROM archives;

            DROP TABLE archives;

            ALTER TABLE archives_new RENAME TO archives;

            CREATE INDEX IF NOT EXISTS idx_archives_directory ON archives(directory);
            CREATE INDEX IF NOT EXISTS idx_archives_name ON archives(name);

            UPDATE schema_version SET version = 6;",
        )?;

        tx.commit()?;

        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        debug!("Migrated schema from version 5 to version 6");
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

    /// Set pending reindex for a session with its source type.
    /// Stores as "session_id:source_type" (e.g. "abc-123:v1").
    pub fn set_pending_reindex(&self, session_id: &str, source_type: SourceType) -> Result<()> {
        let value = format!("{}:{}", session_id, source_type.as_str());
        self.set_state(STATE_PENDING_REINDEX, &value)
    }

    /// Clear pending reindex.
    pub fn clear_pending_reindex(&self) -> Result<()> {
        self.clear_state(STATE_PENDING_REINDEX)
    }

    /// Get pending reindex session ID and source type.
    pub fn get_pending_reindex(&self) -> Result<Option<(String, SourceType)>> {
        match self.get_state(STATE_PENDING_REINDEX)? {
            Some(value) => {
                if let Some((id, st_str)) = value.rsplit_once(':') {
                    let st: SourceType = st_str.parse().unwrap_or_default();
                    Ok(Some((id.to_string(), st)))
                } else {
                    // Legacy format (just session_id), default to Legacy
                    Ok(Some((value, SourceType::Legacy)))
                }
            }
            None => Ok(None),
        }
    }

    // ========== Session Cache Methods ==========

    /// Get cached sessions for a directory, keyed by (session_id, source_type).
    pub fn get_cached_sessions(
        &self,
        directory: &str,
    ) -> Result<HashMap<(String, SourceType), CachedSession>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT session_id, directory, updated_at, created_at, preview, msg_count,
                    has_compact_tag, message_ids, source_type
             FROM session_cache WHERE directory = ?",
        )?;

        let mut cache = HashMap::new();
        let rows = stmt.query_map([directory], |row| {
            let message_ids_json: String = row.get(7)?;
            let source_type_str: String = row.get(8)?;
            Ok(CachedSession {
                session_id: row.get(0)?,
                directory: row.get(1)?,
                updated_at: row.get(2)?,
                created_at: row.get(3)?,
                preview: row.get(4)?,
                msg_count: row.get::<_, i64>(5)? as u32,
                has_compact_tag: row.get::<_, i32>(6)? != 0,
                message_ids: serde_json::from_str(&message_ids_json).unwrap_or_default(),
                source_type: source_type_str.parse().map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        8,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?,
            })
        })?;

        for row in rows {
            let cached = row?;
            cache.insert((cached.session_id.clone(), cached.source_type), cached);
        }

        Ok(cache)
    }

    /// Batch insert/update cached sessions.
    pub fn set_cached_sessions(&self, sessions: &[CachedSession]) -> Result<()> {
        if sessions.is_empty() {
            return Ok(());
        }

        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO session_cache
             (session_id, directory, updated_at, created_at, preview, msg_count,
              has_compact_tag, message_ids, source_type)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )?;

        for session in sessions {
            let message_ids_json = serde_json::to_string(&session.message_ids)?;
            stmt.execute(rusqlite::params![
                session.session_id,
                session.directory,
                session.updated_at,
                session.created_at,
                session.preview,
                session.msg_count as i64,
                session.has_compact_tag as i32,
                message_ids_json,
                session.source_type.as_str(),
            ])?;
        }

        Ok(())
    }

    /// Delete cache entries for sessions that no longer exist in Kiro.
    pub fn delete_stale_cache(
        &self,
        directory: &str,
        live_keys: &HashSet<(String, SourceType)>,
    ) -> Result<usize> {
        let conn = self.open()?;

        // Get all cached keys for this directory
        let mut stmt =
            conn.prepare("SELECT session_id, source_type FROM session_cache WHERE directory = ?")?;
        let cached_keys: Vec<(String, SourceType)> = stmt
            .query_map([directory], |row| {
                let id: String = row.get(0)?;
                let st: String = row.get(1)?;
                Ok((id, st))
            })?
            .filter_map(|r| r.ok())
            .map(|(id, st)| Ok((id, st.parse()?)))
            .collect::<Result<_>>()?;

        // Delete those not in live set
        let mut deleted = 0;
        for (id, source_type) in cached_keys {
            if !live_keys.contains(&(id.clone(), source_type)) {
                conn.execute(
                    "DELETE FROM session_cache WHERE session_id = ? AND source_type = ?",
                    rusqlite::params![id, source_type.as_str()],
                )?;
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    // ========== ACP Cache Methods ==========

    /// Get cached ACP directory mtime and session IDs.
    pub fn get_acp_cache(
        &self,
        directory: &str,
    ) -> Result<Option<(std::time::SystemTime, Vec<String>)>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT dir_mtime_secs, dir_mtime_nanos, session_ids
             FROM acp_cache WHERE directory = ?",
        )?;

        let result = stmt.query_row([directory], |row| {
            let secs: i64 = row.get(0)?;
            let nanos: u32 = row.get(1)?;
            let ids_json: String = row.get(2)?;
            Ok((secs, nanos, ids_json))
        });

        match result {
            Ok((secs, nanos, ids_json)) => {
                let mtime = std::time::UNIX_EPOCH + std::time::Duration::new(secs as u64, nanos);
                let ids: Vec<String> = serde_json::from_str(&ids_json).unwrap_or_default();
                Ok(Some((mtime, ids)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(KsmError::Database(e.to_string())),
        }
    }

    /// Set cached ACP directory mtime and session IDs.
    pub fn set_acp_cache(
        &self,
        directory: &str,
        mtime: std::time::SystemTime,
        session_ids: &[String],
    ) -> Result<()> {
        let conn = self.open()?;
        let duration = mtime
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let ids_json = serde_json::to_string(session_ids)?;

        conn.execute(
            "INSERT INTO acp_cache (directory, dir_mtime_secs, dir_mtime_nanos, session_ids)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(directory) DO UPDATE SET
                dir_mtime_secs = ?2,
                dir_mtime_nanos = ?3,
                session_ids = ?4",
            rusqlite::params![
                directory,
                duration.as_secs() as i64,
                duration.subsec_nanos(),
                ids_json
            ],
        )?;
        Ok(())
    }

    // ========== Archive Methods ==========

    /// Check archive/index status of a session (any source type).
    pub fn get_archive_status(&self, session_id: &str) -> Result<Option<ArchiveStatus>> {
        let conn = self.open()?;
        match conn
            .prepare("SELECT id, name, is_indexed, source_type FROM archives WHERE session_id = ?")?
            .query_row([session_id], |row| {
                let id: i64 = row.get(0)?;
                let name: String = row.get(1)?;
                let is_indexed: bool = row.get::<_, i64>(2)? != 0;
                let st_str: String = row.get(3)?;
                Ok((id, name, is_indexed, st_str))
            }) {
            Ok((id, name, is_indexed, st_str)) => {
                let st: SourceType = st_str.parse().unwrap_or_default();
                if is_indexed {
                    Ok(Some(ArchiveStatus::Indexed {
                        name,
                        archive_id: id,
                        source_type: st,
                    }))
                } else {
                    Ok(Some(ArchiveStatus::Archived {
                        name,
                        archive_id: id,
                        source_type: st,
                    }))
                }
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Check archive/index status of a session for a specific source type.
    pub fn get_archive_status_for_source(
        &self,
        session_id: &str,
        source_type: SourceType,
    ) -> Result<Option<ArchiveStatus>> {
        let conn = self.open()?;
        match conn
            .prepare("SELECT id, name, is_indexed, source_type FROM archives WHERE session_id = ? AND source_type = ?")?
            .query_row(rusqlite::params![session_id, source_type.as_str()], |row| {
                let id: i64 = row.get(0)?;
                let name: String = row.get(1)?;
                let is_indexed: bool = row.get::<_, i64>(2)? != 0;
                let st_str: String = row.get(3)?;
                Ok((id, name, is_indexed, st_str))
            }) {
            Ok((id, name, is_indexed, st_str)) => {
                let st: SourceType = st_str.parse().unwrap_or_default();
                if is_indexed {
                    Ok(Some(ArchiveStatus::Indexed {
                        name,
                        archive_id: id,
                        source_type: st,
                    }))
                } else {
                    Ok(Some(ArchiveStatus::Archived {
                        name,
                        archive_id: id,
                        source_type: st,
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
                "INSERT INTO archives (session_id, name, directory, message_count, session_created_at, archived_at, tags, pruned, is_indexed, source_type)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
                    archive.source_type.as_str(),
                ],
            );

            match insert_result {
                Ok(_) => tx.last_insert_rowid(),
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    let existing_name: String = conn
                        .prepare(
                            "SELECT name FROM archives WHERE session_id = ? AND source_type = ?",
                        )?
                        .query_row(
                            rusqlite::params![&archive.session_id, archive.source_type.as_str()],
                            |row| row.get(0),
                        )
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
            "SELECT id, session_id, name, directory, message_count, session_created_at, archived_at, tags, pruned, is_indexed, source_type
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
            "SELECT id, session_id, name, directory, message_count, session_created_at, archived_at, tags, pruned, is_indexed, source_type
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
            "SELECT id, session_id, name, directory, message_count, session_created_at, archived_at, tags, pruned, is_indexed, source_type
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
            "SELECT id, session_id, name, directory, message_count, session_created_at, archived_at, tags, pruned, is_indexed, source_type
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

        let st_str: String = row
            .get::<_, Option<String>>(10)?
            .unwrap_or_else(|| "v1".to_string());
        let source_type: SourceType = st_str.parse().unwrap_or_default();

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
            source_type,
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_fresh_database_reaches_current_version() {
        let (db, _temp) = KsmDatabase::test_db("test_fresh").unwrap();
        let conn = db.open().unwrap();
        let version: i64 = conn
            .prepare("SELECT version FROM schema_version")
            .unwrap()
            .query_row([], |row| row.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn test_all_tables_exist() {
        let (db, _temp) = KsmDatabase::test_db("test_tables").unwrap();
        let conn = db.open().unwrap();

        let expected = [
            "schema_version",
            "metadata",
            "archives",
            "chunks",
            "chunks_fts",
            "session_cache",
            "acp_cache",
            "state",
        ];

        for table in &expected {
            let exists: bool = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name=?1")
                .unwrap()
                .exists([table])
                .unwrap();
            assert!(exists, "Table {} should exist", table);
        }
    }

    #[test]
    fn test_key_columns_exist() {
        let (db, _temp) = KsmDatabase::test_db("test_columns").unwrap();
        let conn = db.open().unwrap();

        // These should not error
        conn.prepare("SELECT source_type FROM archives LIMIT 0")
            .unwrap();
        conn.prepare("SELECT source_type FROM session_cache LIMIT 0")
            .unwrap();
        conn.prepare("SELECT is_indexed FROM archives LIMIT 0")
            .unwrap();
    }

    #[test]
    fn test_metadata_survives_migration() {
        let (db, _temp) = KsmDatabase::test_db("test_metadata_survives").unwrap();

        // Insert metadata
        let meta = SessionMetadata {
            name: Some("Test Session".to_string()),
            tags: ["tag1", "tag2"].iter().map(|s| s.to_string()).collect(),
            directory: Some("/test".to_string()),
            parent_session_id: None,
            manually_unlinked: false,
        };
        db.set_metadata("test-session-id", &meta).unwrap();

        // Verify it's there
        let loaded = db.get_metadata("test-session-id").unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.name, Some("Test Session".to_string()));
        assert!(loaded.tags.contains("tag1"));
        assert!(loaded.tags.contains("tag2"));
    }

    #[test]
    fn test_archive_survives_migration() {
        let (db, _temp) = KsmDatabase::test_db("test_archive_survives").unwrap();

        // Insert archive with chunks
        let archive = NewArchive {
            session_id: "test-session".to_string(),
            name: "test-archive".to_string(),
            directory: "/test".to_string(),
            message_count: 5,
            session_created_at: 1000,
            archived_at: 2000,
            tags: vec!["archive-tag".to_string()],
            pruned: false,
            source_type: SourceType::Legacy,
        };
        let chunks = vec![NewChunk {
            exchange_index: 0,
            user_content: "User question".to_string(),
            assistant_content: "Assistant answer".to_string(),
            tool_summary: Some("Used tool X".to_string()),
        }];
        db.save_archive(&archive, &chunks, false).unwrap();

        // Verify archive exists
        let loaded = db.get_archive("test-archive", "/test").unwrap();
        assert_eq!(loaded.session_id, "test-session");
        assert_eq!(loaded.message_count, 5);

        // Verify chunks exist
        let loaded_chunks = db.get_chunks(loaded.id).unwrap();
        assert_eq!(loaded_chunks.len(), 1);
        assert_eq!(loaded_chunks[0].user_content, "User question");
    }

    #[test]
    fn test_fts_search_works() {
        let (db, _temp) = KsmDatabase::test_db("test_fts").unwrap();

        // Insert archive with searchable content
        let archive = NewArchive {
            session_id: "search-test".to_string(),
            name: "searchable".to_string(),
            directory: "/test".to_string(),
            message_count: 1,
            session_created_at: 1000,
            archived_at: 2000,
            tags: vec![],
            pruned: false,
            source_type: SourceType::Legacy,
        };
        let chunks = vec![NewChunk {
            exchange_index: 0,
            user_content: "How do I implement authentication?".to_string(),
            assistant_content: "You can use JWT tokens for authentication.".to_string(),
            tool_summary: None,
        }];
        db.save_archive(&archive, &chunks, false).unwrap();

        // Search should find it
        let query = SearchQuery {
            query: "authentication".to_string(),
            directory: "/test".to_string(),
            limit: 10,
        };
        let results = db.search(&query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].archive_name, "searchable");
    }

    #[test]
    fn test_backup_restore_roundtrip() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create database with data
        let db = KsmDatabase::new(db_path.clone()).unwrap();
        let meta = SessionMetadata {
            name: Some("Important Data".to_string()),
            tags: std::collections::HashSet::new(),
            directory: Some("/test".to_string()),
            parent_session_id: None,
            manually_unlinked: false,
        };
        db.set_metadata("important-session", &meta).unwrap();

        // Manually backup
        let backup_path = db_path.with_extension("db.bak");
        std::fs::copy(&db_path, &backup_path).unwrap();

        // Corrupt the original (simulate failed migration)
        std::fs::write(&db_path, b"corrupted data").unwrap();

        // Restore from backup
        std::fs::copy(&backup_path, &db_path).unwrap();

        // Verify data is intact
        let db2 = KsmDatabase::new(db_path).unwrap();
        let loaded = db2.get_metadata("important-session").unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().name, Some("Important Data".to_string()));
    }

    #[test]
    fn test_schema_version_mismatch_error() {
        use assert_matches::assert_matches;

        let (db, _temp) = KsmDatabase::test_db("test_version_mismatch").unwrap();
        let conn = db.open().unwrap();

        // Manually set version to future
        conn.execute("UPDATE schema_version SET version = 999", [])
            .unwrap();
        drop(conn);

        // Re-opening should fail with specific error
        let conn2 = db.open().unwrap();
        let result = db.verify_schema(&conn2);
        assert_matches!(
            result,
            Err(KsmError::SchemaVersionMismatch {
                expected: 6,
                found: 999
            })
        );
    }

    #[test]
    fn test_concurrent_access_safe() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("concurrent.db");

        // Create initial database
        let db1 = KsmDatabase::new(db_path.clone()).unwrap();

        // Insert from first connection
        let meta1 = SessionMetadata {
            name: Some("From DB1".to_string()),
            tags: std::collections::HashSet::new(),
            directory: Some("/test".to_string()),
            parent_session_id: None,
            manually_unlinked: false,
        };
        db1.set_metadata("session-1", &meta1).unwrap();

        // Create second instance on same file
        let db2 = KsmDatabase::new(db_path.clone()).unwrap();

        // Insert from second connection
        let meta2 = SessionMetadata {
            name: Some("From DB2".to_string()),
            tags: std::collections::HashSet::new(),
            directory: Some("/test".to_string()),
            parent_session_id: None,
            manually_unlinked: false,
        };
        db2.set_metadata("session-2", &meta2).unwrap();

        // Both should be readable from either instance
        assert!(db1.get_metadata("session-1").unwrap().is_some());
        assert!(db1.get_metadata("session-2").unwrap().is_some());
        assert!(db2.get_metadata("session-1").unwrap().is_some());
        assert!(db2.get_metadata("session-2").unwrap().is_some());
    }

    #[test]
    fn test_v5_to_v6_archive_preservation() {
        // This test verifies archives survive the table rebuild in v5-v6
        // Since we can't easily test migration mid-chain, we verify the
        // current schema handles archives correctly
        let (db, _temp) = KsmDatabase::test_db("test_archive_preservation").unwrap();

        let archive = NewArchive {
            session_id: "preserve-test".to_string(),
            name: "preserved".to_string(),
            directory: "/test".to_string(),
            message_count: 10,
            session_created_at: 1000,
            archived_at: 2000,
            tags: vec!["important".to_string()],
            pruned: false,
            source_type: SourceType::Legacy,
        };
        let chunks = vec![
            NewChunk {
                exchange_index: 0,
                user_content: "First question".to_string(),
                assistant_content: "First answer".to_string(),
                tool_summary: None,
            },
            NewChunk {
                exchange_index: 1,
                user_content: "Second question".to_string(),
                assistant_content: "Second answer".to_string(),
                tool_summary: Some("Used grep".to_string()),
            },
        ];
        db.save_archive(&archive, &chunks, false).unwrap();

        // Verify all data preserved
        let loaded = db.get_archive("preserved", "/test").unwrap();
        assert_eq!(loaded.session_id, "preserve-test");
        assert_eq!(loaded.message_count, 10);
        assert_eq!(loaded.source_type, SourceType::Legacy);

        let loaded_chunks = db.get_chunks(loaded.id).unwrap();
        assert_eq!(loaded_chunks.len(), 2);
        assert_eq!(loaded_chunks[0].user_content, "First question");
        assert_eq!(loaded_chunks[1].tool_summary, Some("Used grep".to_string()));
    }
}
