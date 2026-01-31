use std::collections::HashSet;

#[derive(Debug)]
pub struct Session {
    pub id: String,
    pub time_ago: String,
    pub preview: String,
    pub msg_count: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct SessionMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "HashSet::is_empty", default)]
    pub tags: HashSet<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
}
