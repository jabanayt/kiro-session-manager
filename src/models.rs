use std::collections::HashSet;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub time_ago: String,
    pub preview: String,
    pub msg_count: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct SessionMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "HashSet::is_empty", default)]
    pub tags: HashSet<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(default)]
    pub manually_unlinked: bool,
}

// JSON structures for parsing kiro-cli database conversation data

#[derive(Debug, Deserialize)]
pub struct ConversationData {
    #[serde(default)]
    pub conversation_id: String,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    #[serde(default)]
    pub latest_summary: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryEntry {
    #[serde(default)]
    pub user: Option<UserMessage>,
    #[serde(default)]
    pub request_metadata: Option<RequestMetadata>,
}

#[derive(Debug, Deserialize)]
pub struct UserMessage {
    #[serde(default)]
    pub content: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct RequestMetadata {
    #[serde(default)]
    pub message_id: Option<String>,
}
