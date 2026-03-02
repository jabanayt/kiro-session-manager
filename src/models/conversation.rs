//! Typed representations of kiro-cli's conversation JSON.
//!
//! These types model the `conversations_v2.value` column from Kiro's SQLite
//! database. All variants are exhaustively verified against 106 conversations
//! and 13,342 history entries.

use serde::Deserialize;

/// Top-level conversation JSON from kiro-cli's conversations_v2.value column.
///
/// Unknown fields are silently ignored (serde default behaviour).
/// Only fields needed by KSM are included.
#[derive(Debug, Deserialize)]
pub struct ConversationData {
    #[serde(default)]
    pub conversation_id: String,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    /// Stays as `serde_json::Value` despite discovery doc decision #4 (uniform
    /// typed access). The array is heterogeneous `[String, RequestMetadata]`
    /// which would require a custom deserialiser for no practical benefit --
    /// only `has_compact_tag` reads this field, and it already works via Value.
    #[serde(default)]
    pub latest_summary: Option<Vec<serde_json::Value>>,
}

/// A single history entry: one request-response cycle.
///
/// `user` and `assistant` are never null in practice (0/13,342),
/// but kept as Option for defensive deserialisation.
/// `request_metadata` is null in 18/13,342 entries (CancelledToolUses
/// and one ToolUseResults case).
#[derive(Debug, Deserialize)]
pub struct HistoryEntry {
    #[serde(default)]
    pub user: Option<UserMessage>,
    #[serde(default)]
    pub assistant: Option<AssistantContent>,
    #[serde(default)]
    pub request_metadata: Option<RequestMetadata>,
}

/// User-side of a history entry.
///
/// `content` is the typed enum. `additional_context` is always empty string.
/// `images` is usually null (19/13,342 have image data, not useful for archiving).
#[derive(Debug, Deserialize)]
pub struct UserMessage {
    #[serde(default)]
    pub content: Option<UserContent>,
    #[serde(default)]
    pub timestamp: Option<String>,
}

// --- User content variants (3, exhaustively verified) ---

/// The content of a user message.
///
/// Three variants observed across all conversations:
/// - `Prompt` -- user typed a message
/// - `ToolUseResults` -- tool execution results sent back to the model
/// - `CancelledToolUses` -- user cancelled tool execution mid-flight
#[derive(Debug, Deserialize)]
pub enum UserContent {
    Prompt(PromptContent),
    ToolUseResults(ToolUseResultsContent),
    CancelledToolUses(CancelledToolUsesContent),
}

#[derive(Debug, Deserialize)]
pub struct PromptContent {
    pub prompt: String,
}

#[derive(Debug, Deserialize)]
pub struct ToolUseResultsContent {
    pub tool_use_results: Vec<ToolResult>,
}

#[derive(Debug, Deserialize)]
pub struct CancelledToolUsesContent {
    #[serde(default)]
    pub prompt: Option<String>,
    pub tool_use_results: Vec<ToolResult>,
}

// --- Assistant content variants (2, exhaustively verified) ---

/// The content of an assistant message.
///
/// Two variants observed:
/// - `Response` -- text-only reply
/// - `ToolUse` -- text reply with one or more tool calls
#[derive(Debug, Deserialize)]
pub enum AssistantContent {
    Response(ResponseContent),
    ToolUse(ToolUseContent),
}

#[derive(Debug, Deserialize)]
pub struct ResponseContent {
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ToolUseContent {
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub tool_uses: Vec<ToolCall>,
}

// --- Tool types ---

/// A single tool call made by the assistant.
///
/// `args` and `orig_args` remain as `serde_json::Value` because tool arguments
/// are tool-specific and unbounded. The archive service reads specific known
/// keys per tool name for summarisation.
#[derive(Debug, Deserialize)]
pub struct ToolCall {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

/// A single tool result returned to the model.
#[derive(Debug, Deserialize)]
pub struct ToolResult {
    #[serde(default)]
    pub tool_use_id: String,
    #[serde(default)]
    pub content: Vec<ToolResultContent>,
    #[serde(default)]
    pub status: ToolResultStatus,
}

/// Content within a tool result (2 variants, exhaustively verified).
#[derive(Debug, Deserialize)]
pub enum ToolResultContent {
    Text(String),
    Json(serde_json::Value),
}

/// Status of a tool result (2 variants, exhaustively verified).
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub enum ToolResultStatus {
    #[default]
    Success,
    Error,
}

// --- Request metadata ---

/// Per-request metadata. Only fields needed by KSM are included.
///
/// `message_id` is used by chain detection (get_message_ids).
/// `message_meta_tags` is used by compact detection (has_compact_tag).
#[derive(Debug, Deserialize)]
pub struct RequestMetadata {
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub message_meta_tags: Vec<String>,
}

// --- Preview extraction ---

impl ConversationData {
    /// Extract preview text from conversation data.
    ///
    /// Returns first user prompt (truncated to 100 chars), "[Compacted session]",
    /// or "[No preview available]".
    pub fn preview(&self) -> String {
        if let Some(first_entry) = self.history.first()
            && let Some(user_msg) = &first_entry.user
            && let Some(UserContent::Prompt(prompt)) = &user_msg.content
        {
            return prompt.prompt.chars().take(100).collect();
        }

        if self.history.is_empty() && self.latest_summary.is_some() {
            return "[Compacted session]".to_string();
        }

        "[No preview available]".to_string()
    }
}
