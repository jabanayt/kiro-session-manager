//! Legacy JSON metadata store.
//!
//! DEPRECATED: This module exists only for migration from older versions.
//! New code should use KsmDatabase directly.
//!
//! TODO(v0.3.0): Remove this module and JsonMetadataStore.

use log::debug;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::config::metadata_path;
use crate::data::MetadataStore;
use crate::error::Result;
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
