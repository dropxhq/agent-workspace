use std::env;
use std::fs;
use std::path::PathBuf;

use crate::error::{WsError, WsResult};

const DEFAULT_CONFIG: &str = "workspace_dir: ./data\nmetadata_suffix: \".meta.yaml\"\n";

pub fn run(target: Option<&str>) -> WsResult<()> {
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

    let data_dir = base.join("data");
    fs::create_dir_all(&data_dir).map_err(WsError::Io)?;

    fs::write(&config_path, DEFAULT_CONFIG).map_err(WsError::Io)?;

    println!("Initialized workspace at {}", base.display());
    println!("  config: {}", config_path.display());
    println!("  data:   {}", data_dir.display());

    Ok(())
}

fn resolve_target_dir(target: Option<&str>) -> WsResult<PathBuf> {
    match target {
        None => env::current_dir().map_err(WsError::Io),
        Some(path) => Ok(PathBuf::from(path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn init_creates_config_and_data_dir() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("agent-ws");
        run(Some(target.to_str().unwrap())).unwrap();

        assert!(target.join("config.yaml").is_file());
        assert!(target.join("data").is_dir());
    }

    #[test]
    fn init_rejects_existing_config() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("config.yaml"), DEFAULT_CONFIG).unwrap();

        let err = run(Some(tmp.path().to_str().unwrap())).unwrap_err();
        assert!(matches!(err, WsError::Other(_)));
    }
}
