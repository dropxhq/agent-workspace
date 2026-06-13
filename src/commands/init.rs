use std::env;
use std::fs;
use std::path::PathBuf;

#[cfg(not(test))]
use crate::storage::mysql::MySqlBackend;
use crate::config::templates::{DEFAULT_FILE_CONFIG, DEFAULT_MYSQL_CONFIG};
use crate::config::{BackendConfig, Config};
use crate::error::{WsError, WsResult};

pub fn run(target: Option<&str>, backend: &str) -> WsResult<()> {
    let base = resolve_target_dir(target)?;

    if base.is_file() {
        return Err(WsError::Other(format!(
            "{} is a file, expected a directory",
            base.display()
        )));
    }

    fs::create_dir_all(&base).map_err(WsError::Io)?;
    let base = fs::canonicalize(&base).map_err(WsError::Io)?;

    let config_path = base.join("config.yaml");
    if config_path.exists() {
        return Err(WsError::Other(format!(
            "already initialized: {} exists",
            config_path.display()
        )));
    }

    let backend = backend.trim().to_ascii_lowercase();
    match backend.as_str() {
        "file" => {
            let data_dir = base.join("data");
            fs::create_dir_all(&data_dir).map_err(WsError::Io)?;
            fs::write(&config_path, DEFAULT_FILE_CONFIG).map_err(WsError::Io)?;

            println!("Initialized workspace at {}", base.display());
            println!("  config: {}", config_path.display());
            println!("  data:   {}", data_dir.display());
        }
        "mysql" => {
            fs::write(&config_path, DEFAULT_MYSQL_CONFIG).map_err(WsError::Io)?;
            let config = Config::load_from_path(&config_path)?;
            ensure_mysql_schema(&config)?;

            println!("Initialized workspace at {}", base.display());
            println!("  config: {}", config_path.display());
            println!("  backend:mysql (schema ensured)");
        }
        _ => {
            return Err(WsError::Other(format!(
                "invalid backend '{backend}', expected 'file' or 'mysql'"
            )));
        }
    }

    Ok(())
}

fn resolve_target_dir(target: Option<&str>) -> WsResult<PathBuf> {
    match target {
        None => env::current_dir().map_err(WsError::Io),
        Some(path) => Ok(PathBuf::from(path)),
    }
}

#[cfg(not(test))]
fn ensure_mysql_schema(config: &Config) -> WsResult<()> {
    match &config.backend {
        BackendConfig::Mysql {
            host,
            port,
            user,
            password,
            database,
        } => MySqlBackend::ensure_schema(host, *port, user, password, database),
        _ => Err(WsError::Other(
            "expected mysql backend config when bootstrapping schema".to_string(),
        )),
    }
}

#[cfg(test)]
fn ensure_mysql_schema(config: &Config) -> WsResult<()> {
    match &config.backend {
        BackendConfig::Mysql { .. } => Ok(()),
        _ => Err(WsError::Other(
            "expected mysql backend config when bootstrapping schema".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn init_file_creates_config_and_data_dir() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("agent-ws");
        run(Some(target.to_str().unwrap()), "file").unwrap();

        assert!(target.join("config.yaml").is_file());
        assert!(target.join("data").is_dir());
        assert_eq!(
            fs::read_to_string(target.join("config.yaml")).unwrap(),
            DEFAULT_FILE_CONFIG
        );
    }

    #[test]
    fn init_mysql_writes_config_without_data_dir() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("agent-ws");
        run(Some(target.to_str().unwrap()), "mysql").unwrap();

        assert!(target.join("config.yaml").is_file());
        assert!(!target.join("data").exists());
        assert_eq!(
            fs::read_to_string(target.join("config.yaml")).unwrap(),
            DEFAULT_MYSQL_CONFIG
        );
    }

    #[test]
    fn init_rejects_existing_config() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("config.yaml"), DEFAULT_FILE_CONFIG).unwrap();

        let err = run(Some(tmp.path().to_str().unwrap()), "file").unwrap_err();
        assert!(matches!(err, WsError::Other(_)));
    }

    #[test]
    fn init_rejects_invalid_backend_type() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("agent-ws");
        let err = run(Some(target.to_str().unwrap()), "sqlite").unwrap_err();
        assert!(matches!(err, WsError::Other(_)));
    }
}
