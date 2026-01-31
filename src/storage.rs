use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use crate::config::load_config;
use crate::models::{Session, SessionMetadata};

pub fn metadata_path() -> Result<PathBuf> {
    let config = load_config()?;
    
    match config.metadata_storage.as_str() {
        "global" => {
            let home = std::env::var("HOME").context("HOME environment variable not set")?;
            let ksm_dir = PathBuf::from(home).join(".ksm");
            fs::create_dir_all(&ksm_dir)?;
            Ok(ksm_dir.join("metadata.json"))
        }
        "local" => {
            let cwd = std::env::current_dir().context("Failed to get current directory")?;
            let kiro_dir = cwd.join(".kiro");
            fs::create_dir_all(&kiro_dir)?;
            Ok(kiro_dir.join("ksm-metadata.json"))
        }
        "custom" => {
            let custom = config.custom_path
                .context("custom_path not set in config when metadata_storage is 'custom'")?;
            let path = PathBuf::from(custom);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            Ok(path)
        }
        _ => anyhow::bail!("Invalid metadata_storage option: {}", config.metadata_storage),
    }
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
    let current_dir = std::env::current_dir()
        .context("Failed to get current directory")?
        .to_string_lossy()
        .to_string();
    
    let session_ids: HashSet<_> = sessions.iter().map(|s| s.id.as_str()).collect();
    let stale_ids: Vec<_> = metadata.iter()
        .filter(|(id, meta)| {
            // Only consider entries from current directory
            if let Some(dir) = &meta.directory {
                dir == &current_dir && !session_ids.contains(id.as_str())
            } else {
                // Legacy entries without directory - clean if not in current sessions
                !session_ids.contains(id.as_str())
            }
        })
        .map(|(id, _)| id.clone())
        .collect();
    
    if !stale_ids.is_empty() {
        for id in stale_ids {
            metadata.remove(&id);
        }
        save_metadata(metadata)?;
    }
    
    Ok(())
}
