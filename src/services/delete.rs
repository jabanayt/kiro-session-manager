//! Session deletion with chain handling.

use std::collections::HashMap;

use crate::data::{KsmDatabase, SessionSource};
use crate::error::Result;
use crate::models::{ArchiveStatus, Session, SessionMetadata, SourceType};
use crate::services::chains;

/// Result of a delete operation.
pub struct DeleteResult {
    /// Session IDs that were deleted.
    pub deleted_ids: Vec<String>,
    /// Relinks performed: (child_id, new_parent_id or "none").
    pub relinked: Vec<(String, Option<String>)>,
    pub indexed_count: usize,
}

/// How to handle chain deletion when a session is part of a chain.
pub enum ChainDeleteChoice {
    /// Delete only this session, relink child to grandparent.
    SingleRelink,
    /// Delete this session and all its parents.
    WithParents,
    /// Delete the entire chain.
    EntireChain,
}

/// Delete a single session that is part of a chain.
pub fn delete_from_chain(
    session_id: &str,
    choice: ChainDeleteChoice,
    sessions: &[Session],
    metadata: &mut HashMap<String, SessionMetadata>,
    source: &dyn SessionSource,
    db: &KsmDatabase,
    source_type: SourceType,
) -> Result<DeleteResult> {
    match choice {
        ChainDeleteChoice::SingleRelink => {
            let relinked_info = chains::relink_around_session(session_id, metadata);

            // Save relinked child's updated metadata
            if let Some((child_id, _)) = &relinked_info
                && let Some(child_meta) = metadata.get(child_id)
            {
                db.set_metadata(child_id, child_meta)?;
            }

            // Clear index if session is indexed before deleting
            let mut indexed_count = 0;
            if let Some(ArchiveStatus::Indexed { archive_id, .. }) =
                db.get_archive_status(session_id)?
            {
                db.set_indexed(archive_id, false)?;
                indexed_count = 1;
            }

            source.delete_session(session_id, source_type)?;
            metadata.remove(session_id);
            db.delete_metadata(session_id)?;

            Ok(DeleteResult {
                deleted_ids: vec![session_id.to_string()],
                relinked: relinked_info.into_iter().collect(),
                indexed_count,
            })
        }
        ChainDeleteChoice::WithParents => {
            let mut to_delete = vec![session_id.to_string()];
            let mut current = session_id.to_string();
            while let Some(meta) = metadata.get(&current) {
                if let Some(parent_id) = &meta.parent_session_id {
                    to_delete.push(parent_id.clone());
                    current = parent_id.clone();
                } else {
                    break;
                }
            }

            let mut indexed_count = 0;
            for id in &to_delete {
                // Clear index if session is indexed before deleting
                if let Some(ArchiveStatus::Indexed { archive_id, .. }) =
                    db.get_archive_status(id)?
                {
                    db.set_indexed(archive_id, false)?;
                    indexed_count += 1;
                }
                source.delete_session(id, source_type)?;
                metadata.remove(id);
                db.delete_metadata(id)?;
            }

            Ok(DeleteResult {
                deleted_ids: to_delete,
                relinked: vec![],
                indexed_count,
            })
        }
        ChainDeleteChoice::EntireChain => {
            let chain = chains::get_full_chain(session_id, metadata, sessions);
            let mut indexed_count = 0;
            for id in &chain {
                // Clear index if session is indexed before deleting
                if let Some(ArchiveStatus::Indexed { archive_id, .. }) =
                    db.get_archive_status(id)?
                {
                    db.set_indexed(archive_id, false)?;
                    indexed_count += 1;
                }
                source.delete_session(id, source_type)?;
                metadata.remove(id);
                db.delete_metadata(id)?;
            }

            Ok(DeleteResult {
                deleted_ids: chain,
                relinked: vec![],
                indexed_count,
            })
        }
    }
}

/// Delete multiple sessions (standard, non-chain deletion).
pub fn delete_sessions(
    sessions: &[(String, SourceType)],
    source: &dyn SessionSource,
    metadata: &mut HashMap<String, SessionMetadata>,
    db: &KsmDatabase,
) -> Result<DeleteResult> {
    let mut indexed_count = 0;

    for (id, source_type) in sessions {
        // Clear index if session is indexed before deleting
        if let Some(ArchiveStatus::Indexed { archive_id, .. }) =
            db.get_archive_status_for_source(id, *source_type)?
        {
            db.set_indexed(archive_id, false)?;
            indexed_count += 1;
        }
        source.delete_session(id, *source_type)?;
        metadata.remove(id);
        db.delete_metadata(id)?;
    }

    Ok(DeleteResult {
        deleted_ids: sessions.iter().map(|(id, _)| id.clone()).collect(),
        relinked: vec![],
        indexed_count,
    })
}
