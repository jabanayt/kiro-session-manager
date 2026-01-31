use anyhow::Result;
use std::io::Write;

use crate::kiro::get_sessions;
use crate::storage::{load_metadata, save_metadata};

pub fn unlink_session(index: usize, keep_metadata: bool) -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;
    
    if index >= sessions.len() {
        anyhow::bail!("Index {} out of range (max: {})", index, sessions.len() - 1);
    }
    
    let session = &sessions[index];
    
    // Check if session has a parent
    let session_meta = metadata.get(&session.id);
    if session_meta.is_none() || session_meta.unwrap().parent_session_id.is_none() {
        anyhow::bail!("Session [{}] is not linked to a parent", index);
    }
    
    let mut session_metadata = session_meta.unwrap().clone();
    let parent_id = session_metadata.parent_session_id.take().unwrap();
    
    // Mark as manually unlinked to prevent auto-detection from re-linking
    session_metadata.manually_unlinked = true;
    
    if !keep_metadata {
        // Ask if user wants to keep inherited metadata
        print!("Keep inherited name and tags? (y/n): ");
        std::io::stdout().flush()?;
        
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        
        if !input.trim().eq_ignore_ascii_case("y") {
            // Remove inherited metadata
            session_metadata.name = None;
            session_metadata.tags.clear();
        }
    }
    
    metadata.insert(session.id.clone(), session_metadata);
    save_metadata(&metadata)?;
    
    // Find parent index for display
    let parent_idx = sessions.iter().position(|s| s.id == parent_id);
    
    if let Some(parent_idx) = parent_idx {
        println!("✓ Unlinked session [{}] from parent [{}]", index, parent_idx);
    } else {
        println!("✓ Unlinked session [{}]", index);
    }
    
    Ok(())
}
