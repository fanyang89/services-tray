//! Service definitions and on-disk persistence.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Persisted definition of a managed service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Start this service automatically when the manager starts.
    #[serde(default)]
    pub autostart: bool,
    /// Restart automatically after a non-zero exit.
    #[serde(default)]
    pub auto_restart: bool,
}

/// Payload for create/update requests.
#[derive(Debug, Clone, Deserialize)]
pub struct ServiceInput {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub auto_restart: bool,
}

impl ServiceInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("服务名称不能为空".into());
        }
        if self.command.trim().is_empty() {
            return Err("启动命令不能为空".into());
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub services: Vec<ServiceConfig>,
}

/// Default config location, e.g. `~/.config/services-manager/config.toml`.
pub fn default_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("services-manager").join("config.toml"))
}

pub fn load(path: &Path) -> anyhow::Result<ConfigFile> {
    if !path.exists() {
        return Ok(ConfigFile::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    let cfg: ConfigFile = toml::from_str(&raw)
        .with_context(|| format!("failed to parse config file {}", path.display()))?;
    Ok(cfg)
}

/// Atomically persist the service list, sorted by name for a stable file.
pub fn save(path: &Path, cfg: &ConfigFile) -> anyhow::Result<()> {
    let mut sorted = cfg.clone();
    sorted.services.sort_by(|a, b| a.name.cmp(&b.name));

    let raw = toml::to_string_pretty(&sorted)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, raw)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
