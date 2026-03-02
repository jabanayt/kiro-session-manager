use log::debug;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::config::load_config;
use crate::data::MetadataStore;
use crate::error::{KsmError, Result};
use crate::models::SessionMetadata;

/// Metadata store backed by a JSON file.
///
/// Supports global (`~/.ksm/metadata.json`), local (`.kiro/ksm-metadata.json`),
/// and custom paths via config.
pub struct JsonMetadataStore {
    path: PathBuf,
}

impl JsonMetadataStore {
    /// Create store using path from config.
    pub fn from_config() -> Result<Self> {
        let path = metadata_path()?;
        Ok(JsonMetadataStore { path })
    }

    /// Create store with explicit path (for testing).
    pub fn new(path: PathBuf) -> Self {
        JsonMetadataStore { path }
    }
}

/// Resolve metadata file path from config.
fn metadata_path() -> Result<PathBuf> {
    let config = load_config()?;

    match config.metadata_storage.as_str() {
        "global" => {
            let home = std::env::var("HOME")
                .map_err(|_| KsmError::Config("HOME environment variable not set".to_string()))?;
            let ksm_dir = PathBuf::from(home).join(".ksm");
            fs::create_dir_all(&ksm_dir)?;
            Ok(ksm_dir.join("metadata.json"))
        }
        "local" => {
            let cwd = std::env::current_dir()?;
            let kiro_dir = cwd.join(".kiro");
            fs::create_dir_all(&kiro_dir)?;
            Ok(kiro_dir.join("ksm-metadata.json"))
        }
        "custom" => {
            let custom = config.custom_path.ok_or_else(|| {
                KsmError::Config(
                    "custom_path not set in config when metadata_storage is 'custom'".to_string(),
                )
            })?;
            let path = PathBuf::from(custom);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            Ok(path)
        }
        other => Err(KsmError::Config(format!(
            "Invalid metadata_storage option: {}",
            other
        ))),
    }
}

impl MetadataStore for JsonMetadataStore {
    fn load(&self) -> Result<HashMap<String, SessionMetadata>> {
        if !self.path.exists() {
            return Ok(HashMap::new());
        }
        let content = fs::read_to_string(&self.path)?;
        let metadata = serde_json::from_str(&content)?;
        debug!("Loaded metadata from {}", self.path.display());
        Ok(metadata)
    }

    fn save(&self, metadata: &HashMap<String, SessionMetadata>) -> Result<()> {
        let content = serde_json::to_string_pretty(metadata)?;
        fs::write(&self.path, content)?;
        debug!("Saved metadata to {}", self.path.display());
        Ok(())
    }
}
