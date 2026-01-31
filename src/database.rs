use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

use crate::models::Session;

/// Check if a session has the Compact tag (indicating it was created from compaction)
pub fn has_compact_tag(session_id: &str) -> Result<bool> {
    let db_path = get_db_path()?;
    
    let output = Command::new("sqlite3")
        .arg(&db_path)
        .arg(format!(
            "SELECT json_extract(value, '$.latest_summary[1].message_meta_tags') FROM conversations_v2 WHERE conversation_id='{}';",
            session_id
        ))
        .output()
        .context("Failed to execute sqlite3")?;
    
    if !output.status.success() {
        anyhow::bail!("Failed to query database: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    let result = String::from_utf8_lossy(&output.stdout);
    Ok(result.contains("Compact"))
}

/// Get session timestamps (created_at, updated_at) in milliseconds
pub fn get_session_timestamps(session_id: &str) -> Result<(i64, i64)> {
    let db_path = get_db_path()?;
    
    let output = Command::new("sqlite3")
        .arg(&db_path)
        .arg(format!(
            "SELECT created_at, updated_at FROM conversations_v2 WHERE conversation_id='{}';",
            session_id
        ))
        .output()
        .context("Failed to execute sqlite3")?;
    
    if !output.status.success() {
        anyhow::bail!("Failed to query database: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    let result = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = result.trim().split('|').collect();
    
    if parts.len() != 2 {
        anyhow::bail!("Unexpected database response format");
    }
    
    let created_at: i64 = parts[0].parse().context("Failed to parse created_at")?;
    let updated_at: i64 = parts[1].parse().context("Failed to parse updated_at")?;
    
    Ok((created_at, updated_at))
}

/// Find potential parent sessions for a child session
/// Returns list of parent session IDs that match timing criteria
pub fn find_potential_parents(child_id: &str, sessions: &[Session]) -> Result<Vec<String>> {
    let (child_created, _) = get_session_timestamps(child_id)?;
    
    let mut candidates = Vec::new();
    
    for session in sessions {
        // Skip self
        if session.id == child_id {
            continue;
        }
        
        // Check if this session has Compact tag (don't link child to another child)
        if has_compact_tag(&session.id)? {
            continue;
        }
        
        // Get parent's timestamps
        let (_, parent_updated) = get_session_timestamps(&session.id)?;
        
        // Check if parent was updated within 5 minutes before child was created
        let time_diff = child_created - parent_updated;
        let five_minutes_ms = 5 * 60 * 1000;
        
        if time_diff > 0 && time_diff <= five_minutes_ms {
            candidates.push(session.id.clone());
        }
    }
    
    // Sort by closest time match (smallest time difference first)
    candidates.sort_by_key(|parent_id| {
        let (_, parent_updated) = get_session_timestamps(parent_id).unwrap_or((0, 0));
        (child_created - parent_updated).abs()
    });
    
    Ok(candidates)
}

/// Get the path to Kiro's database
fn get_db_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    Ok(PathBuf::from(home).join(".local/share/kiro-cli/data.sqlite3"))
}
