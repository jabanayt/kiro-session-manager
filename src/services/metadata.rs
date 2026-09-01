//! Metadata operations: name, tags, cleaning.

use std::collections::{HashMap, HashSet};

use crate::data::KsmDatabase;
use crate::error::{KsmError, Result};
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

/// Human-readable description of tag format rules.
pub const TAG_RULES: &str = "Tags must be lowercase a-z, 0-9, hyphens, underscores, and dots only.";

/// Validate a single tag name. Returns the normalised (lowercased) tag or an error.
pub fn validate_tag(tag: &str) -> Result<String> {
    let tag = tag.trim().to_lowercase();
    if tag.is_empty() {
        return Err(KsmError::InvalidTag("Tag cannot be empty".into()));
    }
    if tag.len() > 50 {
        return Err(KsmError::InvalidTag(
            "Tag cannot exceed 50 characters".into(),
        ));
    }
    if !tag
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(KsmError::InvalidTag(
            "Tags can only contain lowercase letters, numbers, hyphens, underscores, and dots"
                .into(),
        ));
    }
    Ok(tag)
}

/// Validate and normalise a vec of tags.
pub fn validate_tags(tags: &[String]) -> Result<Vec<String>> {
    tags.iter().map(|t| validate_tag(t)).collect()
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
    let validated = validate_tags(tags)?;
    let target_ids = resolve_scope(&scope, metadata, sessions);
    let current_dir = std::env::current_dir()?.to_string_lossy().to_string();

    for session_id in &target_ids {
        let entry = metadata.entry(session_id.clone()).or_default();
        entry.directory = Some(current_dir.clone());
        for tag in &validated {
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

    // Verify tags can be found before removing anything
    // Single scope with no metadata is caught here (found = false)
    for tag in tags {
        let found = target_ids.iter().any(|id| {
            metadata
                .get(id)
                .is_some_and(|entry| entry.tags.contains(tag))
        });
        if !found {
            let scope_desc = if target_ids.len() == 1 {
                "on session".to_string()
            } else {
                format!("on any session in chain ({} sessions)", target_ids.len())
            };
            return Err(KsmError::TagNotFound(format!(
                "\"{}\" not found {}",
                tag, scope_desc
            )));
        }
    }

    // Remove tags, tracking which sessions were actually modified
    let mut affected_ids = Vec::new();
    for session_id in &target_ids {
        if let Some(entry) = metadata.get_mut(session_id) {
            let mut modified = false;
            for tag in tags {
                if entry.tags.remove(tag) {
                    modified = true;
                }
            }
            if modified {
                entry.directory = Some(current_dir.clone());
                db.set_metadata(session_id, entry)?;
                affected_ids.push(session_id.clone());
            }
        }
    }

    Ok(MetadataUpdateResult { affected_ids })
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn test_validate_tag_normalizes_case() {
        assert_eq!(validate_tag("UPPER").unwrap(), "upper");
        assert_eq!(validate_tag("MixedCase").unwrap(), "mixedcase");
    }

    #[test]
    fn test_validate_tag_trims_whitespace() {
        assert_eq!(validate_tag("  tag  ").unwrap(), "tag");
        assert_eq!(validate_tag("\ttab\t").unwrap(), "tab");
    }

    #[test]
    fn test_validate_tag_allows_valid_chars() {
        assert!(validate_tag("simple").is_ok());
        assert!(validate_tag("with-hyphen").is_ok());
        assert!(validate_tag("with_underscore").is_ok());
        assert!(validate_tag("with.dot").is_ok());
        assert!(validate_tag("with123numbers").is_ok());
    }

    #[test]
    fn test_validate_tag_rejects_empty() {
        use assert_matches::assert_matches;

        assert_matches!(validate_tag(""), Err(KsmError::InvalidTag(_)));
        assert_matches!(validate_tag("   "), Err(KsmError::InvalidTag(_)));
    }

    #[test]
    fn test_validate_tag_rejects_too_long() {
        use assert_matches::assert_matches;

        let long_tag = "a".repeat(51);
        assert_matches!(validate_tag(&long_tag), Err(KsmError::InvalidTag(_)));

        let ok_tag = "a".repeat(50);
        assert!(validate_tag(&ok_tag).is_ok());
    }

    #[test]
    fn test_validate_tag_rejects_special_chars() {
        use assert_matches::assert_matches;

        assert_matches!(validate_tag("has space"), Err(KsmError::InvalidTag(_)));
        assert_matches!(validate_tag("has@symbol"), Err(KsmError::InvalidTag(_)));
        assert_matches!(validate_tag("has/slash"), Err(KsmError::InvalidTag(_)));
        assert_matches!(validate_tag("has:colon"), Err(KsmError::InvalidTag(_)));
    }

    #[test]
    fn test_validate_tags_all_valid() {
        let tags = vec!["tag1".to_string(), "tag2".to_string()];
        let result = validate_tags(&tags).unwrap();
        assert_eq!(result, vec!["tag1", "tag2"]);
    }

    #[test]
    fn test_validate_tags_one_invalid() {
        let tags = vec!["valid".to_string(), "in valid".to_string()];
        assert!(validate_tags(&tags).is_err());
    }

    #[test]
    fn test_resolve_scope_single() {
        use crate::models::{Session, SessionMetadata};
        use std::collections::HashMap;

        let metadata: HashMap<String, SessionMetadata> = HashMap::new();
        let sessions: Vec<Session> = vec![];
        let scope = MetadataScope::Single("session-1".to_string());

        let result = resolve_scope(&scope, &metadata, &sessions);
        assert_eq!(result, vec!["session-1"]);
    }
}
