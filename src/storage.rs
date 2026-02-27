use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use crate::config::load_config;
use crate::models::{Session, SessionMetadata};

pub fn metadata_path() -> Result<PathBuf> {
    let config = load_config()?;

    match config.metadata_storage.as_str() {
        "global" => {
            let home = std::env::var("HOME").context("HOME environment variable not set")?;
            let ksm_dir = PathBuf::from(home).join(".ksm");
            fs::create_dir_all(&ksm_dir)?;
            Ok(ksm_dir.join("metadata.json"))
        }
        "local" => {
            let cwd = std::env::current_dir().context("Failed to get current directory")?;
            let kiro_dir = cwd.join(".kiro");
            fs::create_dir_all(&kiro_dir)?;
            Ok(kiro_dir.join("ksm-metadata.json"))
        }
        "custom" => {
            let custom = config
                .custom_path
                .context("custom_path not set in config when metadata_storage is 'custom'")?;
            let path = PathBuf::from(custom);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            Ok(path)
        }
        _ => anyhow::bail!(
            "Invalid metadata_storage option: {}",
            config.metadata_storage
        ),
    }
}

pub fn load_metadata() -> Result<HashMap<String, SessionMetadata>> {
    let path = metadata_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = fs::read_to_string(&path)?;
    let metadata = serde_json::from_str(&content)?;
    Ok(metadata)
}

pub fn save_metadata(metadata: &HashMap<String, SessionMetadata>) -> Result<()> {
    let path = metadata_path()?;
    let content = serde_json::to_string_pretty(metadata)?;
    fs::write(&path, content)?;
    Ok(())
}

pub fn cleanup_stale_metadata(
    metadata: &mut HashMap<String, SessionMetadata>,
    sessions: &[Session],
) -> Result<()> {
    // Defence in depth: never clean when sessions list is empty
    // An empty list likely means database/CLI failure, not zero real sessions
    if sessions.is_empty() {
        return Ok(());
    }

    let current_dir = std::env::current_dir()
        .context("Failed to get current directory")?
        .to_string_lossy()
        .to_string();

    let session_ids: HashSet<_> = sessions.iter().map(|s| s.id.as_str()).collect();

    // Auto-migrate legacy entries: add directory field to active sessions
    // TODO(v0.2.0): Remove legacy support for entries without directory field
    let mut migrated = false;
    for (id, meta) in metadata.iter_mut() {
        if meta.directory.is_none() && session_ids.contains(id.as_str()) {
            meta.directory = Some(current_dir.clone());
            migrated = true;
        }
    }

    if migrated {
        save_metadata(metadata)?;
    }

    let stale_ids: Vec<_> = metadata
        .iter()
        .filter(|(id, meta)| {
            // Only consider entries from current directory
            if let Some(dir) = &meta.directory {
                dir == &current_dir && !session_ids.contains(id.as_str())
            } else {
                // Legacy entries without directory - skip (could belong to any directory)
                false
            }
        })
        .map(|(id, _)| id.clone())
        .collect();

    if !stale_ids.is_empty() {
        for id in stale_ids {
            metadata.remove(&id);
        }
        save_metadata(metadata)?;
    }

    Ok(())
}

/// Get all session IDs in a chain (both parents and children)
/// Returns Vec with the session itself, all parents, and all children
/// Only includes sessions that actually exist in the sessions list
pub fn get_full_chain(
    session_id: &str,
    metadata: &HashMap<String, SessionMetadata>,
    sessions: &[Session],
) -> Vec<String> {
    let session_ids: std::collections::HashSet<_> =
        sessions.iter().map(|s| s.id.as_str()).collect();
    let mut chain = vec![session_id.to_string()];

    // Walk up to find all parents
    let mut current = session_id.to_string();
    while let Some(meta) = metadata.get(&current) {
        if let Some(parent_id) = &meta.parent_session_id {
            if session_ids.contains(parent_id.as_str()) {
                chain.push(parent_id.clone());
                current = parent_id.clone();
            } else {
                break;
            }
        } else {
            break;
        }
    }

    // Walk down to find all children
    fn find_children(
        parent_id: &str,
        metadata: &HashMap<String, SessionMetadata>,
        chain: &mut Vec<String>,
        session_ids: &std::collections::HashSet<&str>,
    ) {
        for (id, meta) in metadata {
            if let Some(pid) = &meta.parent_session_id {
                if pid == parent_id && !chain.contains(id) && session_ids.contains(id.as_str()) {
                    chain.push(id.clone());
                    find_children(id, metadata, chain, session_ids);
                }
            }
        }
    }

    find_children(session_id, metadata, &mut chain, &session_ids);

    chain
}

/// Get ordered chain for display (youngest child → oldest parent)
pub fn get_ordered_chain(
    session_id: &str,
    metadata: &HashMap<String, SessionMetadata>,
    sessions: &[Session],
) -> Vec<String> {
    let chain = get_full_chain(session_id, metadata, sessions);

    if chain.len() <= 1 {
        return chain;
    }

    let mut ordered = Vec::new();

    // Find the youngest child (no one points to it as parent)
    let youngest = chain
        .iter()
        .find(|id| {
            !metadata
                .values()
                .any(|m| m.parent_session_id.as_ref() == Some(id))
        })
        .unwrap_or(&session_id.to_string())
        .clone();

    // Walk up the parent chain from youngest
    ordered.push(youngest.clone());
    let mut current = youngest;
    while let Some(meta) = metadata.get(&current) {
        if let Some(parent_id) = &meta.parent_session_id {
            if chain.contains(parent_id) {
                ordered.push(parent_id.clone());
                current = parent_id.clone();
            } else {
                break;
            }
        } else {
            break;
        }
    }

    ordered
}

/// Relink sessions around a deleted session to maintain chain integrity
/// Updates child's parent_session_id to point to grandparent
pub fn relink_around_session(
    session_id: &str,
    metadata: &mut HashMap<String, SessionMetadata>,
) -> Result<()> {
    // Find the parent of the session being deleted
    let parent_id = metadata
        .get(session_id)
        .and_then(|m| m.parent_session_id.clone());

    // Find any child that points to this session
    let child_id = metadata
        .iter()
        .find(|(_, m)| m.parent_session_id.as_ref() == Some(&session_id.to_string()))
        .map(|(id, _)| id.clone());

    // If there's a child, update its parent_session_id
    if let Some(child) = child_id {
        if let Some(child_meta) = metadata.get_mut(&child) {
            child_meta.parent_session_id = parent_id;
        }
    }

    Ok(())
}
