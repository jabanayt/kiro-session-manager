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
    fn read_meta(&self, session_id: &str) -> Result<AcpMeta> {
        let path = self.json_path(session_id);
        let content = std::fs::read_to_string(&path)
            .map_err(|_| KsmError::SessionNotFound(session_id.to_string()))?;
        serde_json::from_str(&content).map_err(|e| {
            KsmError::Database(format!("Failed to parse ACP meta {}: {}", session_id, e))
        })
    }

    /// List all session IDs whose .json files exist in the sessions dir.
    fn all_ids(&self) -> Vec<String> {
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
}

// --- Serde types for .json metadata ---

#[derive(Deserialize)]
struct AcpMeta {
    session_id: String,
    cwd: String,
    created_at: String, // ISO 8601
    updated_at: String, // ISO 8601
    #[serde(default)]
    title: Option<String>,
}

// --- Serde types for .jsonl events ---

#[derive(Deserialize)]
struct AcpEvent {
    kind: String,
    data: serde_json::Value,
}

/// Parse ISO 8601 timestamp to milliseconds since epoch.
fn iso_to_ms(s: &str) -> i64 {
    // Use a simple manual parse to avoid adding a chrono dependency.
    // Format: "2026-03-22T05:59:41.761090399Z"
    // We parse up to seconds precision and ignore sub-seconds.
    let s = s.trim_end_matches('Z');
    // Split on T
    let (date_part, time_part) = match s.split_once('T') {
        Some(p) => p,
        None => return 0,
    };
    let date_parts: Vec<&str> = date_part.split('-').collect();
    let time_parts: Vec<&str> = time_part.split(':').collect();
    if date_parts.len() < 3 || time_parts.len() < 3 {
        return 0;
    }
    let year: i64 = date_parts[0].parse().unwrap_or(0);
    let month: i64 = date_parts[1].parse().unwrap_or(0);
    let day: i64 = date_parts[2].parse().unwrap_or(0);
    let hour: i64 = time_parts[0].parse().unwrap_or(0);
    let min: i64 = time_parts[1].parse().unwrap_or(0);
    let sec: i64 = time_parts[2]
        .split('.')
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);

    // Days since epoch using proleptic Gregorian calendar
    let days = days_since_epoch(year, month, day);
    let secs = days * 86400 + hour * 3600 + min * 60 + sec;
    secs * 1000
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
                created_at: iso_to_ms(&meta.created_at),
                updated_at: iso_to_ms(&meta.updated_at),
                preview,
                msg_count,
                source_type: SourceType::Acp,
            });
        }

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
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
            updates.push((meta.session_id, iso_to_ms(&meta.updated_at)));
        }

        Ok(updates)
    }

    fn get_conversation(&self, session_id: &str) -> Result<ConversationData> {
        let jsonl_path = self.jsonl_path(session_id);
        let content = std::fs::read_to_string(&jsonl_path)
            .map_err(|_| KsmError::SessionNotFound(session_id.to_string()))?;
        Ok(parse_jsonl(session_id, &content))
    }

    fn get_conversation_with_created_at(
        &self,
        session_id: &str,
    ) -> Result<(ConversationData, i64)> {
        let meta = self.read_meta(session_id)?;
        let created_at = iso_to_ms(&meta.created_at);
        let conv = self.get_conversation(session_id)?;
        Ok((conv, created_at))
    }

    fn get_message_ids(&self, session_id: &str) -> Result<Vec<String>> {
        let jsonl_path = self.jsonl_path(session_id);
        let content = std::fs::read_to_string(&jsonl_path)
            .map_err(|_| KsmError::SessionNotFound(session_id.to_string()))?;
        Ok(extract_message_ids_from_jsonl(&content))
    }

    fn has_compact_tag(&self, _session_id: &str) -> Result<bool> {
        Ok(false) // ACP sessions don't use the Compact tag mechanism
    }

    fn get_timestamps(&self, session_id: &str) -> Result<(i64, i64)> {
        let meta = self.read_meta(session_id)?;
        Ok((iso_to_ms(&meta.created_at), iso_to_ms(&meta.updated_at)))
    }

    fn update_timestamp(&self, session_id: &str, timestamp: i64) -> Result<()> {
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

    fn delete_session(&self, session_id: &str) -> Result<()> {
        let json_path = self.json_path(session_id);
        let jsonl_path = self.jsonl_path(session_id);
        // Remove both files; ignore errors if already gone
        let _ = std::fs::remove_file(&json_path);
        let _ = std::fs::remove_file(&jsonl_path);
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
