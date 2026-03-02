//! Archive domain types.
//!
//! These types represent archived sessions, content chunks, search queries,
//! and search results. Used by services/archive.rs and data/archive_store.rs.

/// An archived session record.
///
/// Stored in the `archives` table. Contains metadata about the archived
/// session but not the content itself (content is in chunks).
#[derive(Debug, Clone)]
pub struct Archive {
    /// Database row ID.
    pub id: i64,
    /// Original kiro-cli session ID.
    pub session_id: String,
    /// User-assigned archive name.
    pub name: String,
    /// Project directory the session belonged to.
    pub directory: String,
    /// Number of messages in the original session.
    pub message_count: u32,
    /// When the original session was created (ms since epoch).
    pub session_created_at: i64,
    /// When the session was archived (ms since epoch).
    pub archived_at: i64,
    /// Tags (from session metadata at archive time).
    pub tags: Vec<String>,
    /// Whether the session was pruned before archiving.
    pub pruned: bool,
}

/// A single content chunk (one user-assistant exchange).
///
/// Stored in the `chunks` table. Each chunk represents everything
/// between one user prompt and the next.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Database row ID.
    pub id: i64,
    /// Parent archive ID.
    pub archive_id: i64,
    /// Position in the conversation (0-based).
    pub exchange_index: i32,
    /// Cleaned user content (prompt text).
    pub user_content: String,
    /// Cleaned assistant content (response text).
    pub assistant_content: String,
    /// Tool call summaries for this exchange (if any).
    pub tool_summary: Option<String>,
}

/// Parameters for creating a new archive.
///
/// Passed from the service to the data layer. Separates "what to save"
/// from "how to save it".
#[derive(Debug)]
pub struct NewArchive {
    pub session_id: String,
    pub name: String,
    pub directory: String,
    pub message_count: u32,
    pub session_created_at: i64,
    pub archived_at: i64,
    pub tags: Vec<String>,
    pub pruned: bool,
}

/// A single chunk to be inserted alongside a new archive.
#[derive(Debug)]
pub struct NewChunk {
    pub exchange_index: i32,
    pub user_content: String,
    pub assistant_content: String,
    pub tool_summary: Option<String>,
}

/// Search query parameters.
#[derive(Debug)]
pub struct SearchQuery {
    /// FTS5 query string (passed through to SQLite).
    pub query: String,
    /// Filter to archives in this directory only.
    pub directory: String,
    /// Maximum number of results.
    pub limit: u32,
}

/// A single search result (one matching chunk with archive context).
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The archive this chunk belongs to.
    pub archive_name: String,
    /// The archive's session ID.
    /// Included for future use (TUI, cross-referencing) but not displayed
    /// in Phase 1 CLI output.
    pub archive_session_id: String,
    /// Exchange index within the archive.
    pub exchange_index: i32,
    /// Snippet from user content (with FTS5 highlighting if available).
    pub user_snippet: String,
    /// Snippet from assistant content (with FTS5 highlighting if available).
    pub assistant_snippet: String,
    /// Tool summary snippet (if matched).
    pub tool_snippet: Option<String>,
    /// FTS5 rank score (lower is more relevant).
    pub rank: f64,
}

/// Result of an archive operation (returned by service to CLI).
#[derive(Debug)]
pub struct ArchiveResult {
    pub archive_name: String,
    pub chunk_count: usize,
    pub message_count: u32,
    pub pruned: bool,
}

/// Result of a show-archive operation.
#[derive(Debug)]
pub struct ShowArchiveResult {
    pub archive: Archive,
    pub chunks: Vec<Chunk>,
}

/// Result of a delete-archive operation.
#[derive(Debug)]
pub struct DeleteArchiveResult {
    pub archive_name: String,
    pub message_count: u32,
}
