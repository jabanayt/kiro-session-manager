use serde::Deserialize;

/// A kiro-cli chat session with raw (unformatted) data.
///
/// Timestamps are milliseconds since epoch. Message count is an integer.
/// All formatting (time_ago, "X msgs") happens in the CLI/TUI layer.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub created_at: i64,    // ms since epoch
    pub updated_at: i64,    // ms since epoch
    pub preview: String,
    pub msg_count: u32,
}

// --- JSON structures for parsing kiro-cli database conversation data ---

/// Top-level conversation JSON from kiro-cli's conversations_v2.value column.
#[derive(Debug, Deserialize)]
pub struct ConversationData {
    #[allow(dead_code)]
    #[serde(default)]
    pub conversation_id: String,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    #[serde(default)]
    pub latest_summary: Option<Vec<serde_json::Value>>,
}

/// A single history entry (one user turn + metadata).
#[derive(Debug, Deserialize)]
pub struct HistoryEntry {
    #[serde(default)]
    pub user: Option<UserMessage>,
    #[serde(default)]
    pub request_metadata: Option<RequestMetadata>,
}

/// User message content wrapper.
#[derive(Debug, Deserialize)]
pub struct UserMessage {
    #[serde(default)]
    pub content: Option<serde_json::Value>,
}

/// Per-request metadata containing message ID.
#[derive(Debug, Deserialize)]
pub struct RequestMetadata {
    #[serde(default)]
    pub message_id: Option<String>,
}

/// Extract preview text from conversation data.
///
/// Returns first user message (truncated to 100 chars), "[Compacted session]",
/// or "[No preview available]".
pub fn extract_preview(data: &ConversationData) -> String {
    if let Some(first_entry) = data.history.first()
        && let Some(user_msg) = &first_entry.user
            && let Some(content) = &user_msg.content
                && let Some(prompt_obj) = content.get("Prompt")
                    && let Some(prompt_text) = prompt_obj.get("prompt")
                        && let Some(text) = prompt_text.as_str() {
                            return text.chars().take(100).collect();
                        }

    if data.history.is_empty() && data.latest_summary.is_some() {
        return "[Compacted session]".to_string();
    }

    "[No preview available]".to_string()
}
