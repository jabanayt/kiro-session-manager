/// Whether a session comes from the legacy SQLite database or the ACP file store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum SourceType {
    #[default]
    Legacy,
    Acp,
}

impl SourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceType::Acp => "v2",
            SourceType::Legacy => "v1",
        }
    }
}

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
    pub source_type: SourceType,
}
