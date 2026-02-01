use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

use crate::models::Session;

/// Execute a SQLite query and return the output
fn query_db(sql: &str) -> Result<String> {
    let db_path = get_db_path()?;
    let output = Command::new("sqlite3")
        .arg(&db_path)
        .arg(sql)
        .output()
        .context("Failed to execute sqlite3")?;
    
    if !output.status.success() {
        anyhow::bail!("Failed to query database: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Check if a session has the Compact tag (indicating it was created from compaction)
pub fn has_compact_tag(session_id: &str) -> Result<bool> {
    let result = query_db(&format!(
        "SELECT json_extract(value, '$.latest_summary[1].message_meta_tags') FROM conversations_v2 WHERE conversation_id='{}';",
        session_id
    ))?;
    Ok(result.contains("Compact"))
}

/// Get session timestamps (created_at, updated_at) in milliseconds
pub fn get_session_timestamps(session_id: &str) -> Result<(i64, i64)> {
    let result = query_db(&format!(
        "SELECT created_at, updated_at FROM conversations_v2 WHERE conversation_id='{}';",
        session_id
    ))?;
    
    let parts: Vec<&str> = result.trim().split('|').collect();
    if parts.len() != 2 {
        anyhow::bail!("Unexpected database response format");
    }
    
    let created_at: i64 = parts[0].parse().context("Failed to parse created_at")?;
    let updated_at: i64 = parts[1].parse().context("Failed to parse updated_at")?;
    
    Ok((created_at, updated_at))
}

/// Extract message IDs from a session's history
pub fn get_message_ids(session_id: &str) -> Result<Vec<String>> {
    let json_str = query_db(&format!(
        "SELECT value FROM conversations_v2 WHERE conversation_id='{}';",
        session_id
    ))?;
    
    let session_data: serde_json::Value = serde_json::from_str(&json_str)
        .context("Failed to parse session JSON")?;
    
    let mut message_ids = Vec::new();
    if let Some(history) = session_data.get("history").and_then(|h| h.as_array()) {
        for msg in history {
            if let Some(msg_id) = msg
                .get("request_metadata")
                .and_then(|m| m.get("message_id"))
                .and_then(|id| id.as_str())
            {
                message_ids.push(msg_id.to_string());
            }
        }
    }
    
    Ok(message_ids)
}

/// Find potential parent sessions for a child session
/// Primary: message_id overlap (definitive proof)
/// Fallback: timestamp matching
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
            // Only consider sessions created before the child
            if created < child_created {
                candidates.push((session.id.clone(), created));
            }
        }
    }
    
    if !candidates.is_empty() {
        // Sort by most recently created (closest to child's creation time)
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

/// Get the path to Kiro's database
fn get_db_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    Ok(PathBuf::from(home).join(".local/share/kiro-cli/data.sqlite3"))
}
