use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_metadata_storage")]
    pub metadata_storage: String,
    pub custom_path: Option<String>,
    #[serde(default = "default_auto_detect")]
    pub auto_detect_continuations: bool,
}

fn default_metadata_storage() -> String {
    "global".to_string()
}

fn default_auto_detect() -> bool {
    false
}

impl Default for Config {
    fn default() -> Self {
        Config {
            metadata_storage: default_metadata_storage(),
            custom_path: None,
            auto_detect_continuations: default_auto_detect(),
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

# Auto-detect compacted sessions and suggest linking them to parents
# Set to true to enable automatic detection on 'ksm list'
# Only sessions with Kiro's Compact tag will be auto-linked
auto_detect_continuations = false
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
    let mut config: Config = toml::from_str(&content)
        .context("Failed to parse config.toml")?;
    
    // Check if config needs updating (missing fields will use defaults from serde)
    // Re-save to ensure all fields are present
    let default = Config::default();
    let mut needs_update = false;
    
    // Check if auto_detect_continuations is missing from file
    if !content.contains("auto_detect_continuations") {
        config.auto_detect_continuations = default.auto_detect_continuations;
        needs_update = true;
    }
    
    if needs_update {
        // Re-write config with all fields
        let updated_content = format!(
            r#"# Metadata storage location
# Options: "global", "local", "custom"
# - global: ~/.ksm/metadata.json (shared across all projects)
# - local: .kiro/ksm-metadata.json (per-directory, stored with sessions)
# - custom: Use custom_path below
metadata_storage = "{}"

{}
# Auto-detect compacted sessions and suggest linking them to parents
# Set to true to enable automatic detection on 'ksm list'
# Only sessions with Kiro's Compact tag will be auto-linked
auto_detect_continuations = {}
"#,
            config.metadata_storage,
            config.custom_path.as_ref().map(|p| format!("custom_path = \"{}\"\n", p)).unwrap_or_else(|| "# custom_path = \"/path/to/metadata.json\"\n".to_string()),
            config.auto_detect_continuations
        );
        fs::write(&path, updated_content)?;
    }
    
    Ok(config)
}
