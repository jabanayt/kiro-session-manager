//! SQLite/FTS5-backed archive store.
//!
//! Reads and writes to the archives, chunks, and chunks_fts tables
//! in ksm.db. FTS5 index is maintained automatically via triggers.

use rusqlite::Connection;
use std::path::PathBuf;

use crate::config::ksm_db_path;
use crate::data::ArchiveStore;
use crate::error::{KsmError, Result};
use crate::models::{Archive, Chunk, NewArchive, NewChunk, SearchQuery, SearchResult};

/// Archive store backed by SQLite with FTS5 full-text search.
pub struct SqliteArchiveStore {
    path: PathBuf,
}

impl SqliteArchiveStore {
    /// Create store using path from config.
    ///
    /// Verifies schema_version >= 2 (archive tables exist). If not,
    /// returns an error directing the user to run any ksm command first
    /// to trigger database migration.
    pub fn from_config() -> Result<Self> {
        let path = ksm_db_path()?;
        let conn = Connection::open(&path).map_err(|e| KsmError::Storage {
            message: format!("Failed to open database: {}", e),
            path: Some(path.clone()),
        })?;

        let version: i64 = conn
            .prepare("SELECT version FROM schema_version")
            .and_then(|mut stmt| stmt.query_row([], |row| row.get(0)))
            .map_err(|e| KsmError::Storage {
                message: format!("Failed to read schema version: {}", e),
                path: Some(path.clone()),
            })?;

        if version < 2 {
            return Err(KsmError::SchemaVersionMismatch {
                expected: 2,
                found: version,
            });
        }

        Ok(SqliteArchiveStore { path })
    }

    /// Create store with explicit path (for testing).
    pub fn new(path: PathBuf) -> Self {
        SqliteArchiveStore { path }
    }

    /// Open read-write connection to ksm.db.
    fn open(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path).map_err(|e| KsmError::Storage {
            message: format!("Failed to open database: {}", e),
            path: Some(self.path.clone()),
        })?;

        conn.pragma_update(None, "foreign_keys", true)
            .map_err(|e| KsmError::Storage {
                message: format!("Failed to enable foreign keys: {}", e),
                path: Some(self.path.clone()),
            })?;

        Ok(conn)
    }
}

impl ArchiveStore for SqliteArchiveStore {
    fn save_archive(&self, archive: &NewArchive, chunks: &[NewChunk]) -> Result<i64> {
        let conn = self.open()?;
        let tx = conn.unchecked_transaction()?;

        let tags_json = if archive.tags.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&archive.tags)?)
        };

        let archive_id = {
            let insert_result = tx.execute(
                "INSERT INTO archives (session_id, name, directory, message_count, session_created_at, archived_at, tags, pruned)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                rusqlite::params![
                    archive.session_id,
                    archive.name,
                    archive.directory,
                    archive.message_count,
                    archive.session_created_at,
                    archive.archived_at,
                    tags_json,
                    archive.pruned,
                ],
            );

            match insert_result {
                Ok(_) => tx.last_insert_rowid(),
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    // session_id UNIQUE constraint -- look up existing archive name
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

    fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
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

    fn list_archives(&self, directory: &str) -> Result<Vec<Archive>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, name, directory, message_count, session_created_at, archived_at, tags, pruned
             FROM archives WHERE directory = ? ORDER BY archived_at DESC",
        )?;

        let rows = stmt.query_map([directory], |row| {
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
            })
        })?;

        let mut archives = Vec::new();
        for row in rows {
            archives.push(row?);
        }
        Ok(archives)
    }

    fn get_archive(&self, name: &str, directory: &str) -> Result<Archive> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, name, directory, message_count, session_created_at, archived_at, tags, pruned
             FROM archives WHERE name = ? AND directory = ?",
        )?;

        stmt.query_row(rusqlite::params![name, directory], |row| {
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
            })
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => KsmError::ArchiveNotFound(name.to_string()),
            other => other.into(),
        })
    }

    fn get_chunks(&self, archive_id: i64) -> Result<Vec<Chunk>> {
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

    fn delete_archive(&self, archive_id: i64) -> Result<i64> {
        let conn = self.open()?;

        let chunk_count: i64 = conn
            .prepare("SELECT COUNT(*) FROM chunks WHERE archive_id = ?")?
            .query_row([archive_id], |row| row.get(0))?;

        conn.execute("DELETE FROM archives WHERE id = ?", [archive_id])?;

        Ok(chunk_count)
    }

    fn is_archived(&self, session_id: &str) -> Result<Option<String>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare("SELECT name FROM archives WHERE session_id = ?")?;

        match stmt.query_row([session_id], |row| row.get::<_, String>(0)) {
            Ok(name) => Ok(Some(name)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
