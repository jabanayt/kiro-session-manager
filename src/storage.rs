use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use crate::models::{Session, SessionMetadata};

pub fn metadata_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let ksm_dir = PathBuf::from(home).join(".ksm");
    fs::create_dir_all(&ksm_dir)?;
    Ok(ksm_dir.join("metadata.json"))
}

pub fn load_metadata() -> Result<HashMap<String, SessionMetadata>> {
    let path = metadata_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = fs::read_to_string(&path)?;
    let metadata = serde_json::from_str(&content)?;
    Ok(metadata)
}

pub fn save_metadata(metadata: &HashMap<String, SessionMetadata>) -> Result<()> {
    let path = metadata_path()?;
    let content = serde_json::to_string_pretty(metadata)?;
    fs::write(&path, content)?;
    Ok(())
}

pub fn cleanup_stale_metadata(metadata: &mut HashMap<String, SessionMetadata>, sessions: &[Session]) -> Result<()> {
    let session_ids: HashSet<_> = sessions.iter().map(|s| s.id.as_str()).collect();
    let stale_ids: Vec<_> = metadata.keys()
        .filter(|id| !session_ids.contains(id.as_str()))
        .cloned()
        .collect();
    
    if !stale_ids.is_empty() {
        for id in stale_ids {
            metadata.remove(&id);
        }
        save_metadata(metadata)?;
    }
    
    Ok(())
}
