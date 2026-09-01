//! Session source backed by ACP/TUI file pairs at ~/.kiro/sessions/cli/.
//!
//! Each session is stored as:
//!   <session-id>.json  - metadata (session_id, cwd, created_at, updated_at, title)
//!   <session-id>.jsonl - event log (Prompt, AssistantMessage, ToolResults lines)

use std::path::PathBuf;

use serde::Deserialize;

use crate::data::SessionSource;
use crate::error::{KsmError, Result};
use crate::models::{
    AssistantContent, ConversationData, HistoryEntry, PromptContent, ResponseContent, Session,
    SourceType, UserContent, UserMessage,
};

pub struct AcpSource {
    sessions_dir: PathBuf,
}

impl Default for AcpSource {
    fn default() -> Self {
        Self::new()
    }
}

impl AcpSource {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        AcpSource {
            sessions_dir: PathBuf::from(home).join(".kiro/sessions/cli"),
        }
    }

    fn json_path(&self, session_id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{}.json", session_id))
    }

    fn jsonl_path(&self, session_id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{}.jsonl", session_id))
    }

    /// Read and parse the .json metadata file for a session.
    pub(crate) fn read_meta(&self, session_id: &str) -> Result<AcpMeta> {
        let path = self.json_path(session_id);
        let content = std::fs::read_to_string(&path)
            .map_err(|_| KsmError::SessionNotFound(session_id.to_string()))?;
        serde_json::from_str(&content).map_err(|e| {
            KsmError::Database(format!("Failed to parse ACP meta {}: {}", session_id, e))
        })
    }

    /// List all session IDs whose .json files exist in the sessions dir.
    pub(crate) fn all_ids(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.sessions_dir) else {
            return Vec::new();
        };
        entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                name.strip_suffix(".json").map(|s| s.to_string())
            })
            .collect()
    }

    /// Get the sessions directory mtime for cache validation.
    pub fn dir_mtime(&self) -> Result<std::time::SystemTime> {
        Ok(std::fs::metadata(&self.sessions_dir)?.modified()?)
    }

    /// Check if a session exists in ACP storage.
    pub fn has_session(&self, session_id: &str) -> bool {
        self.json_path(session_id).exists()
    }
}

// --- Serde types for .json metadata ---

#[derive(Deserialize)]
pub(crate) struct AcpMeta {
    pub(crate) session_id: String,
    pub(crate) cwd: String,
    pub(crate) created_at: String, // ISO 8601
    pub(crate) updated_at: String, // ISO 8601
    #[serde(default)]
    pub(crate) title: Option<String>,
}

// --- Serde types for .jsonl events ---

#[derive(Deserialize)]
struct AcpEvent {
    kind: String,
    data: serde_json::Value,
}

/// Parse ISO 8601 timestamp to milliseconds since epoch.
///
/// Handles timezone suffixes:
/// - `Z` (UTC)
/// - `+HH:MM` or `-HH:MM` (timezone offset, stripped and treated as UTC)
///
/// Returns error for invalid formats instead of silently returning 0.
fn iso_to_ms(s: &str) -> Result<i64> {
    let s = s.trim();

    // Handle timezone suffix (Z or +HH:MM or -HH:MM)
    let datetime = if let Some(stripped) = s.strip_suffix('Z') {
        stripped
    } else if s.len() > 6
        && (s.as_bytes()[s.len() - 6] == b'+' || s.as_bytes()[s.len() - 6] == b'-')
    {
        // Has offset like +12:00 or -05:00, strip it (treat as UTC for simplicity)
        &s[..s.len() - 6]
    } else {
        return Err(KsmError::Parse(format!("invalid timestamp format: {}", s)));
    };

    let (date_part, time_part) = datetime
        .split_once('T')
        .ok_or_else(|| KsmError::Parse(format!("missing T separator: {}", s)))?;

    let date_parts: Vec<&str> = date_part.split('-').collect();
    let time_parts: Vec<&str> = time_part.split(':').collect();

    if date_parts.len() < 3 || time_parts.len() < 3 {
        return Err(KsmError::Parse(format!("incomplete timestamp: {}", s)));
    }

    let year: i64 = date_parts[0]
        .parse()
        .map_err(|_| KsmError::Parse(format!("invalid year: {}", s)))?;
    let month: i64 = date_parts[1]
        .parse()
        .map_err(|_| KsmError::Parse(format!("invalid month: {}", s)))?;
    let day: i64 = date_parts[2]
        .parse()
        .map_err(|_| KsmError::Parse(format!("invalid day: {}", s)))?;
    let hour: i64 = time_parts[0]
        .parse()
        .map_err(|_| KsmError::Parse(format!("invalid hour: {}", s)))?;
    let min: i64 = time_parts[1]
        .parse()
        .map_err(|_| KsmError::Parse(format!("invalid minute: {}", s)))?;
    let sec: i64 = time_parts[2]
        .split('.')
        .next()
        .unwrap_or("0")
        .parse()
        .map_err(|_| KsmError::Parse(format!("invalid second: {}", s)))?;

    // Days since epoch using proleptic Gregorian calendar
    let days = days_since_epoch(year, month, day);
    let secs = days * 86400 + hour * 3600 + min * 60 + sec;
    Ok(secs * 1000)
}

fn days_since_epoch(year: i64, month: i64, day: i64) -> i64 {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
    let y = if month <= 2 { year - 1 } else { year };
    let m = month;
    let d = day;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Parse the .jsonl event log into ConversationData.
fn parse_jsonl(session_id: &str, content: &str) -> ConversationData {
    let mut history = Vec::new();

    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        let Ok(event) = serde_json::from_str::<AcpEvent>(line) else {
            continue;
        };
        if event.kind != "Prompt" {
            continue;
        }
        // Extract user text from Prompt event
        let user_text = extract_text_content(&event.data);

        // Collect the next AssistantMessage event (may be separated by ToolResults)
        let mut assistant_text = String::new();
        let mut message_id: Option<String> = None;

        // Peek ahead through remaining lines for the next AssistantMessage
        let remaining: Vec<&str> = lines.clone().collect();
        for next_line in &remaining {
            let Ok(next_event) = serde_json::from_str::<AcpEvent>(next_line) else {
                continue;
            };
            if next_event.kind == "AssistantMessage" {
                assistant_text = extract_text_content(&next_event.data);
                message_id = next_event
                    .data
                    .get("message_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                break;
            }
            if next_event.kind == "Prompt" {
                break; // Next user turn, stop
            }
        }

        let user_msg = UserMessage {
            content: Some(UserContent::Prompt(PromptContent { prompt: user_text })),
            timestamp: None,
        };
        let assistant = Some(AssistantContent::Response(ResponseContent {
            message_id,
            content: assistant_text,
        }));

        history.push(HistoryEntry {
            user: Some(user_msg),
            assistant,
            request_metadata: None,
        });
    }

    ConversationData {
        conversation_id: session_id.to_string(),
        history,
        latest_summary: None,
    }
}

/// Extract concatenated text from ACP event data content array.
fn extract_text_content(data: &serde_json::Value) -> String {
    data.get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|item| item.get("kind").and_then(|k| k.as_str()) == Some("text"))
                .filter_map(|item| item.get("data").and_then(|d| d.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

/// Extract preview from .jsonl: title if present, else first Prompt text.
fn extract_preview(title: Option<&str>, jsonl_content: &str) -> String {
    if let Some(t) = title
        && !t.is_empty()
    {
        return t.to_string();
    }
    // Fall back to first Prompt text
    for line in jsonl_content.lines() {
        let Ok(event) = serde_json::from_str::<AcpEvent>(line) else {
            continue;
        };
        if event.kind == "Prompt" {
            let text = extract_text_content(&event.data);
            if !text.is_empty() {
                let sanitised: String = text
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(100)
                    .collect();
                return sanitised;
            }
        }
    }
    "[No preview available]".to_string()
}

/// Count Prompt events in .jsonl (= number of user turns = message pairs).
fn count_messages(jsonl_content: &str) -> u32 {
    jsonl_content
        .lines()
        .filter(|line| {
            serde_json::from_str::<AcpEvent>(line)
                .map(|e| e.kind == "Prompt")
                .unwrap_or(false)
        })
        .count() as u32
}

/// Extract message IDs from AssistantMessage events in .jsonl.
fn extract_message_ids_from_jsonl(jsonl_content: &str) -> Vec<String> {
    jsonl_content
        .lines()
        .filter_map(|line| {
            let event = serde_json::from_str::<AcpEvent>(line).ok()?;
            if event.kind != "AssistantMessage" {
                return None;
            }
            event
                .data
                .get("message_id")?
                .as_str()
                .map(|s| s.to_string())
        })
        .collect()
}

impl SessionSource for AcpSource {
    fn list_sessions(&self) -> Result<Vec<Session>> {
        let current_dir = std::env::current_dir()?.display().to_string();
        let mut sessions = Vec::new();

        for id in self.all_ids() {
            let Ok(meta) = self.read_meta(&id) else {
                continue;
            };
            if meta.cwd != current_dir {
                continue;
            }
            let jsonl_path = self.jsonl_path(&id);
            let jsonl = std::fs::read_to_string(&jsonl_path).unwrap_or_default();
            let preview = extract_preview(meta.title.as_deref(), &jsonl);
            let msg_count = count_messages(&jsonl);

            sessions.push(Session {
                id: meta.session_id,
                created_at: iso_to_ms(&meta.created_at)?,
                updated_at: iso_to_ms(&meta.updated_at)?,
                preview,
                msg_count,
                source_type: SourceType::Acp,
            });
        }

        sessions.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        Ok(sessions)
    }

    fn list_session_updates(&self) -> Result<Vec<(String, i64)>> {
        let current_dir = std::env::current_dir()?.display().to_string();
        let mut updates = Vec::new();

        for id in self.all_ids() {
            let Ok(meta) = self.read_meta(&id) else {
                continue;
            };
            if meta.cwd != current_dir {
                continue;
            }
            updates.push((meta.session_id, iso_to_ms(&meta.updated_at)?));
        }

        Ok(updates)
    }

    fn get_conversation(
        &self,
        session_id: &str,
        _source_type: SourceType,
    ) -> Result<ConversationData> {
        let jsonl_path = self.jsonl_path(session_id);
        let content = std::fs::read_to_string(&jsonl_path)
            .map_err(|_| KsmError::SessionNotFound(session_id.to_string()))?;
        Ok(parse_jsonl(session_id, &content))
    }

    fn get_conversation_with_created_at(
        &self,
        session_id: &str,
        source_type: SourceType,
    ) -> Result<(ConversationData, i64)> {
        let meta = self.read_meta(session_id)?;
        let created_at = iso_to_ms(&meta.created_at)?;
        let conv = self.get_conversation(session_id, source_type)?;
        Ok((conv, created_at))
    }

    fn get_message_ids(&self, session_id: &str, _source_type: SourceType) -> Result<Vec<String>> {
        let jsonl_path = self.jsonl_path(session_id);
        let content = std::fs::read_to_string(&jsonl_path)
            .map_err(|_| KsmError::SessionNotFound(session_id.to_string()))?;
        Ok(extract_message_ids_from_jsonl(&content))
    }

    fn has_compact_tag(&self, _session_id: &str, _source_type: SourceType) -> Result<bool> {
        Ok(false) // ACP sessions don't use the Compact tag mechanism
    }

    fn get_timestamps(&self, session_id: &str, _source_type: SourceType) -> Result<(i64, i64)> {
        let meta = self.read_meta(session_id)?;
        Ok((iso_to_ms(&meta.created_at)?, iso_to_ms(&meta.updated_at)?))
    }

    fn update_timestamp(
        &self,
        session_id: &str,
        timestamp: i64,
        _source_type: SourceType,
    ) -> Result<()> {
        let path = self.json_path(session_id);
        let content = std::fs::read_to_string(&path)
            .map_err(|_| KsmError::SessionNotFound(session_id.to_string()))?;
        let mut value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| KsmError::Database(format!("Failed to parse ACP meta: {}", e)))?;

        // Convert ms back to ISO 8601
        let secs = timestamp / 1000;
        let ms = timestamp % 1000;
        let iso = ms_to_iso(secs, ms);
        value["updated_at"] = serde_json::Value::String(iso);

        let updated = serde_json::to_string_pretty(&value)
            .map_err(|e| KsmError::Database(format!("Failed to serialize ACP meta: {}", e)))?;
        std::fs::write(&path, updated)?;
        Ok(())
    }

    /// Delete a session from ACP storage via kiro-cli.
    ///
    /// Uses `kiro-cli chat --delete-session <id> --session-source v2` to ensure
    /// proper cleanup consistent with kiro-cli's expectations.
    fn delete_session(&self, session_id: &str, _source_type: SourceType) -> Result<()> {
        let output = std::process::Command::new("kiro-cli")
            .args([
                "chat",
                "--delete-session",
                session_id,
                "--session-source",
                "v2",
            ])
            .output()?;

        if !output.status.success() {
            return Err(KsmError::KiroCli(format!(
                "Failed to delete session {}: {}",
                session_id,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    fn source_type(&self) -> SourceType {
        SourceType::Acp
    }
}

/// Convert Unix seconds + ms to ISO 8601 string (UTC).
fn ms_to_iso(secs: i64, ms: i64) -> String {
    // Reverse of days_since_epoch
    let (year, month, day) = epoch_secs_to_ymd(secs);
    let time_of_day = secs.rem_euclid(86400);
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}000000Z",
        year, month, day, h, m, s, ms
    )
}

fn epoch_secs_to_ymd(secs: i64) -> (i64, i64, i64) {
    let days = secs.div_euclid(86400);
    // Civil date from days since epoch (same algorithm, reversed)
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn test_iso_to_ms_utc() {
        // 2026-01-15T10:30:00Z
        let result = iso_to_ms("2026-01-15T10:30:00Z").unwrap();
        // Verify it's a reasonable timestamp (after 2020, before 2030)
        assert!(result > 1577836800000); // 2020-01-01
        assert!(result < 1893456000000); // 2030-01-01
    }

    #[test]
    fn test_iso_to_ms_with_offset() {
        // Should parse without error (offset is stripped)
        let result = iso_to_ms("2026-01-15T10:30:00+12:00");
        assert!(result.is_ok());
    }

    #[test]
    fn test_iso_to_ms_with_fractional_seconds() {
        let result = iso_to_ms("2026-01-15T10:30:00.123Z");
        assert!(result.is_ok());
    }

    #[test]
    fn test_iso_to_ms_invalid_format() {
        assert!(iso_to_ms("not a date").is_err());
        assert!(iso_to_ms("2026-01-15").is_err()); // Missing time
        assert!(iso_to_ms("10:30:00Z").is_err()); // Missing date
    }

    #[test]
    fn test_days_since_epoch() {
        // 1970-01-01 should be day 0
        assert_eq!(days_since_epoch(1970, 1, 1), 0);
        // 1970-01-02 should be day 1
        assert_eq!(days_since_epoch(1970, 1, 2), 1);
    }

    #[test]
    fn test_extract_text_content_simple() {
        let data = serde_json::json!({
            "content": [
                {"kind": "text", "data": "Hello "},
                {"kind": "text", "data": "World"}
            ]
        });
        assert_eq!(extract_text_content(&data), "Hello World");
    }

    #[test]
    fn test_extract_text_content_filters_non_text() {
        let data = serde_json::json!({
            "content": [
                {"kind": "text", "data": "Hello"},
                {"kind": "image", "data": "base64..."},
                {"kind": "text", "data": " World"}
            ]
        });
        assert_eq!(extract_text_content(&data), "Hello World");
    }

    #[test]
    fn test_extract_text_content_empty() {
        let data = serde_json::json!({});
        assert_eq!(extract_text_content(&data), "");

        let data2 = serde_json::json!({"content": []});
        assert_eq!(extract_text_content(&data2), "");
    }

    #[test]
    fn test_extract_preview_uses_title() {
        let preview = extract_preview(Some("My Title"), "");
        assert_eq!(preview, "My Title");
    }

    #[test]
    fn test_extract_preview_empty_title_falls_back() {
        let jsonl =
            r#"{"kind":"Prompt","data":{"content":[{"kind":"text","data":"First message"}]}}"#;
        let preview = extract_preview(Some(""), jsonl);
        assert_eq!(preview, "First message");
    }

    #[test]
    fn test_extract_preview_no_content() {
        let preview = extract_preview(None, "");
        assert_eq!(preview, "[No preview available]");
    }

    #[test]
    fn test_extract_message_ids_from_jsonl() {
        let jsonl = r#"{"kind":"Prompt","data":{}}
{"kind":"AssistantMessage","data":{"message_id":"msg-123"}}
{"kind":"Prompt","data":{}}
{"kind":"AssistantMessage","data":{"message_id":"msg-456"}}"#;

        let ids = extract_message_ids_from_jsonl(jsonl);
        assert_eq!(ids, vec!["msg-123", "msg-456"]);
    }

    #[test]
    fn test_extract_message_ids_skips_non_assistant() {
        let jsonl = r#"{"kind":"Prompt","data":{"message_id":"should-skip"}}
{"kind":"AssistantMessage","data":{"message_id":"msg-123"}}"#;

        let ids = extract_message_ids_from_jsonl(jsonl);
        assert_eq!(ids, vec!["msg-123"]);
    }

    #[test]
    fn test_parse_jsonl_single_exchange() {
        let jsonl = r#"{"kind":"Prompt","data":{"content":[{"kind":"text","data":"Hello"}]}}
{"kind":"AssistantMessage","data":{"content":[{"kind":"text","data":"Hi there"}],"message_id":"msg-1"}}"#;

        let conv = parse_jsonl("test-session", jsonl);
        assert_eq!(conv.conversation_id, "test-session");
        assert_eq!(conv.history.len(), 1);
    }

    #[test]
    fn test_parse_jsonl_empty() {
        let conv = parse_jsonl("test-session", "");
        assert_eq!(conv.conversation_id, "test-session");
        assert!(conv.history.is_empty());
    }
}
