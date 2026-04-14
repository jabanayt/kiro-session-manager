use std::collections::HashMap;

use crate::data::{KsmDatabase, SessionSource};
use crate::error::{KsmError, Result};
use crate::models::{ArchiveStatus, Session, SessionMetadata, SourceType};
use crate::services::sessions;

/// How the CLI/TUI identified the session to resume.
pub enum ResumeTarget {
    Index(usize),
    Tag(String),
    Name(String),
    Last,
}

/// Result of resolving a resume target.
pub enum ResumeResult {
    /// Session found, ready to launch kiro-cli with explicit UI flag.
    Ready {
        session_id: String,
        display_name: String,
        source_type: SourceType,
    },
    /// Multiple sessions match a tag. Caller must pick one and retry with Index.
    MultipleMatches {
        tag: String,
        matches: Vec<ResumeMatch>,
    },
    // REMOVED: LaunchDirect variant no longer needed
}

/// A single match when resolving by tag or name.
pub struct ResumeMatch {
    pub session_id: String,
    pub original_index: usize,
    pub display_name: String,
    pub source_type: SourceType,
}

/// Single entry point for resume operations.
///
/// Resolves the target to a session, updates its timestamp (except for Last),
/// and returns enough info for the CLI/TUI to launch kiro-cli or prompt for
/// disambiguation.
pub fn resume(
    target: ResumeTarget,
    source: &dyn SessionSource,
    db: &KsmDatabase,
    directory: &str,
) -> Result<ResumeResult> {
    match target {
        ResumeTarget::Last => {
            let list_result = sessions::session_context(source, db, directory)?;
            if list_result.all_sessions.is_empty() {
                return Err(KsmError::SessionNotFound("No sessions found".to_string()));
            }
            // all_sessions is sorted by updated_at DESC, so [0] is most recent
            let session = &list_result.all_sessions[0];
            let display_name = list_result
                .metadata
                .get(&session.id)
                .and_then(|m| m.name.clone())
                .unwrap_or_else(|| session.preview.clone());

            set_pending_reindex_if_indexed(&session.id, db, session.source_type)?;

            Ok(ResumeResult::Ready {
                session_id: session.id.clone(),
                display_name,
                source_type: session.source_type,
            })
        }

        ResumeTarget::Index(index) => {
            let list_result = sessions::session_context(source, db, directory)?;
            sessions::validate_index(index, list_result.all_sessions.len())?;
            let session = &list_result.all_sessions[index];
            let display_name = list_result
                .metadata
                .get(&session.id)
                .and_then(|m| m.name.clone())
                .unwrap_or_else(|| session.preview.clone());

            // Set pending reindex if session is indexed
            set_pending_reindex_if_indexed(&session.id, db, session.source_type)?;

            Ok(ResumeResult::Ready {
                session_id: session.id.clone(),
                display_name,
                source_type: session.source_type,
            })
        }

        ResumeTarget::Name(name) => {
            let list_result = sessions::session_context(source, db, directory)?;
            let found = find_by_name(&name, &list_result.all_sessions, &list_result.metadata)?;

            set_pending_reindex_if_indexed(&found.session_id, db, found.source_type)?;

            Ok(ResumeResult::Ready {
                session_id: found.session_id,
                display_name: found.display_name,
                source_type: found.source_type,
            })
        }

        ResumeTarget::Tag(tag) => {
            let list_result = sessions::session_context(source, db, directory)?;
            let matches = find_by_tag(&tag, &list_result.all_sessions, &list_result.metadata);
            match matches.len() {
                0 => Err(KsmError::SessionNotFound(format!(
                    "No sessions found with tag '{}'",
                    tag
                ))),
                1 => {
                    set_pending_reindex_if_indexed(
                        &matches[0].session_id,
                        db,
                        matches[0].source_type,
                    )?;

                    Ok(ResumeResult::Ready {
                        session_id: matches[0].session_id.clone(),
                        display_name: matches[0].display_name.clone(),
                        source_type: matches[0].source_type,
                    })
                }
                _ => Ok(ResumeResult::MultipleMatches { tag, matches }),
            }
        }
    }
}

/// Set pending_reindex if the session is indexed.
fn set_pending_reindex_if_indexed(
    session_id: &str,
    db: &KsmDatabase,
    source_type: SourceType,
) -> Result<()> {
    if let Some(ArchiveStatus::Indexed { .. }) =
        db.get_archive_status_for_source(session_id, source_type)?
    {
        db.set_pending_reindex(session_id, source_type)?;
    }
    Ok(())
}

// --- Private helpers ---

/// Resolve a tag to matching sessions.
fn find_by_tag(
    tag: &str,
    sessions: &[Session],
    metadata: &HashMap<String, SessionMetadata>,
) -> Vec<ResumeMatch> {
    sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            metadata
                .get(&s.id)
                .map(|m| {
                    m.tags
                        .iter()
                        .any(|t| t.to_lowercase() == tag.to_lowercase())
                })
                .unwrap_or(false)
        })
        .map(|(idx, s)| ResumeMatch {
            session_id: s.id.clone(),
            original_index: idx,
            display_name: metadata
                .get(&s.id)
                .and_then(|m| m.name.clone())
                .unwrap_or_else(|| s.preview.clone()),
            source_type: s.source_type, // Get from session in iterator
        })
        .collect()
}

/// Resolve a name to a matching session.
fn find_by_name(
    name: &str,
    sessions: &[Session],
    metadata: &HashMap<String, SessionMetadata>,
) -> Result<ResumeMatch> {
    sessions
        .iter()
        .enumerate()
        .find(|(_, s)| {
            metadata
                .get(&s.id)
                .and_then(|m| m.name.as_deref())
                .map(|n| n == name)
                .unwrap_or(false)
        })
        .map(|(idx, s)| ResumeMatch {
            session_id: s.id.clone(),
            original_index: idx,
            display_name: name.to_string(),
            source_type: s.source_type, // Get from session in iterator
        })
        .ok_or_else(|| KsmError::SessionNotFound(format!("No session with name '{}'", name)))
}
