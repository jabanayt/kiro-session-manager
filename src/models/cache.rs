use crate::models::Session;
use serde::{Deserialize, Serialize};

/// Cached session data for fast list and chain detection operations.
///
/// Stored in ksm.db session_cache table. Refreshed when Kiro's
/// updated_at timestamp changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSession {
    pub session_id: String,
    pub directory: String,
    pub updated_at: i64,
    pub created_at: i64,
    pub preview: String,
    pub msg_count: u32,
    pub has_compact_tag: bool,
    pub message_ids: Vec<String>,
}

impl From<&CachedSession> for Session {
    fn from(c: &CachedSession) -> Self {
        Session {
            id: c.session_id.clone(),
            created_at: c.created_at,
            updated_at: c.updated_at,
            preview: c.preview.clone(),
            msg_count: c.msg_count,
        }
    }
}
