use log::debug;
use std::fs;
use std::path::PathBuf;

use crate::error::{KsmError, Result};

/// Application configuration loaded from `~/.ksm/config.toml`.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Config {
    #[serde(default = "default_metadata_storage")]
    pub metadata_storage: String,
    pub custom_path: Option<String>,
    #[serde(default = "default_auto_detect")]
    pub auto_detect_continuations: bool,
    #[serde(default = "default_auto_clean")]
    pub auto_clean: bool,
    #[serde(default)]
    pub index: IndexConfig,
}

fn default_metadata_storage() -> String {
    "global".to_string()
}

fn default_auto_detect() -> bool {
    false
}

fn default_auto_clean() -> bool {
    true
}

fn default_auto_update() -> bool {
    true
}

/// Index-related configuration.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IndexConfig {
    /// Automatically update indexed sessions when resumed.
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self { auto_update: true }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            metadata_storage: default_metadata_storage(),
            custom_path: None,
            auto_detect_continuations: default_auto_detect(),
            auto_clean: default_auto_clean(),
            index: IndexConfig::default(),
        }
    }
}

/// Returns path to `~/.ksm/config.toml`, creating `~/.ksm/` if needed.
pub fn config_path() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| KsmError::Config("HOME environment variable not set".to_string()))?;
    let ksm_dir = PathBuf::from(home).join(".ksm");
    fs::create_dir_all(&ksm_dir)?;
    Ok(ksm_dir.join("config.toml"))
}

/// Creates default config file and returns the Config.
fn create_default_config() -> Result<Config> {
    let config = Config::default();
    let path = config_path()?;

    let content = r#"# KSM Configuration
# Values shown in comments are defaults. Uncomment to override.

# metadata_storage = "global"
# custom_path = "/path/to/ksm.db"

# auto_detect_continuations = false
# auto_clean = true

[index]
# auto_update = true
"#;

    fs::write(&path, content)?;
    debug!("Created default config at {}", path.display());
    Ok(config)
}

/// Resolve the path to `metadata.json` based on storage configuration.
///
/// Follows the same storage mode logic:
/// - global: `~/.ksm/metadata.json`
/// - local: `.kiro/ksm-metadata.json`
/// - custom: `{custom_path}` (the path itself)
///
/// Shared by `JsonMetadataStore` and `SqliteMetadataStore::migrate_from_json`.
pub fn metadata_path() -> Result<PathBuf> {
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

/// Resolve the path to `ksm.db` based on storage configuration.
///
/// Follows the same storage mode logic as metadata:
/// - global: `~/.ksm/ksm.db`
/// - local: `.kiro/ksm.db`
/// - custom: `{custom_path_parent}/ksm.db`
pub fn ksm_db_path() -> Result<PathBuf> {
    let config = load_config()?;

    match config.metadata_storage.as_str() {
        "global" => {
            let home = std::env::var("HOME")
                .map_err(|_| KsmError::Config("HOME environment variable not set".to_string()))?;
            let ksm_dir = PathBuf::from(home).join(".ksm");
            fs::create_dir_all(&ksm_dir)?;
            Ok(ksm_dir.join("ksm.db"))
        }
        "local" => {
            let cwd = std::env::current_dir()?;
            let kiro_dir = cwd.join(".kiro");
            fs::create_dir_all(&kiro_dir)?;
            Ok(kiro_dir.join("ksm.db"))
        }
        "custom" => {
            let custom = config.custom_path.ok_or_else(|| {
                KsmError::Config(
                    "custom_path not set in config when metadata_storage is 'custom'".to_string(),
                )
            })?;
            let path = PathBuf::from(custom);
            let parent = path.parent().ok_or_else(|| {
                KsmError::Config(format!(
                    "Cannot determine parent directory for custom path: {}",
                    path.display()
                ))
            })?;
            fs::create_dir_all(parent)?;
            Ok(parent.join("ksm.db"))
        }
        other => Err(KsmError::Config(format!(
            "Invalid metadata_storage option: {}",
            other
        ))),
    }
}

/// Loads configuration from `~/.ksm/config.toml`.
///
/// Creates default config if file doesn't exist.
/// Auto-migrates missing fields on existing configs.
pub fn load_config() -> Result<Config> {
    let path = config_path()?;

    if !path.exists() {
        return create_default_config();
    }

    let content = fs::read_to_string(&path)?;
    let mut config: Config = toml::from_str(&content)?;

    // Check if config needs updating (missing fields will use defaults from serde)
    // Re-save to ensure all fields are present
    let default = Config::default();
    let mut needs_update = false;

    // Check if auto_detect_continuations is missing from file
    if !content.contains("auto_detect_continuations") {
        config.auto_detect_continuations = default.auto_detect_continuations;
        needs_update = true;
    }

    // Check if auto_clean is missing from file
    if !content.contains("auto_clean") {
        config.auto_clean = default.auto_clean;
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

# Automatically clean stale metadata entries on list/resume
# Set to false to disable (prevents metadata loss if database fails)
auto_clean = {}
"#,
            config.metadata_storage,
            config
                .custom_path
                .as_ref()
                .map(|p| format!("custom_path = \"{}\"\n", p))
                .unwrap_or_else(|| "# custom_path = \"/path/to/metadata.json\"\n".to_string()),
            config.auto_detect_continuations,
            config.auto_clean
        );
        fs::write(&path, updated_content)?;
    }

    debug!("Loaded config from {}", path.display());
    Ok(config)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn test_config_default_values() {
        let config = Config::default();
        assert_eq!(config.metadata_storage, "global");
        assert!(!config.auto_detect_continuations);
        assert!(config.auto_clean);
        assert!(config.index.auto_update);
    }

    #[test]
    fn test_config_parse_minimal() {
        let content = r#"metadata_storage = "global""#;
        let config: Config = toml::from_str(content).unwrap();
        assert_eq!(config.metadata_storage, "global");
        // Defaults should apply
        assert!(!config.auto_detect_continuations);
        assert!(config.auto_clean);
    }

    #[test]
    fn test_config_parse_full() {
        let content = r#"
metadata_storage = "custom"
custom_path = "/my/path.db"
auto_detect_continuations = true
auto_clean = false

[index]
auto_update = false
"#;
        let config: Config = toml::from_str(content).unwrap();
        assert_eq!(config.metadata_storage, "custom");
        assert_eq!(config.custom_path, Some("/my/path.db".to_string()));
        assert!(config.auto_detect_continuations);
        assert!(!config.auto_clean);
        assert!(!config.index.auto_update);
    }
}
