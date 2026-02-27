use std::collections::{HashMap, HashSet};

use crate::config::load_config;
use crate::data::{MetadataStore, SessionSource};
use crate::error::Result;
use crate::models::{Session, SessionMetadata};
use crate::services::{chains, metadata};

/// Result of listing sessions.
pub struct SessionListResult {
    /// All sessions from the source (unfiltered, for index lookups).
    pub all_sessions: Vec<Session>,
    /// Metadata for all sessions.
    pub metadata: HashMap<String, SessionMetadata>,
    /// Number of sessions auto-linked during this list operation.
    pub auto_linked: usize,
}

/// Fetch sessions, load metadata, run auto-clean and auto-detect.
///
/// This is the primary entry point for getting session data.
pub fn list_sessions(
    source: &dyn SessionSource,
    store: &dyn MetadataStore,
) -> Result<SessionListResult> {
    let sessions = source.list_sessions()?;
    let mut meta = store.load()?;
    let config = load_config()?;

    // Auto-clean stale metadata if enabled
    if config.auto_clean && !sessions.is_empty() {
        metadata::clean_stale_metadata(&sessions, &mut meta, store)?;
    }

    // Auto-detect continuations if enabled
    let auto_linked = if config.auto_detect_continuations && !sessions.is_empty() {
        chains::auto_link_continuations(&sessions, &mut meta, source, store)?
    } else {
        0
    };

    Ok(SessionListResult {
        all_sessions: sessions,
        metadata: meta,
        auto_linked,
    })
}

/// Filter out parent sessions (sessions referenced as parents by other sessions).
///
/// Returns indices into the original sessions vec.
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
