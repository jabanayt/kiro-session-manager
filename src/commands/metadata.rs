use anyhow::Result;
use std::collections::HashSet;

use crate::kiro::get_sessions;
use crate::storage::{load_metadata, save_metadata};

pub fn set_name(index: usize, name: &str) -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;
    
    if index >= sessions.len() {
        anyhow::bail!("Index {} out of range (max: {})", index, sessions.len() - 1);
    }
    
    let session = &sessions[index];
    metadata.entry(session.id.clone())
        .or_default()
        .name = Some(name.to_string());
    
    save_metadata(&metadata)?;
    println!("Set name for session [{}]: {}", index, name);
    Ok(())
}

pub fn add_tags(index: usize, tags: &[String]) -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;
    
    if index >= sessions.len() {
        anyhow::bail!("Index {} out of range (max: {})", index, sessions.len() - 1);
    }
    
    let session = &sessions[index];
    let entry = metadata.entry(session.id.clone()).or_default();
    
    for tag in tags {
        entry.tags.insert(tag.clone());
    }
    
    save_metadata(&metadata)?;
    println!("Added tags to session [{}]: {}", index, tags.join(", "));
    Ok(())
}

pub fn remove_tags(index: usize, tags: &[String]) -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;
    
    if index >= sessions.len() {
        anyhow::bail!("Index {} out of range (max: {})", index, sessions.len() - 1);
    }
    
    let session = &sessions[index];
    if let Some(entry) = metadata.get_mut(&session.id) {
        for tag in tags {
            entry.tags.remove(tag);
        }
        save_metadata(&metadata)?;
        println!("Removed tags from session [{}]: {}", index, tags.join(", "));
    } else {
        println!("No metadata found for session [{}]", index);
    }
    
    Ok(())
}

pub fn clean_metadata() -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;
    
    let session_ids: HashSet<_> = sessions.iter().map(|s| s.id.as_str()).collect();
    let stale_ids: Vec<_> = metadata.keys()
        .filter(|id| !session_ids.contains(id.as_str()))
        .cloned()
        .collect();
    
    if stale_ids.is_empty() {
        println!("No stale metadata found.");
        return Ok(());
    }
    
    println!("Removing metadata for {} deleted session(s):", stale_ids.len());
    for id in &stale_ids {
        if let Some(meta) = metadata.get(id) {
            let display = meta.name.as_deref().unwrap_or(&id[..8]);
            println!("  - {}", display);
        }
        metadata.remove(id);
    }
    
    save_metadata(&metadata)?;
    println!("\nDone!");
    Ok(())
}
