use anyhow::Result;
use std::io::Write;

use crate::kiro::get_sessions;
use crate::storage::{load_metadata, save_metadata};

pub fn link_sessions(child_index: usize, parent_index: usize) -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;
    
    // Validate indices
    if child_index >= sessions.len() {
        anyhow::bail!("Child index {} out of range (max: {})", child_index, sessions.len() - 1);
    }
    if parent_index >= sessions.len() {
        anyhow::bail!("Parent index {} out of range (max: {})", parent_index, sessions.len() - 1);
    }
    if child_index == parent_index {
        anyhow::bail!("Cannot link a session to itself");
    }
    
    let child = &sessions[child_index];
    let parent = &sessions[parent_index];
    
    // Check if child is already someone else's parent (enforce linear chains)
    if metadata.values().any(|m| m.parent_session_id.as_ref() == Some(&child.id)) {
        anyhow::bail!("Session [{}] is already a parent. Cannot link it as a child.\nCompaction creates linear chains: each session can only be a child of one parent.", child_index);
    }
    
    // Check if parent already has another child (enforce linear chains)
    if metadata.values().any(|m| m.parent_session_id.as_ref() == Some(&parent.id)) {
        anyhow::bail!("Session [{}] already has a child. Cannot link another child to it.\nCompaction creates linear chains: each session can only have one child.", parent_index);
    }
    
    // Check if child already has a parent (prevent re-linking)
    if let Some(existing) = metadata.get(&child.id) {
        if existing.parent_session_id.is_some() {
            anyhow::bail!("Session [{}] is already linked to a parent. Use 'ksm unlink {}' first if you want to change the parent.", child_index, child_index);
        }
    }
    
    // Check if child already has metadata
    let child_meta = metadata.get(&child.id);
    let parent_meta = metadata.get(&parent.id).cloned();
    
    if let Some(existing) = child_meta {
        if existing.name.is_some() || !existing.tags.is_empty() {
            // Check if metadata matches parent (skip warning if identical)
            let metadata_matches = if let Some(ref parent_meta) = parent_meta {
                existing.name == parent_meta.name && existing.tags == parent_meta.tags
            } else {
                false
            };
            
            if !metadata_matches {
                // Child has different metadata - warn user
                println!("⚠ Warning: Session [{}] already has metadata:", child_index);
            if let Some(name) = &existing.name {
                println!("  Name: \"{}\"", name);
            }
            if !existing.tags.is_empty() {
                println!("  Tags: {}", existing.tags.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(", "));
            }
            
            println!("\nParent [{}] has:", parent_index);
            if let Some(ref parent_meta) = parent_meta {
                if let Some(name) = &parent_meta.name {
                    println!("  Name: \"{}\"", name);
                }
                if !parent_meta.tags.is_empty() {
                    println!("  Tags: {}", parent_meta.tags.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(", "));
                }
            } else {
                println!("  (no metadata)");
            }
            
            print!("\nThis will REPLACE child's metadata with parent's.\nContinue? (y/n): ");
            std::io::stdout().flush()?;
            
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            
            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Cancelled.");
                return Ok(());
            }
            }
        }
    }
    
    // Perform linking
    let mut child_metadata = metadata.get(&child.id).cloned().unwrap_or_default();
    child_metadata.parent_session_id = Some(parent.id.clone());
    child_metadata.manually_unlinked = false; // Clear flag when manually linking
    
    // Inherit name and tags from parent
    if let Some(ref parent_meta) = parent_meta {
        child_metadata.name = parent_meta.name.clone();
        child_metadata.tags = parent_meta.tags.clone();
    }
    
    metadata.insert(child.id.clone(), child_metadata);
    save_metadata(&metadata)?;
    
    println!("✓ Linked session [{}] to parent [{}]", child_index, parent_index);
    
    if let Some(parent_meta) = parent_meta {
        if let Some(name) = &parent_meta.name {
            println!("✓ Inherited name: \"{}\"", name);
        }
        if !parent_meta.tags.is_empty() {
            println!("✓ Inherited tags: {}", parent_meta.tags.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(", "));
        }
    }
    
    Ok(())
}
