use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;

use crate::models::{ConversationData, Session};

/// Get the path to Kiro's database
pub fn get_db_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    Ok(PathBuf::from(home).join(".local/share/kiro-cli/data.sqlite3"))
}

/// Open database connection in read-only mode
pub fn open_db_connection() -> Result<Connection> {
    let db_path = get_db_path()?;
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .context("Failed to open database")?;

    // Verify conversations_v2 table exists
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='conversations_v2'",
            [],
            |row| row.get(0),
        )
        .context("Failed to check for conversations_v2 table")?;

    if !table_exists {
        anyhow::bail!("conversations_v2 table not found. Please update kiro-cli.");
    }

    Ok(conn)
}

/// Open database connection with write permissions
pub fn open_db_connection_write() -> Result<Connection> {
    let db_path = get_db_path()?;
    Connection::open(&db_path).context("Failed to open database for writing")
}

/// Fetch all sessions from database for current directory
pub fn fetch_sessions_from_db() -> Result<Vec<Session>> {
    let conn = open_db_connection()?;
    let current_dir = std::env::current_dir()?.display().to_string();

    let mut stmt = conn.prepare(
        "SELECT conversation_id, value, created_at, updated_at 
         FROM conversations_v2 
         WHERE key = ? 
         ORDER BY updated_at DESC",
    )?;

    let sessions = stmt
        .query_map([&current_dir], |row| {
            let conversation_id: String = row.get(0)?;
            let value_json: String = row.get(1)?;
            let _created_at: i64 = row.get(2)?;
            let updated_at: i64 = row.get(3)?;

            Ok((conversation_id, value_json, updated_at))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut result = Vec::new();
    for (id, json, updated_at) in sessions {
        match serde_json::from_str::<ConversationData>(&json) {
            Ok(data) => {
                result.push(conversation_to_session(id, data, updated_at)?);
            }
            Err(e) => {
                anyhow::bail!(
                    "Failed to parse conversation JSON for session {}: {}",
                    id,
                    e
                );
            }
        }
    }

    Ok(result)
}

/// Convert database conversation data to Session struct
fn conversation_to_session(id: String, data: ConversationData, updated_at: i64) -> Result<Session> {
    let time_ago = calculate_time_ago(updated_at);
    let preview = extract_preview(&data);
    let msg_count = format_msg_count(&data);

    Ok(Session {
        id,
        time_ago,
        preview,
        msg_count,
    })
}

/// Calculate relative time string from timestamp
fn calculate_time_ago(timestamp_ms: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let diff_ms = now - timestamp_ms;
    let diff_secs = diff_ms / 1000;

    if diff_secs < 60 {
        return format!("{} seconds ago", diff_secs);
    }

    let diff_mins = diff_secs / 60;
    if diff_mins < 60 {
        return if diff_mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", diff_mins)
        };
    }

    let diff_hours = diff_mins / 60;
    if diff_hours < 24 {
        return if diff_hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", diff_hours)
        };
    }

    let diff_days = diff_hours / 24;
    if diff_days == 1 {
        "1 day ago".to_string()
    } else {
        format!("{} days ago", diff_days)
    }
}

/// Extract preview text from conversation data
fn extract_preview(data: &ConversationData) -> String {
    if let Some(first_entry) = data.history.first() {
        if let Some(user_msg) = &first_entry.user {
            if let Some(content) = &user_msg.content {
                if let Some(prompt_obj) = content.get("Prompt") {
                    if let Some(prompt_text) = prompt_obj.get("prompt") {
                        if let Some(text) = prompt_text.as_str() {
                            return text.chars().take(100).collect();
                        }
                    }
                }
            }
        }
    }

    if data.history.is_empty() && data.latest_summary.is_some() {
        return "[Compacted session]".to_string();
    }

    "[No preview available]".to_string()
}

/// Format message count as "X msgs"
fn format_msg_count(data: &ConversationData) -> String {
    let count = data.history.len();
    if count == 1 {
        "1 msg".to_string()
    } else {
        format!("{} msgs", count)
    }
}

/// Check if a session has the Compact tag
pub fn has_compact_tag(session_id: &str) -> Result<bool> {
    let conn = open_db_connection()?;
    let value_json: String = conn.query_row(
        "SELECT value FROM conversations_v2 WHERE conversation_id = ?",
        [session_id],
        |row| row.get(0),
    )?;

    let data: ConversationData = serde_json::from_str(&value_json)?;

    if let Some(summary) = data.latest_summary {
        if summary.len() > 1 {
            if let Some(tags) = summary[1].get("message_meta_tags") {
                if let Some(tags_arr) = tags.as_array() {
                    return Ok(tags_arr.iter().any(|t| t.as_str() == Some("Compact")));
                }
            }
        }
    }

    Ok(false)
}

/// Get session timestamps (created_at, updated_at) in milliseconds
pub fn get_session_timestamps(session_id: &str) -> Result<(i64, i64)> {
    let conn = open_db_connection()?;
    conn.query_row(
        "SELECT created_at, updated_at FROM conversations_v2 WHERE conversation_id = ?",
        [session_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .context("Failed to get session timestamps")
}

/// Extract message IDs from a session's history
pub fn get_message_ids(session_id: &str) -> Result<Vec<String>> {
    let conn = open_db_connection()?;
    let value_json: String = conn.query_row(
        "SELECT value FROM conversations_v2 WHERE conversation_id = ?",
        [session_id],
        |row| row.get(0),
    )?;

    let data: ConversationData = serde_json::from_str(&value_json)?;

    let mut message_ids = Vec::new();
    for entry in &data.history {
        if let Some(metadata) = &entry.request_metadata {
            if let Some(msg_id) = &metadata.message_id {
                message_ids.push(msg_id.clone());
            }
        }
    }

    Ok(message_ids)
}

/// Find potential parent sessions for a child session
pub fn find_potential_parents(child_id: &str, sessions: &[Session]) -> Result<Vec<String>> {
    let child_msg_ids = get_message_ids(child_id)?;
    let (child_created, _) = get_session_timestamps(child_id)?;
    let mut candidates = Vec::new();

    // Primary: message_id overlap
    for session in sessions {
        if session.id == child_id {
            continue;
        }

        let parent_msg_ids = get_message_ids(&session.id)?;
        if child_msg_ids.iter().any(|id| parent_msg_ids.contains(id)) {
            let (created, _) = get_session_timestamps(&session.id)?;
            if created < child_created {
                candidates.push((session.id.clone(), created));
            }
        }
    }

    if !candidates.is_empty() {
        candidates.sort_by_key(|(_, created)| -created);
        return Ok(candidates.into_iter().map(|(id, _)| id).collect());
    }

    // Fallback: timestamp matching
    for session in sessions {
        if session.id == child_id || has_compact_tag(&session.id)? {
            continue;
        }

        let (_, parent_updated) = get_session_timestamps(&session.id)?;
        let time_diff = child_created - parent_updated;
        if time_diff > 0 && time_diff <= 5 * 60 * 1000 {
            candidates.push((session.id.clone(), parent_updated));
        }
    }

    candidates.sort_by_key(|(_, updated)| -updated);
    Ok(candidates.into_iter().map(|(id, _)| id).collect())
}
