use log::debug;
use std::collections::{HashMap, HashSet};

use crate::data::{MetadataStore, SessionSource};
use crate::error::{KsmError, Result};
use crate::models::{Session, SessionMetadata};

// --- Result types ---

/// Result of a link operation.
pub struct LinkResult {
    pub child_id: String,
    pub parent_id: String,
    pub inherited_name: Option<String>,
    pub inherited_tags: HashSet<String>,
}

/// Result of an unlink operation.
pub struct UnlinkResult {
    pub session_id: String,
    pub former_parent_id: String,
    pub metadata_cleared: bool,
}

/// A detected continuation candidate (child → potential parent).
pub struct DetectionCandidate {
    pub child: Session,
    pub parent_id: String,
}

// --- Chain traversal ---

/// Get all session IDs in a chain (parents + children).
///
/// Only includes sessions that exist in the provided sessions list.
pub fn get_full_chain(
    session_id: &str,
    metadata: &HashMap<String, SessionMetadata>,
    sessions: &[Session],
) -> Vec<String> {
    let session_ids: HashSet<_> = sessions.iter().map(|s| s.id.as_str()).collect();
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
        session_ids: &HashSet<&str>,
    ) {
        for (id, meta) in metadata {
            if let Some(pid) = &meta.parent_session_id
                && pid == parent_id
                && !chain.contains(id)
                && session_ids.contains(id.as_str())
            {
                chain.push(id.clone());
                find_children(id, metadata, chain, session_ids);
            }
        }
    }

    find_children(session_id, metadata, &mut chain, &session_ids);
    chain
}

/// Get ordered chain for display (youngest child → oldest parent).
pub fn get_ordered_chain(
    session_id: &str,
    metadata: &HashMap<String, SessionMetadata>,
    sessions: &[Session],
) -> Vec<String> {
    let chain = get_full_chain(session_id, metadata, sessions);
    if chain.len() <= 1 {
        return chain;
    }

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

    // Walk up from youngest
    let mut ordered = vec![youngest.clone()];
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

/// Relink sessions around a deleted session to maintain chain integrity.
///
/// Updates child's parent_session_id to point to grandparent.
/// Returns `Some((child_id, new_parent_id))` if a relink occurred, `None` if
/// the deleted session had no child to relink.
pub fn relink_around_session(
    session_id: &str,
    metadata: &mut HashMap<String, SessionMetadata>,
) -> Option<(String, Option<String>)> {
    let parent_id = metadata
        .get(session_id)
        .and_then(|m| m.parent_session_id.clone());

    let child_id = metadata
        .iter()
        .find(|(_, m)| m.parent_session_id.as_ref() == Some(&session_id.to_string()))
        .map(|(id, _)| id.clone());

    if let Some(child) = &child_id
        && let Some(child_meta) = metadata.get_mut(child)
    {
        child_meta.parent_session_id = parent_id.clone();
    }

    child_id.map(|child| (child, parent_id))
}

// --- Link helpers ---

/// Apply a parent link to a child session's metadata (mutation only).
///
/// Sets parent_session_id, clears manually_unlinked, inherits name/tags/directory
/// from parent. Falls back to current_dir if parent has no directory.
/// Does NOT validate or save — caller is responsible for both.
fn apply_link(child_id: &str, parent_id: &str, metadata: &mut HashMap<String, SessionMetadata>) {
    let parent_meta = metadata.get(parent_id).cloned();

    let mut child_metadata = metadata.get(child_id).cloned().unwrap_or_default();
    child_metadata.parent_session_id = Some(parent_id.to_string());
    child_metadata.manually_unlinked = false;

    if let Some(parent) = &parent_meta {
        child_metadata.name = parent.name.clone();
        child_metadata.tags = parent.tags.clone();
        child_metadata.directory = parent.directory.clone().or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|d| d.to_string_lossy().to_string())
        });
    } else if child_metadata.directory.is_none() {
        child_metadata.directory = std::env::current_dir()
            .ok()
            .map(|d| d.to_string_lossy().to_string());
    }

    metadata.insert(child_id.to_string(), child_metadata);
}

// --- Link / Unlink operations ---

/// Link a child session to a parent session (complete operation).
///
/// Includes all validation, metadata conflict detection, and data mutation
/// in one call. If the child has metadata that differs from the parent and
/// `confirm_metadata_replace` is false, returns `Err(KsmError::MetadataConflict)`
/// so the CLI/TUI can prompt the user and retry with `true`.
pub fn link_sessions(
    child_id: &str,
    parent_id: &str,
    confirm_metadata_replace: bool,
    metadata: &mut HashMap<String, SessionMetadata>,
    store: &dyn MetadataStore,
) -> Result<LinkResult> {
    // --- Validation ---

    if child_id == parent_id {
        return Err(KsmError::ChainConflict(
            "Cannot link a session to itself".to_string(),
        ));
    }

    if metadata
        .values()
        .any(|m| m.parent_session_id.as_ref() == Some(&child_id.to_string()))
    {
        return Err(KsmError::ChainConflict(
            "Session is already a parent. Cannot link it as a child.".to_string(),
        ));
    }

    if metadata
        .values()
        .any(|m| m.parent_session_id.as_ref() == Some(&parent_id.to_string()))
    {
        return Err(KsmError::ChainConflict(
            "Parent already has a child. Cannot link another.".to_string(),
        ));
    }

    if let Some(existing) = metadata.get(child_id)
        && existing.parent_session_id.is_some()
    {
        return Err(KsmError::ChainConflict(
            "Session is already linked to a parent. Unlink first.".to_string(),
        ));
    }

    // --- Metadata conflict check ---

    if !confirm_metadata_replace {
        let child_meta = metadata.get(child_id);
        let parent_meta = metadata.get(parent_id);

        if let Some(existing) = child_meta
            && (existing.name.is_some() || !existing.tags.is_empty())
        {
            let conflicts = if let Some(parent) = parent_meta {
                existing.name != parent.name || existing.tags != parent.tags
            } else {
                true
            };

            if conflicts {
                return Err(KsmError::MetadataConflict {
                    child_id: child_id.to_string(),
                    parent_id: parent_id.to_string(),
                });
            }
        }
    }

    // --- Execute link ---

    apply_link(child_id, parent_id, metadata);
    store.save(metadata)?;

    let child_metadata = metadata.get(child_id).unwrap();
    Ok(LinkResult {
        child_id: child_id.to_string(),
        parent_id: parent_id.to_string(),
        inherited_name: child_metadata.name.clone(),
        inherited_tags: child_metadata.tags.clone(),
    })
}

/// Unlink a session from its parent.
///
/// `clear_metadata`: if true, removes inherited name and tags.
pub fn execute_unlink(
    session_id: &str,
    clear_metadata: bool,
    metadata: &mut HashMap<String, SessionMetadata>,
    store: &dyn MetadataStore,
) -> Result<UnlinkResult> {
    // Single check: session must exist in metadata AND have a parent link
    let parent_id = metadata
        .get(session_id)
        .and_then(|m| m.parent_session_id.clone())
        .ok_or_else(|| KsmError::ChainConflict("Session is not linked to a parent".to_string()))?;

    let mut updated = metadata.get(session_id).cloned().unwrap_or_default();
    updated.parent_session_id = None;
    updated.manually_unlinked = true;

    if clear_metadata {
        updated.name = None;
        updated.tags.clear();
    }

    metadata.insert(session_id.to_string(), updated);
    store.save(metadata)?;

    Ok(UnlinkResult {
        session_id: session_id.to_string(),
        former_parent_id: parent_id,
        metadata_cleared: clear_metadata,
    })
}

// --- Detection ---

/// Find potential parent sessions for a child session using message ID overlap
/// and timestamp proximity.
pub fn find_potential_parents(
    child_id: &str,
    sessions: &[Session],
    source: &dyn SessionSource,
) -> Result<Vec<String>> {
    let child_msg_ids = source.get_message_ids(child_id)?;
    let (child_created, _) = source.get_timestamps(child_id)?;
    let mut candidates = Vec::new();

    // Primary: message_id overlap
    for session in sessions {
        if session.id == child_id {
            continue;
        }
        let parent_msg_ids = source.get_message_ids(&session.id)?;
        if child_msg_ids.iter().any(|id| parent_msg_ids.contains(id)) {
            let (created, _) = source.get_timestamps(&session.id)?;
            if created < child_created {
                candidates.push((session.id.clone(), created));
            }
        }
    }

    if !candidates.is_empty() {
        candidates.sort_by_key(|(_, created)| -created);
        return Ok(candidates.into_iter().map(|(id, _)| id).collect());
    }

    // Fallback: timestamp matching (within 5 minutes)
    for session in sessions {
        if session.id == child_id {
            continue;
        }
        if source.has_compact_tag(&session.id).unwrap_or(false) {
            continue;
        }
        let (_, parent_updated) = source.get_timestamps(&session.id)?;
        let time_diff = child_created - parent_updated;
        if time_diff > 0 && time_diff <= 5 * 60 * 1000 {
            candidates.push((session.id.clone(), parent_updated));
        }
    }

    candidates.sort_by_key(|(_, updated)| -updated);
    Ok(candidates.into_iter().map(|(id, _)| id).collect())
}

/// Detect unlinked compacted sessions that could be linked to parents.
pub fn detect_unlinked_continuations(
    sessions: &[Session],
    metadata: &HashMap<String, SessionMetadata>,
    source: &dyn SessionSource,
    force: bool,
) -> Result<Vec<DetectionCandidate>> {
    let mut candidates = Vec::new();

    for session in sessions {
        // Skip if already linked
        if let Some(meta) = metadata.get(&session.id) {
            if meta.parent_session_id.is_some() {
                continue;
            }
            if !force && meta.manually_unlinked {
                continue;
            }
        }

        // Check if this session has Compact tag
        if !source.has_compact_tag(&session.id)? {
            continue;
        }

        let parent_candidates = find_potential_parents(&session.id, sessions, source)?;
        if parent_candidates.is_empty() {
            continue;
        }

        let parent_id = &parent_candidates[0];

        // Check if parent already has a child (enforce linear chains)
        if metadata
            .values()
            .any(|m| m.parent_session_id.as_ref() == Some(parent_id))
        {
            continue;
        }

        candidates.push(DetectionCandidate {
            child: session.clone(),
            parent_id: parent_id.clone(),
        });
    }

    debug!("Detected {} continuation candidates", candidates.len());
    Ok(candidates)
}

/// Auto-link all detected continuations (no user interaction).
pub fn auto_link_continuations(
    sessions: &[Session],
    metadata: &mut HashMap<String, SessionMetadata>,
    source: &dyn SessionSource,
    store: &dyn MetadataStore,
) -> Result<usize> {
    let candidates = detect_unlinked_continuations(sessions, metadata, source, false)?;
    if candidates.is_empty() {
        return Ok(0);
    }

    for candidate in &candidates {
        apply_link(&candidate.child.id, &candidate.parent_id, metadata);
    }

    store.save(metadata)?;
    Ok(candidates.len())
}
