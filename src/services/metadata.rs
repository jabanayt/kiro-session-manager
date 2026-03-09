//! Metadata operations: name, tags, cleaning.

use std::collections::{HashMap, HashSet};

use crate::data::KsmDatabase;
use crate::error::Result;
use crate::models::{Session, SessionMetadata};
use crate::services::chains;

/// Whether a metadata operation targets a single session or an entire chain.
///
/// CLI/TUI passes this based on --chain flag. The service resolves chain
/// membership internally via chains::get_full_chain.
pub enum MetadataScope {
    /// Operate on this session only.
    Single(String),
    /// Operate on this session and all sessions in its chain.
    Chain(String),
}

/// Result of a metadata update operation.
pub struct MetadataUpdateResult {
    /// Session IDs that were modified.
    pub affected_ids: Vec<String>,
}

/// Resolve scope to target session IDs.
fn resolve_scope(
    scope: &MetadataScope,
    metadata: &HashMap<String, SessionMetadata>,
    sessions: &[Session],
) -> Vec<String> {
    match scope {
        MetadataScope::Single(id) => vec![id.clone()],
        MetadataScope::Chain(id) => {
            let chain = chains::get_full_chain(id, metadata, sessions);
            if chain.len() <= 1 {
                vec![id.clone()]
            } else {
                chain
            }
        }
    }
}

/// Set name on a session or chain.
pub fn set_name(
    scope: MetadataScope,
    name: &str,
    sessions: &[Session],
    metadata: &mut HashMap<String, SessionMetadata>,
    db: &KsmDatabase,
) -> Result<MetadataUpdateResult> {
    let target_ids = resolve_scope(&scope, metadata, sessions);
    let current_dir = std::env::current_dir()?.to_string_lossy().to_string();

    for session_id in &target_ids {
        let entry = metadata.entry(session_id.clone()).or_default();
        entry.name = Some(name.to_string());
        entry.directory = Some(current_dir.clone());
        db.set_metadata(session_id, entry)?;
    }

    Ok(MetadataUpdateResult {
        affected_ids: target_ids,
    })
}

/// Add tags to a session or chain.
pub fn add_tags(
    scope: MetadataScope,
    tags: &[String],
    sessions: &[Session],
    metadata: &mut HashMap<String, SessionMetadata>,
    db: &KsmDatabase,
) -> Result<MetadataUpdateResult> {
    let target_ids = resolve_scope(&scope, metadata, sessions);
    let current_dir = std::env::current_dir()?.to_string_lossy().to_string();

    for session_id in &target_ids {
        let entry = metadata.entry(session_id.clone()).or_default();
        entry.directory = Some(current_dir.clone());
        for tag in tags {
            entry.tags.insert(tag.clone());
        }
        db.set_metadata(session_id, entry)?;
    }

    Ok(MetadataUpdateResult {
        affected_ids: target_ids,
    })
}

/// Remove tags from a session or chain.
pub fn remove_tags(
    scope: MetadataScope,
    tags: &[String],
    sessions: &[Session],
    metadata: &mut HashMap<String, SessionMetadata>,
    db: &KsmDatabase,
) -> Result<MetadataUpdateResult> {
    let target_ids = resolve_scope(&scope, metadata, sessions);
    let current_dir = std::env::current_dir()?.to_string_lossy().to_string();

    for session_id in &target_ids {
        if let Some(entry) = metadata.get_mut(session_id) {
            entry.directory = Some(current_dir.clone());
            for tag in tags {
                entry.tags.remove(tag);
            }
            db.set_metadata(session_id, entry)?;
        }
    }

    Ok(MetadataUpdateResult {
        affected_ids: target_ids,
    })
}

/// Auto-clean stale metadata entries and migrate legacy entries.
///
/// Called by sessions::list_sessions when auto_clean is enabled.
/// Includes legacy migration (adding directory field to entries without one).
pub fn clean_stale_metadata(
    sessions: &[Session],
    metadata: &mut HashMap<String, SessionMetadata>,
    db: &KsmDatabase,
) -> Result<()> {
    if sessions.is_empty() {
        return Ok(());
    }

    let current_dir = std::env::current_dir()?.to_string_lossy().to_string();
    let session_ids: HashSet<_> = sessions.iter().map(|s| s.id.as_str()).collect();

    // Auto-migrate legacy entries: add directory field to active sessions
    // TODO(v0.3.0): Remove this migration block - all users should have directory field by then
    for (id, meta) in metadata.iter_mut() {
        if meta.directory.is_none() && session_ids.contains(id.as_str()) {
            meta.directory = Some(current_dir.clone());
            db.set_metadata(id, meta)?;
        }
    }

    // Remove stale entries
    let stale_ids: Vec<_> = metadata
        .iter()
        .filter(|(id, meta)| {
            if let Some(dir) = &meta.directory {
                dir == &current_dir && !session_ids.contains(id.as_str())
            } else {
                false
            }
        })
        .map(|(id, _)| id.clone())
        .collect();

    for id in &stale_ids {
        metadata.remove(id);
        db.delete_metadata(id)?;
    }

    Ok(())
}

/// Clean up metadata for sessions that no longer exist.
///
/// Returns IDs and display names of removed entries.
/// Used by cmd_clean_metadata (explicit command) -- does NOT include
/// legacy migration since that runs via clean_stale_metadata on every list.
pub fn clean_metadata(
    sessions: &[Session],
    metadata: &mut HashMap<String, SessionMetadata>,
    db: &KsmDatabase,
) -> Result<Vec<(String, String)>> {
    let current_dir = std::env::current_dir()?.to_string_lossy().to_string();
    let session_ids: HashSet<_> = sessions.iter().map(|s| s.id.as_str()).collect();

    let stale: Vec<(String, String)> = metadata
        .iter()
        .filter(|(id, meta)| {
            if let Some(dir) = &meta.directory {
                dir == &current_dir && !session_ids.contains(id.as_str())
            } else {
                false
            }
        })
        .map(|(id, meta)| {
            let display = meta.name.as_deref().unwrap_or(&id[..8]).to_string();
            (id.clone(), display)
        })
        .collect();

    for (id, _) in &stale {
        metadata.remove(id);
        db.delete_metadata(id)?;
    }

    Ok(stale)
}

/// Get the chain context for a session (for CLI/TUI display).
///
/// Returns None if session is not part of a chain.
pub struct ChainContext {
    pub chain_ids: Vec<String>,
    pub ordered_ids: Vec<String>,
}

pub fn get_chain_context(
    session_id: &str,
    metadata: &HashMap<String, SessionMetadata>,
    sessions: &[Session],
) -> Option<ChainContext> {
    let chain = chains::get_full_chain(session_id, metadata, sessions);
    if chain.len() <= 1 {
        return None;
    }
    let ordered = chains::get_ordered_chain(session_id, metadata, sessions);
    Some(ChainContext {
        chain_ids: chain,
        ordered_ids: ordered,
    })
}
