use std::path::{Path, PathBuf};

mod load;
mod raw;
pub mod templates;

pub use raw::DEFAULT_HOOK_TIMEOUT_MS;

#[derive(Debug, Clone, Copy, Default)]
pub struct IoOptions {
    pub skip_hooks: bool,
}

#[derive(Debug, Clone)]
pub struct HookCommand {
    pub command: Vec<String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub struct HookConfig {
    pub read: Option<HookCommand>,
    pub write: Option<HookCommand>,
}

impl HookConfig {
    pub fn is_empty(&self) -> bool {
        self.read.is_none() && self.write.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub config_path: PathBuf,
    pub backend: BackendConfig,
    pub hooks: Option<HookConfig>,
}

#[derive(Debug, Clone)]
pub enum BackendConfig {
    File {
        workspace_dir: PathBuf,
        metadata_suffix: String,
    },
    Mysql {
        host: String,
        port: u16,
        user: String,
        password: String,
        database: String,
    },
}

impl Config {
    pub fn load() -> crate::error::WsResult<Self> {
        load::load()
    }

    pub fn load_from_path(config_path: &Path) -> crate::error::WsResult<Self> {
        load::load_from_path(config_path)
    }

    pub fn config_dir(&self) -> PathBuf {
        self.config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn workspace_dir(&self) -> &PathBuf {
        match &self.backend {
            BackendConfig::File { workspace_dir, .. } => workspace_dir,
            BackendConfig::Mysql { .. } => {
                panic!("workspace_dir is only available for file backend")
            }
        }
    }

    pub fn metadata_suffix(&self) -> &str {
        match &self.backend {
            BackendConfig::File {
                metadata_suffix, ..
            } => metadata_suffix,
            BackendConfig::Mysql { .. } => {
                panic!("metadata_suffix is only available for file backend")
            }
        }
    }
}
