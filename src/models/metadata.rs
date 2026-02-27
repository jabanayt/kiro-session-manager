use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Metadata attached to a session (name, tags, chain links).
///
/// Stored in the metadata store (currently JSON, future SQLite).
/// The `directory` field enables per-project isolation in global storage mode.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "HashSet::is_empty", default)]
    pub tags: HashSet<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(default)]
    pub manually_unlinked: bool,
}
