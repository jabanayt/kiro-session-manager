use anyhow::Result;
use std::collections::HashMap;

use crate::kiro::get_sessions;
use crate::models::{Session, SessionMetadata};
use crate::storage::{cleanup_stale_metadata, load_metadata};

pub fn format_session_display(
    session: &Session,
    metadata: &HashMap<String, SessionMetadata>,
    include_original: bool,
) -> String {
    let meta = metadata.get(&session.id);
    
    let mut display = String::new();
    
    // Add tags if present
    if let Some(meta) = meta {
        if !meta.tags.is_empty() {
            let mut tags: Vec<_> = meta.tags.iter().collect();
            tags.sort();
            for tag in tags {
                display.push_str(&format!("[{}] ", tag));
            }
        }
    }
    
    // Add name or preview
    if let Some(meta) = meta {
        if let Some(name) = &meta.name {
            display.push_str(name);
            // If we have a custom name and include_original is true, show original too
            if include_original {
                display.push_str(&format!(" ({})", session.preview));
            }
        } else {
            display.push_str(&session.preview);
        }
    } else {
        display.push_str(&session.preview);
    }
    
    display
}

pub fn display_sessions_with_metadata(
    sessions: &[Session],
    metadata: &HashMap<String, SessionMetadata>,
) {
    println!("\nKiro Chat Sessions:\n");
    for (idx, session) in sessions.iter().enumerate() {
        let display = format_session_display(session, metadata, false);
        println!("[{}] {} | {} | {}", idx, session.time_ago, session.msg_count, display);
    }
}

pub fn list_sessions() -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;
    
    // Only cleanup if we have sessions (directory-aware, but avoid data loss on kiro-cli failure)
    if !sessions.is_empty() {
        cleanup_stale_metadata(&mut metadata, &sessions)?;
    }

    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    display_sessions_with_metadata(&sessions, &metadata);
    println!("\nUse 'ksm delete <indices>' to delete sessions (e.g., 'ksm delete 0,2,4')");

    Ok(())
}
