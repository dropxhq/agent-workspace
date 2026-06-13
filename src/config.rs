use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{WsError, WsResult};

const DEFAULT_METADATA_SUFFIX: &str = ".meta.yaml";

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub workspace_dir: PathBuf,
    #[serde(default = "default_metadata_suffix")]
    pub metadata_suffix: String,
}

fn default_metadata_suffix() -> String {
    DEFAULT_METADATA_SUFFIX.to_string()
}

impl Config {
    pub fn load() -> WsResult<Self> {
        let config_path = resolve_config_path()?;
        let contents = fs::read_to_string(&config_path).map_err(|e| {
            WsError::Other(format!(
                "failed to read config {}: {e}",
                config_path.display()
            ))
        })?;

        let raw: RawConfig = serde_yaml::from_str(&contents).map_err(|e| {
            WsError::Other(format!(
                "failed to parse config {}: {e}",
                config_path.display()
            ))
        })?;

        let config_dir = config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let workspace_dir = if raw.workspace_dir.is_absolute() {
            raw.workspace_dir
        } else {
            config_dir.join(raw.workspace_dir)
        };

        let workspace_dir = fs::canonicalize(&workspace_dir).map_err(|e| {
            WsError::Other(format!(
                "workspace_dir {} does not exist or is inaccessible: {e}",
                workspace_dir.display()
            ))
        })?;

        if !workspace_dir.is_dir() {
            return Err(WsError::Other(format!(
                "workspace_dir {} is not a directory",
                workspace_dir.display()
            )));
        }

        let test_file = workspace_dir.join(".ws_write_test");
        fs::write(&test_file, b"").map_err(|e| {
            WsError::Other(format!(
                "workspace_dir {} is not writable: {e}",
                workspace_dir.display()
            ))
        })?;
        let _ = fs::remove_file(test_file);

        Ok(Config {
            workspace_dir,
            metadata_suffix: raw.metadata_suffix,
        })
    }
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    workspace_dir: PathBuf,
    #[serde(default = "default_metadata_suffix")]
    metadata_suffix: String,
}

fn resolve_config_path() -> WsResult<PathBuf> {
    if let Ok(path) = env::var("AGENT_WORKSPACE_CONFIG") {
        let path = PathBuf::from(path);
        if !path.is_file() {
            return Err(WsError::Other(format!(
                "AGENT_WORKSPACE_CONFIG points to non-existent file: {}",
                path.display()
            )));
        }
        return Ok(path);
    }

    let cwd_config = env::current_dir()
        .map_err(WsError::Io)?
        .join("config.yaml");
    if !cwd_config.is_file() {
        return Err(WsError::Other(format!(
            "config not found: set AGENT_WORKSPACE_CONFIG or place config.yaml in cwd ({})",
            cwd_config.display()
        )));
    }
    Ok(cwd_config)
}
