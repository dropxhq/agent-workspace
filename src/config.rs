use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{WsError, WsResult};

const DEFAULT_METADATA_SUFFIX: &str = ".meta.yaml";
const DEFAULT_MYSQL_PORT: u16 = 3306;

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

fn default_metadata_suffix() -> String {
    DEFAULT_METADATA_SUFFIX.to_string()
}

fn default_mysql_port() -> u16 {
    DEFAULT_MYSQL_PORT
}

#[derive(Debug, Deserialize)]
struct RawFileBackend {
    #[serde(default = "default_type_file")]
    r#type: String,
    workspace_dir: PathBuf,
    #[serde(default = "default_metadata_suffix")]
    metadata_suffix: String,
}

fn default_type_file() -> String {
    "file".to_string()
}

#[derive(Debug, Deserialize)]
struct RawMysqlBackend {
    #[serde(default = "default_type_mysql")]
    r#type: String,
    host: String,
    #[serde(default = "default_mysql_port")]
    port: u16,
    user: String,
    password: String,
    database: String,
}

fn default_type_mysql() -> String {
    "mysql".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawBackend {
    Wrapped { backend: RawBackendInner },
    File(RawFileBackend),
    Mysql(RawMysqlBackend),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawBackendInner {
    File(RawFileBackend),
    Mysql(RawMysqlBackend),
}

impl Config {
    pub fn load() -> WsResult<Self> {
        let config_path = resolve_config_path()?;
        Self::load_from_path(&config_path)
    }

    pub fn load_from_path(config_path: &Path) -> WsResult<Self> {
        let contents = fs::read_to_string(config_path).map_err(|e| {
            WsError::Other(format!(
                "failed to read config {}: {e}",
                config_path.display()
            ))
        })?;

        let raw: RawBackend = serde_yaml::from_str(&contents).map_err(|e| {
            WsError::Other(format!(
                "failed to parse config {}: {e}",
                config_path.display()
            ))
        })?;

        let config_dir = config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let backend = match raw {
            RawBackend::Wrapped { backend } => match backend {
                RawBackendInner::File(f) => parse_file_backend(f, &config_dir, config_path)?,
                RawBackendInner::Mysql(m) => parse_mysql_backend(m)?,
            },
            RawBackend::File(f) => parse_file_backend(f, &config_dir, config_path)?,
            RawBackend::Mysql(m) => parse_mysql_backend(m)?,
        };

        Ok(Config {
            config_path: config_path.to_path_buf(),
            backend,
        })
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

fn parse_file_backend(
    raw: RawFileBackend,
    config_dir: &Path,
    config_path: &Path,
) -> WsResult<BackendConfig> {
    if raw.r#type != "file" {
        return Err(WsError::Other(format!(
            "unknown backend type '{}' in {}",
            raw.r#type,
            config_path.display()
        )));
    }

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

    Ok(BackendConfig::File {
        workspace_dir,
        metadata_suffix: raw.metadata_suffix,
    })
}

fn parse_mysql_backend(raw: RawMysqlBackend) -> WsResult<BackendConfig> {
    if raw.r#type != "mysql" {
        return Err(WsError::Other(format!(
            "unknown backend type '{}', expected 'mysql'",
            raw.r#type
        )));
    }
    if raw.host.is_empty() || raw.user.is_empty() || raw.database.is_empty() {
        return Err(WsError::Other(
            "mysql backend requires host, user, and database".to_string(),
        ));
    }

    Ok(BackendConfig::Mysql {
        host: raw.host,
        port: raw.port,
        user: raw.user,
        password: raw.password,
        database: raw.database,
    })
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

    let cwd_config = env::current_dir().map_err(WsError::Io)?.join("config.yaml");
    if !cwd_config.is_file() {
        return Err(WsError::Other(format!(
            "config not found: set AGENT_WORKSPACE_CONFIG or place config.yaml in cwd ({})",
            cwd_config.display()
        )));
    }
    Ok(cwd_config)
}

#[cfg(test)]
mod config_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn parses_file_backend_config() {
        let tmp = TempDir::new().unwrap();
        let data = tmp.path().join("data");
        fs::create_dir_all(&data).unwrap();
        let cfg_path = tmp.path().join("config.yaml");
        fs::write(
            &cfg_path,
            r#"
backend:
  type: file
  workspace_dir: ./data
  metadata_suffix: ".meta.yaml"
"#,
        )
        .unwrap();
        let config = Config::load_from_path(&cfg_path).unwrap();
        assert!(matches!(config.backend, BackendConfig::File { .. }));
    }
}
