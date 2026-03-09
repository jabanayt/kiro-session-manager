//! Session listing and validation.

use std::collections::{HashMap, HashSet};

use crate::config::load_config;
use crate::data::{KsmDatabase, SessionSource};
use crate::error::Result;
use crate::models::{Session, SessionMetadata};
use crate::services::{chains, metadata};

/// Result of listing sessions.
pub struct SessionListResult {
    pub all_sessions: Vec<Session>,
    pub metadata: HashMap<String, SessionMetadata>,
    pub auto_linked: usize,
    /// Session IDs that are indexed (for display markers).
    pub indexed_session_ids: Vec<String>,
}

/// Fetch sessions, load metadata, run auto-clean and auto-detect.
pub fn list_sessions(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    directory: &str,
) -> Result<SessionListResult> {
    let sessions = source.list_sessions()?;
    let mut meta = db.load_all_metadata()?;
    let config = load_config()?;

    if config.auto_clean && !sessions.is_empty() {
        metadata::clean_stale_metadata(&sessions, &mut meta, db)?;
    }

    let auto_linked = if config.auto_detect_continuations && !sessions.is_empty() {
        chains::auto_link_continuations(&sessions, &mut meta, source, db)?
    } else {
        0
    };

    // Get indexed session IDs for display
    let indexed = db.list_indexed(directory)?;
    let indexed_session_ids: Vec<String> = indexed.iter().map(|a| a.session_id.clone()).collect();

    Ok(SessionListResult {
        all_sessions: sessions,
        metadata: meta,
        auto_linked,
        indexed_session_ids,
    })
}

/// Get existing name and tags for a session (for CLI prompts).
pub fn get_session_defaults(
    session_id: &str,
    db: &KsmDatabase,
) -> Result<(Option<String>, Vec<String>)> {
    let meta = db.get_metadata(session_id)?;
    let name = meta.as_ref().and_then(|m| m.name.clone());
    let tags: Vec<String> = meta
        .map(|m| {
            let mut tags: Vec<String> = m.tags.iter().cloned().collect();
            tags.sort();
            tags
        })
        .unwrap_or_default();
    Ok((name, tags))
}

/// Filter out parent sessions (sessions referenced as parents by other sessions).
pub fn visible_session_indices(
    sessions: &[Session],
    metadata: &HashMap<String, SessionMetadata>,
) -> Vec<usize> {
    let parent_ids: HashSet<_> = metadata
        .values()
        .filter_map(|m| m.parent_session_id.as_ref())
        .collect();

    sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| !parent_ids.contains(&s.id))
        .map(|(i, _)| i)
        .collect()
}

/// Validate that an index is within range.
pub fn validate_index(index: usize, session_count: usize) -> Result<()> {
    if index >= session_count {
        return Err(crate::error::KsmError::IndexOutOfRange {
            index,
            max: session_count.saturating_sub(1),
        });
    }
    Ok(())
}

/// Validate multiple indices.
pub fn validate_indices(indices: &[usize], session_count: usize) -> Result<()> {
    for &idx in indices {
        validate_index(idx, session_count)?;
    }
    Ok(())
}

/// A single difference between two session sources.
pub struct SessionDiff {
    pub index: usize,
    pub field: String,
    pub source_a: String,
    pub source_b: String,
}

/// Result of comparing two session sources.
pub struct CompareResult {
    pub source_a_count: usize,
    pub source_b_count: usize,
    pub differences: Vec<SessionDiff>,
}

/// Compare sessions from two sources (e.g., database vs CLI).
pub fn compare_sources(
    source_a: &dyn SessionSource,
    source_b: &dyn SessionSource,
) -> Result<CompareResult> {
    let sessions_a = source_a.list_sessions()?;
    let sessions_b = source_b.list_sessions()?;

    let mut differences = Vec::new();

    for (i, (a, b)) in sessions_a.iter().zip(sessions_b.iter()).enumerate() {
        if a.id != b.id {
            differences.push(SessionDiff {
                index: i,
                field: "ID".to_string(),
                source_a: a.id.clone(),
                source_b: b.id.clone(),
            });
        }
        if a.preview != b.preview {
            differences.push(SessionDiff {
                index: i,
                field: "Preview".to_string(),
                source_a: a.preview.clone(),
                source_b: b.preview.clone(),
            });
        }
        if a.msg_count != b.msg_count {
            differences.push(SessionDiff {
                index: i,
                field: "Count".to_string(),
                source_a: a.msg_count.to_string(),
                source_b: b.msg_count.to_string(),
            });
        }
        if a.updated_at != b.updated_at {
            differences.push(SessionDiff {
                index: i,
                field: "Timestamp".to_string(),
                source_a: a.updated_at.to_string(),
                source_b: b.updated_at.to_string(),
            });
        }
    }

    Ok(CompareResult {
        source_a_count: sessions_a.len(),
        source_b_count: sessions_b.len(),
        differences,
    })
}
