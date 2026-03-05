use std::collections::HashMap;

use crate::data::{KsmDatabase, SessionSource};
use crate::error::{KsmError, Result};
use crate::models::{ArchiveStatus, Session, SessionMetadata};
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
    /// Session found, timestamp updated, ready to launch kiro-cli.
    Ready {
        session_id: String,
        display_name: String,
    },
    /// Multiple sessions match a tag -- caller must pick one and retry with Index.
    MultipleMatches {
        tag: String,
        matches: Vec<ResumeMatch>,
    },
    /// Resume last -- no timestamp manipulation needed, just launch kiro-cli.
    LaunchDirect,
}

/// A single match when resolving by tag (used in MultipleMatches).
pub struct ResumeMatch {
    pub session_id: String,
    pub original_index: usize,
    pub display_name: String,
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
        ResumeTarget::Last => Ok(ResumeResult::LaunchDirect),

        ResumeTarget::Index(index) => {
            let list_result = sessions::list_sessions(source, db, directory)?;
            sessions::validate_index(index, list_result.all_sessions.len())?;
            let session = &list_result.all_sessions[index];
            let display_name = list_result
                .metadata
                .get(&session.id)
                .and_then(|m| m.name.clone())
                .unwrap_or_else(|| session.preview.clone());

            // Set pending reindex if session is indexed
            set_pending_reindex_if_indexed(&session.id, db)?;

            prepare_resume(&session.id, source)?;
            Ok(ResumeResult::Ready {
                session_id: session.id.clone(),
                display_name,
            })
        }

        ResumeTarget::Name(name) => {
            let list_result = sessions::list_sessions(source, db, directory)?;
            let found = find_by_name(&name, &list_result.all_sessions, &list_result.metadata)?;

            set_pending_reindex_if_indexed(&found.session_id, db)?;

            prepare_resume(&found.session_id, source)?;
            Ok(ResumeResult::Ready {
                session_id: found.session_id,
                display_name: found.display_name,
            })
        }

        ResumeTarget::Tag(tag) => {
            let list_result = sessions::list_sessions(source, db, directory)?;
            let matches = find_by_tag(&tag, &list_result.all_sessions, &list_result.metadata);
            match matches.len() {
                0 => Err(KsmError::SessionNotFound(format!(
                    "No sessions found with tag '{}'",
                    tag
                ))),
                1 => {
                    set_pending_reindex_if_indexed(&matches[0].session_id, db)?;

                    prepare_resume(&matches[0].session_id, source)?;
                    Ok(ResumeResult::Ready {
                        session_id: matches[0].session_id.clone(),
                        display_name: matches[0].display_name.clone(),
                    })
                }
                _ => Ok(ResumeResult::MultipleMatches { tag, matches }),
            }
        }
    }
}

/// Set pending_reindex if the session is indexed.
fn set_pending_reindex_if_indexed(session_id: &str, db: &KsmDatabase) -> Result<()> {
    if let Some(ArchiveStatus::Indexed { .. }) = db.get_archive_status(session_id)? {
        db.set_pending_reindex(session_id)?;
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
                .map(|m| m.tags.contains(tag))
                .unwrap_or(false)
        })
        .map(|(idx, s)| ResumeMatch {
            session_id: s.id.clone(),
            original_index: idx,
            display_name: metadata
                .get(&s.id)
                .and_then(|m| m.name.clone())
                .unwrap_or_else(|| s.preview.clone()),
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
        })
        .ok_or_else(|| KsmError::SessionNotFound(format!("No session with name '{}'", name)))
}

/// Update a session's timestamp to now so kiro-cli's --resume picks it up.
fn prepare_resume(session_id: &str, source: &dyn SessionSource) -> Result<()> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    source.update_timestamp(session_id, timestamp)
}
