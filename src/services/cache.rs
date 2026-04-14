//! Session cache service.
//!
//! Provides cached session data for list display and chain detection.
//! Automatically refreshes stale entries based on Kiro's updated_at timestamp.
//! Skips ACP file scanning when directory mtime unchanged.

use log::debug;
use std::collections::{HashMap, HashSet};

use crate::data::{KsmDatabase, SessionSource};
use crate::error::Result;
use crate::models::CachedSession;

/// Get all cached session data for a directory.
///
/// Automatically refreshes stale entries (where Kiro's updated_at changed).
/// Skips ACP file scanning when ACP directory mtime is unchanged.
pub fn sessions(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    directory: &str,
) -> Result<HashMap<String, CachedSession>> {
    debug!("Loading session cache for directory: {}", directory);

    // 1. Get v1 updates (always fast, SQLite index scan)
    let v1_updates = source.list_session_updates_v1()?;
    debug!("Found {} v1 sessions", v1_updates.len());

    // 2. Get v2 (ACP) session IDs - conditional on mtime
    let acp_ids = get_acp_ids_cached(source, db, directory)?;
    debug!("Found {} ACP sessions", acp_ids.len());

    // 3. Load existing cache from ksm.db
    let mut cache = db.get_cached_sessions(directory)?;
    debug!("Loaded {} cached entries from ksm.db", cache.len());

    // 4. Build combined update list
    let mut updates: Vec<(String, i64)> = v1_updates;

    // For ACP sessions, use cached timestamp or 0 to force cache miss.
    // Timestamp 0 ensures new ACP sessions (not yet in cache) trigger a full
    // conversation fetch, since no valid session has updated_at == 0.
    for acp_id in &acp_ids {
        let ts = cache.get(acp_id).map(|c| c.updated_at).unwrap_or(0);
        updates.push((acp_id.clone(), ts));
    }

    if updates.is_empty() {
        debug!("No sessions found");
        return Ok(HashMap::new());
    }

    // 5. Find stale/missing entries
    let mut to_update = Vec::new();
    let mut hits = 0;
    let mut misses = 0;

    for (session_id, updated_at) in &updates {
        if let Some(cached) = cache.get(session_id)
            && cached.updated_at == *updated_at
            && *updated_at != 0
        {
            hits += 1;
            continue;
        }

        // Cache miss or stale
        misses += 1;
        debug!("Cache miss: session {}", session_id);

        let (conversation, created_at) = source.get_conversation_with_created_at(session_id)?;
        let (_, actual_updated_at) = source.get_timestamps(session_id)?;
        let preview = conversation.preview();
        let msg_count = conversation.history.len() as u32;
        let has_compact_tag = extract_has_compact_tag(&conversation);
        let message_ids = extract_message_ids(&conversation);

        let cached = CachedSession {
            session_id: session_id.clone(),
            directory: directory.to_string(),
            updated_at: actual_updated_at,
            created_at,
            preview,
            msg_count,
            has_compact_tag,
            message_ids,
            source_type: source.session_source_type(session_id),
        };

        cache.insert(session_id.clone(), cached.clone());
        to_update.push(cached);
    }

    debug!("Cache: {} hits, {} misses", hits, misses);

    // 6. Write updates to ksm.db
    if !to_update.is_empty() {
        db.set_cached_sessions(&to_update)?;
        debug!("Updated {} stale cache entries", to_update.len());
    }

    // 7. Filter to only sessions that exist
    let live_ids: HashSet<_> = updates.iter().map(|(id, _)| id.clone()).collect();
    cache.retain(|id, _| live_ids.contains(id));

    Ok(cache)
}

/// Get ACP session IDs, using cached list if mtime unchanged.
fn get_acp_ids_cached(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    directory: &str,
) -> Result<Vec<String>> {
    let Some(current_mtime) = source.acp_dir_mtime() else {
        return Ok(Vec::new());
    };

    if let Some((cached_mtime, cached_ids)) = db.get_acp_cache(directory)?
        && cached_mtime == current_mtime
    {
        debug!("ACP cache hit (mtime unchanged)");
        return Ok(cached_ids);
    }

    debug!("ACP cache miss (scanning directory)");
    let ids = source.list_acp_session_ids()?;
    db.set_acp_cache(directory, current_mtime, &ids)?;
    Ok(ids)
}

/// Remove cache entries for sessions that no longer exist.
///
/// Called under auto_clean config setting.
pub fn clean_stale(
    db: &KsmDatabase,
    directory: &str,
    live_session_ids: &HashSet<String>,
) -> Result<usize> {
    let deleted = db.delete_stale_cache(directory, live_session_ids)?;
    if deleted > 0 {
        debug!("Cleaned {} stale cache entries", deleted);
    }
    Ok(deleted)
}

/// Extract has_compact_tag from conversation data.
fn extract_has_compact_tag(conversation: &crate::models::ConversationData) -> bool {
    if let Some(summary) = &conversation.latest_summary
        && summary.len() > 1
        && let Some(tags) = summary[1].get("message_meta_tags")
        && let Some(tags_arr) = tags.as_array()
    {
        return tags_arr.iter().any(|t| t.as_str() == Some("Compact"));
    }
    false
}

/// Extract message_ids from conversation data.
fn extract_message_ids(conversation: &crate::models::ConversationData) -> Vec<String> {
    let mut ids = Vec::new();
    for entry in &conversation.history {
        if let Some(metadata) = &entry.request_metadata
            && let Some(msg_id) = &metadata.message_id
        {
            ids.push(msg_id.clone());
        }
    }
    ids
}
