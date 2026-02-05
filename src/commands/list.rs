use anyhow::Result;
use std::collections::HashMap;

use crate::commands::detect::auto_link_continuations;
use crate::config::load_config;
use crate::kiro::get_sessions;
use crate::models::{Session, SessionMetadata};
use crate::storage::{cleanup_stale_metadata, load_metadata};

/// Filter out parent sessions (sessions that are referenced as parents)
pub fn filter_parent_sessions<'a>(
    sessions: &'a [Session],
    metadata: &HashMap<String, SessionMetadata>,
) -> Vec<&'a Session> {
    let parent_ids: std::collections::HashSet<_> = metadata
        .values()
        .filter_map(|m| m.parent_session_id.as_ref())
        .collect();
    
    sessions
        .iter()
        .filter(|s| !parent_ids.contains(&s.id))
        .collect()
}

pub fn format_session_display(
    session: &Session,
    metadata: &HashMap<String, SessionMetadata>,
    sessions: &[Session],
    include_original: bool,
    show_parent_inline: bool,
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
    
    // Add parent indicator if present and requested
    if show_parent_inline {
        if let Some(meta) = meta {
            if let Some(parent_id) = &meta.parent_session_id {
                // Find parent index
                if let Some(parent_idx) = sessions.iter().position(|s| &s.id == parent_id) {
                    display.push_str(&format!(" \x1b[36m↳ from [{}]\x1b[0m", parent_idx));
                }
            }
        }
    }
    
    display
}

/// Display filtered sessions with optional parent chain display
pub fn display_filtered_sessions(
    sessions: &[Session],
    metadata: &HashMap<String, SessionMetadata>,
    show_parents: bool,
) {
    let filtered_sessions = filter_parent_sessions(sessions, metadata);

    if filtered_sessions.is_empty() {
        println!("No sessions found.");
        return;
    }

    println!("\nKiro Chat Sessions:\n");
    for session in &filtered_sessions {
        // Find original index
        let idx = sessions.iter().position(|s| s.id == session.id).unwrap();
        
        if show_parents {
            // Show session with detailed parent chain
            let display = format_session_display(session, metadata, sessions, false, false);
            println!("[{}] {} | {} | {}", idx, session.time_ago, session.msg_count, display);
            
            // Show parent chain with details and indentation
            let mut current_id = session.id.clone();
            let mut depth = 1;
            while let Some(meta) = metadata.get(&current_id) {
                if let Some(parent_id) = &meta.parent_session_id {
                    if let Some(parent_idx) = sessions.iter().position(|s| &s.id == parent_id) {
                        let parent = &sessions[parent_idx];
                        let parent_display = format_session_display(parent, metadata, sessions, false, false);
                        let indent = "    ".repeat(depth);
                        println!("{}\x1b[36m↳ from [{}]\x1b[0m {} ({})", indent, parent_idx, parent_display, parent.time_ago);
                        current_id = parent_id.clone();
                        depth += 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        } else {
            // Default view: show inline parent indicator
            let display = format_session_display(session, metadata, sessions, false, true);
            println!("[{}] {} | {} | {}", idx, session.time_ago, session.msg_count, display);
        }
    }
}

pub fn list_sessions(show_parents: bool) -> Result<()> {
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

    // Auto-detect continuations if enabled
    let config = load_config()?;
    if config.auto_detect_continuations {
        let detected = auto_link_continuations(&sessions, &mut metadata)?;
        if detected > 0 {
            println!("✓ Auto-linked {} compacted session(s) to their parents\n", detected);
        }
    }

    display_filtered_sessions(&sessions, &metadata, show_parents);
    
    println!("\nUse 'ksm delete <indices>' to delete sessions (e.g., 'ksm delete 0,2,4')");

    Ok(())
}
