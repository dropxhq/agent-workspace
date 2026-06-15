use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::raw::{
    RawBackend, RawBackendInner, RawConfig, RawFileBackend, RawHookCommand, RawHooks,
    RawMysqlBackend, DEFAULT_HOOK_TIMEOUT_MS,
};
use crate::config::{BackendConfig, Config, HookCommand, HookConfig};
use crate::error::{WsError, WsResult};

pub fn load() -> WsResult<Config> {
    let config_path = resolve_config_path()?;
    load_from_path(&config_path)
}

pub fn load_from_path(config_path: &Path) -> WsResult<Config> {
    let contents = fs::read_to_string(config_path).map_err(|e| {
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

    let backend = match raw.backend {
        RawBackend::Wrapped { backend } => match backend {
            RawBackendInner::File(f) => parse_file_backend(f, &config_dir, config_path)?,
            RawBackendInner::Mysql(m) => parse_mysql_backend(m)?,
        },
        RawBackend::File(f) => parse_file_backend(f, &config_dir, config_path)?,
        RawBackend::Mysql(m) => parse_mysql_backend(m)?,
    };

    let hooks = raw.hooks.map(parse_hooks).transpose()?;

    Ok(Config {
        config_path: config_path.to_path_buf(),
        backend,
        hooks,
    })
}

fn parse_hooks(raw: RawHooks) -> WsResult<HookConfig> {
    let read = raw.read.map(|h| parse_hook_command(h, "hooks.read")).transpose()?;
    let write = raw
        .write
        .map(|h| parse_hook_command(h, "hooks.write"))
        .transpose()?;

    if read.is_none() && write.is_none() {
        return Err(WsError::Other(
            "hooks block is present but neither hooks.read nor hooks.write is configured".to_string(),
        ));
    }

    if read.is_none() {
        eprintln!(
            "warning: hooks.read is not configured; read operations will see physical content while writes may be transformed"
        );
    }
    if write.is_none() {
        eprintln!(
            "warning: hooks.write is not configured; write operations will store logical content while reads may be transformed"
        );
    }

    Ok(HookConfig { read, write })
}

fn parse_hook_command(raw: RawHookCommand, label: &str) -> WsResult<HookCommand> {
    if raw.command.is_empty() {
        return Err(WsError::Other(format!(
            "{label}.command must be a non-empty argv array"
        )));
    }

    let timeout_ms = raw.timeout_ms.unwrap_or(DEFAULT_HOOK_TIMEOUT_MS);
    if timeout_ms == 0 {
        return Err(WsError::Other(format!(
            "{label}.timeout_ms must be greater than 0"
        )));
    }

    Ok(HookCommand {
        command: raw.command,
        timeout_ms,
    })
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
        let config = load_from_path(&cfg_path).unwrap();
        assert!(matches!(config.backend, BackendConfig::File { .. }));
        assert!(config.hooks.is_none());
    }

    #[test]
    fn parses_hooks_config() {
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
hooks:
  read:
    command: ["cat"]
  write:
    command: ["cat"]
"#,
        )
        .unwrap();
        let config = load_from_path(&cfg_path).unwrap();
        let hooks = config.hooks.expect("hooks");
        assert!(hooks.read.is_some());
        assert!(hooks.write.is_some());
    }
}
