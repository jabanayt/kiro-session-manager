//! Session cache service.
//!
//! Provides cached session data for list display and chain detection.
//! Automatically refreshes stale entries based on Kiro's updated_at timestamp.

use log::debug;
use std::collections::{HashMap, HashSet};

use crate::data::{KsmDatabase, SessionSource};
use crate::error::Result;
use crate::models::CachedSession;

/// Get all cached session data for a directory.
///
/// Automatically refreshes stale entries (where Kiro's updated_at changed).
/// Returns HashMap keyed by session_id for fast lookups.
pub fn sessions(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    directory: &str,
) -> Result<HashMap<String, CachedSession>> {
    debug!("Loading session cache for directory: {}", directory);

    // 1. Get timestamps from Kiro DB (cheap query)
    let timestamps = source.list_session_timestamps()?;
    if timestamps.is_empty() {
        debug!("No sessions found in Kiro DB");
        return Ok(HashMap::new());
    }
    debug!("Found {} sessions in Kiro DB", timestamps.len());

    // 2. Load existing cache from ksm.db
    let mut cache = db.get_cached_sessions(directory)?;
    debug!("Loaded {} cached entries from ksm.db", cache.len());

    // 3. Find stale/missing entries
    let mut to_update = Vec::new();
    let mut hits = 0;
    let mut misses = 0;

    for (session_id, created_at, updated_at) in &timestamps {
        if let Some(cached) = cache.get(session_id)
            && cached.updated_at == *updated_at
        {
            hits += 1;
            continue; // Cache hit
        }

        // Cache miss or stale - fetch full data
        misses += 1;
        debug!("Cache miss: session {} (fetching from Kiro)", session_id);

        let conversation = source.get_conversation(session_id)?;
        let preview = conversation.preview();
        let msg_count = conversation.history.len() as u32;
        let has_compact_tag = extract_has_compact_tag(&conversation);
        let message_ids = extract_message_ids(&conversation);

        let cached = CachedSession {
            session_id: session_id.clone(),
            directory: directory.to_string(),
            updated_at: *updated_at,
            created_at: *created_at,
            preview,
            msg_count,
            has_compact_tag,
            message_ids,
        };

        cache.insert(session_id.clone(), cached.clone());
        to_update.push(cached);
    }

    debug!("Cache: {} hits, {} misses", hits, misses);

    // 4. Write updates to ksm.db
    if !to_update.is_empty() {
        db.set_cached_sessions(&to_update)?;
        debug!("Updated {} stale cache entries", to_update.len());
    }

    // 5. Filter to only sessions that exist in Kiro
    let live_ids: HashSet<_> = timestamps.iter().map(|(id, _, _)| id.clone()).collect();
    cache.retain(|id, _| live_ids.contains(id));

    Ok(cache)
}

/// Remove cache entries for sessions that no longer exist in Kiro.
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
