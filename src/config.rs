use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_metadata_storage")]
    pub metadata_storage: String,
    pub custom_path: Option<String>,
}

fn default_metadata_storage() -> String {
    "global".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            metadata_storage: default_metadata_storage(),
            custom_path: None,
        }
    }
}

fn config_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let ksm_dir = PathBuf::from(home).join(".ksm");
    fs::create_dir_all(&ksm_dir)?;
    Ok(ksm_dir.join("config.toml"))
}

fn create_default_config() -> Result<Config> {
    let config = Config::default();
    let path = config_path()?;
    
    let content = r#"# Metadata storage location
# Options: "global", "local", "custom"
# - global: ~/.ksm/metadata.json (shared across all projects)
# - local: .kiro/ksm-metadata.json (per-directory, stored with sessions)
# - custom: Use custom_path below
metadata_storage = "global"

# Custom metadata path (only used when metadata_storage = "custom")
# custom_path = "/path/to/metadata.json"
"#;
    
    fs::write(&path, content)?;
    Ok(config)
}

pub fn load_config() -> Result<Config> {
    let path = config_path()?;
    
    if !path.exists() {
        return create_default_config();
    }
    
    let content = fs::read_to_string(&path)?;
    let config: Config = toml::from_str(&content)
        .context("Failed to parse config.toml")?;
    
    Ok(config)
}
