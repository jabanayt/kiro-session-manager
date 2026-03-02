/// A kiro-cli chat session with raw (unformatted) data.
///
/// Timestamps are milliseconds since epoch. Message count is an integer.
/// All formatting (time_ago, "X msgs") happens in the CLI/TUI layer.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub preview: String,
    pub msg_count: u32,
}
