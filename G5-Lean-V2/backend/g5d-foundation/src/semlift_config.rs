//! Partial `semlift.yaml` configuration shared by runtime families.

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors loading the partial runtime SemLift configuration.
#[derive(Debug, Error)]
pub enum SemliftConfigError {
    /// The configuration file could not be read.
    #[error("failed to read {path}: {source}")]
    Read {
        /// Configuration path.
        path: String,
        /// Filesystem error.
        source: std::io::Error,
    },
    /// The configuration file was not valid YAML for the owned sections.
    #[error("failed to parse {path}: {source}")]
    Parse {
        /// Configuration path.
        path: String,
        /// YAML parse error.
        source: serde_yaml::Error,
    },
}

/// Partial parse of `semlift.yaml` for fields the Rust runtime needs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SemliftConfig {
    /// View workflow behavior.
    #[serde(default)]
    pub views: ViewsConfig,
    /// Grounding workflow behavior.
    #[serde(default)]
    pub grounding: GroundingConfig,
}

impl SemliftConfig {
    /// Load the partial configuration from `semlift.yaml` at a workspace root.
    pub fn load(workspace_path: &Path) -> Result<Option<Self>, SemliftConfigError> {
        let config_path = workspace_path.join("semlift.yaml");
        if !config_path.exists() {
            return Ok(None);
        }
        let path = config_path.display().to_string();
        let content =
            std::fs::read_to_string(&config_path).map_err(|source| SemliftConfigError::Read {
                path: path.clone(),
                source,
            })?;
        serde_yaml::from_str(&content)
            .map(Some)
            .map_err(|source| SemliftConfigError::Parse { path, source })
    }
}

/// Controls view workflow behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewsConfig {
    /// Use standard per-file/per-node fanout for view sync.
    #[serde(default = "default_true")]
    pub fan_out_sync: bool,
    /// Generate code tasks for specs that name source files.
    #[serde(default = "default_true")]
    pub generate_code: bool,
}

impl Default for ViewsConfig {
    fn default() -> Self {
        Self {
            fan_out_sync: true,
            generate_code: true,
        }
    }
}

/// Controls grounding workflow behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingConfig {
    /// Allow chat tasks to persist chat intent and grounding records.
    #[serde(default = "default_true")]
    pub chat_intents_enabled: bool,
}

impl Default for GroundingConfig {
    fn default() -> Self {
        Self {
            chat_intents_enabled: true,
        }
    }
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_sections_and_fields_default_enabled() {
        let config: SemliftConfig = serde_yaml::from_str("{}\n").expect("config parses");
        assert!(config.views.fan_out_sync);
        assert!(config.views.generate_code);
        assert!(config.grounding.chat_intents_enabled);
    }
}
