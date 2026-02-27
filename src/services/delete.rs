use std::collections::HashMap;

use crate::data::{MetadataStore, SessionSource};
use crate::error::Result;
use crate::models::{Session, SessionMetadata};
use crate::services::chains;

/// Result of a delete operation.
pub struct DeleteResult {
    /// Session IDs that were deleted.
    pub deleted_ids: Vec<String>,
    /// Relinks performed: (child_id, new_parent_id or "none").
    pub relinked: Vec<(String, Option<String>)>,
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
    store: &dyn MetadataStore,
) -> Result<DeleteResult> {
    match choice {
        ChainDeleteChoice::SingleRelink => {
            let relinked_info = chains::relink_around_session(session_id, metadata);
            store.save(metadata)?;
            source.delete_session(session_id)?;
            metadata.remove(session_id);
            store.save(metadata)?;

            Ok(DeleteResult {
                deleted_ids: vec![session_id.to_string()],
                relinked: relinked_info.into_iter().collect(),
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

            for id in &to_delete {
                source.delete_session(id)?;
                metadata.remove(id);
            }
            store.save(metadata)?;

            Ok(DeleteResult {
                deleted_ids: to_delete,
                relinked: vec![],
            })
        }
        ChainDeleteChoice::EntireChain => {
            let chain = chains::get_full_chain(session_id, metadata, sessions);
            for id in &chain {
                source.delete_session(id)?;
                metadata.remove(id);
            }
            store.save(metadata)?;

            Ok(DeleteResult {
                deleted_ids: chain,
                relinked: vec![],
            })
        }
    }
}

/// Delete multiple sessions (standard, non-chain deletion).
pub fn delete_sessions(
    session_ids: &[String],
    source: &dyn SessionSource,
    metadata: &mut HashMap<String, SessionMetadata>,
    store: &dyn MetadataStore,
) -> Result<DeleteResult> {
    for id in session_ids {
        source.delete_session(id)?;
        metadata.remove(id);
    }
    store.save(metadata)?;

    Ok(DeleteResult {
        deleted_ids: session_ids.to_vec(),
        relinked: vec![],
    })
}
