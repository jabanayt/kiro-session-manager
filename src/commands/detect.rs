use anyhow::Result;
use std::io::Write;

use crate::commands::list::format_session_display;
use crate::database::{find_potential_parents, has_compact_tag};
use crate::kiro::get_sessions;
use crate::models::{Session, SessionMetadata};
use crate::storage::{load_metadata, save_metadata};
use std::collections::HashMap;

/// Detect unlinked compacted sessions and return list of (child_session, parent_id) pairs
pub fn detect_unlinked_continuations(
    sessions: &[Session],
    metadata: &HashMap<String, SessionMetadata>,
    force: bool,
) -> Result<Vec<(Session, String)>> {
    let mut candidates = Vec::new();
    
    for session in sessions {
        // Skip if already linked
        if let Some(meta) = metadata.get(&session.id) {
            if meta.parent_session_id.is_some() {
                continue;
            }
            // Skip if manually unlinked (unless force flag)
            if !force && meta.manually_unlinked {
                continue;
            }
        }
        
        // Check if this session has Compact tag
        if !has_compact_tag(&session.id)? {
            continue;
        }
        
        // Find potential parents
        let parent_candidates = find_potential_parents(&session.id, sessions)?;
        
        if parent_candidates.is_empty() {
            continue;
        }
        
        // Get the best match (first candidate, already sorted by closest time)
        let parent_id = &parent_candidates[0];
        
        // Check if parent already has a child (enforce linear chains)
        if metadata.values().any(|m| m.parent_session_id.as_ref() == Some(parent_id)) {
            continue;
        }
        
        candidates.push((session.clone(), parent_id.clone()));
    }
    
    Ok(candidates)
}

/// Auto-link detected continuations (used by list command)
pub fn auto_link_continuations(
    sessions: &[Session],
    metadata: &mut HashMap<String, SessionMetadata>,
) -> Result<usize> {
    let candidates = detect_unlinked_continuations(sessions, metadata, false)?;
    
    if candidates.is_empty() {
        return Ok(0);
    }
    
    for (session, parent_id) in &candidates {
        let mut child_metadata = metadata.get(&session.id).cloned().unwrap_or_default();
        child_metadata.parent_session_id = Some(parent_id.clone());
        
        // Inherit name and tags from parent
        if let Some(parent_meta) = metadata.get(parent_id) {
            child_metadata.name = parent_meta.name.clone();
            child_metadata.tags = parent_meta.tags.clone();
        }
        
        metadata.insert(session.id.clone(), child_metadata);
    }
    
    save_metadata(metadata)?;
    Ok(candidates.len())
}

pub fn detect_continuations(force: bool) -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;
    
    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }
    
    println!("Scanning for compacted sessions...\n");
    
    let candidates = detect_unlinked_continuations(&sessions, &metadata, force)?;
    
    if candidates.is_empty() {
        println!("No unlinked compacted sessions found.");
        return Ok(());
    }
    
    for (session, parent_id) in candidates {
        let child_idx = sessions.iter().position(|s| s.id == session.id).unwrap();
        let parent_idx = sessions.iter().position(|s| &s.id == &parent_id).unwrap();
        let parent = &sessions[parent_idx];
        
        let child_display = format_session_display(&session, &metadata, &sessions, false, false);
        let parent_display = format_session_display(parent, &metadata, &sessions, false, false);
        
        println!("[{}] {} ({})", child_idx, child_display, session.time_ago);
        println!("    might continue from");
        println!("[{}] {} ({})", parent_idx, parent_display, parent.time_ago);
        
        print!("\nLink them? (y/n): ");
        std::io::stdout().flush()?;
        
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        
        if input.trim().eq_ignore_ascii_case("y") {
            // Perform linking
            let mut child_metadata = metadata.get(&session.id).cloned().unwrap_or_default();
            child_metadata.parent_session_id = Some(parent_id.clone());
            
            // Inherit name and tags from parent
            if let Some(parent_meta) = metadata.get(&parent_id) {
                child_metadata.name = parent_meta.name.clone();
                child_metadata.tags = parent_meta.tags.clone();
            }
            
            metadata.insert(session.id.clone(), child_metadata);
            save_metadata(&metadata)?;
            
            println!("✓ Linked [{}] to [{}]\n", child_idx, parent_idx);
        } else {
            println!("Skipped.\n");
        }
    }
    
    Ok(())
}
