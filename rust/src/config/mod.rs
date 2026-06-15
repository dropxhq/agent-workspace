use std::path::PathBuf;

mod load;
mod raw;
pub mod templates;

#[derive(Debug, Clone)]
pub struct Config {
    pub config_path: PathBuf,
    pub backend: BackendConfig,
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
