use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::{WsError, WsResult};
use crate::paths::{is_metadata_path, normalize_workspace_relative};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPath {
    pub relative: String,
    pub absolute: PathBuf,
}

pub fn parse_ws_path(input: &str, config: &Config) -> WsResult<ResolvedPath> {
    parse_ws_path_in(config.workspace_dir(), input)
}

pub fn parse_ws_path_in(workspace_dir: &Path, input: &str) -> WsResult<ResolvedPath> {
    let relative = normalize_workspace_relative(input);
    resolve_relative_in(&relative, workspace_dir)
}

pub fn resolve_relative(relative: &str, config: &Config) -> WsResult<ResolvedPath> {
    resolve_relative_in(relative, config.workspace_dir())
}

pub fn resolve_relative_in(relative: &str, workspace_dir: &Path) -> WsResult<ResolvedPath> {
    let absolute = workspace_dir.join(relative);
    validate_within_workspace(&absolute, workspace_dir)?;

    Ok(ResolvedPath {
        relative: relative.to_string(),
        absolute,
    })
}

pub fn validate_within_workspace(path: &Path, workspace: &Path) -> WsResult<()> {
    let workspace_canonical = std::fs::canonicalize(workspace).map_err(WsError::Io)?;

    if path.exists() {
        let canonical = std::fs::canonicalize(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                WsError::NotFound(path.display().to_string())
            } else {
                WsError::Io(e)
            }
        })?;
        if !canonical.starts_with(&workspace_canonical) {
            return Err(WsError::PathEscape(path.display().to_string()));
        }
        return Ok(());
    }

    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            let canonical = std::fs::canonicalize(&current).map_err(WsError::Io)?;
            if !canonical.starts_with(&workspace_canonical) {
                return Err(WsError::PathEscape(path.display().to_string()));
            }
            return Ok(());
        }
        if !current.pop() {
            break;
        }
    }

    Ok(())
}

/// Parse a workspace-relative path for write/remove when parent dirs may not exist yet.
pub fn parse_ws_path_for_write(input: &str, config: &Config) -> WsResult<ResolvedPath> {
    parse_ws_path_for_write_in(config.workspace_dir(), config.metadata_suffix(), input)
}

/// Parse a workspace-relative path for write/remove when parent dirs may not exist yet.
pub fn parse_ws_path_for_write_in(
    workspace_dir: &Path,
    metadata_suffix: &str,
    input: &str,
) -> WsResult<ResolvedPath> {
    let relative = normalize_workspace_relative(input);

    if is_metadata_path(&relative, metadata_suffix) {
        return Err(WsError::NotFound(relative));
    }

    let absolute = workspace_dir.join(&relative);

    if absolute.exists() {
        validate_within_workspace(&absolute, workspace_dir)?;
    } else {
        validate_parent_within_workspace(&absolute, workspace_dir)?;
    }

    Ok(ResolvedPath { relative, absolute })
}

fn validate_parent_within_workspace(path: &Path, workspace: &Path) -> WsResult<()> {
    let workspace_canonical = std::fs::canonicalize(workspace).map_err(WsError::Io)?;
    let mut current = path.to_path_buf();

    loop {
        if current.exists() {
            let canonical = std::fs::canonicalize(&current).map_err(WsError::Io)?;
            if !canonical.starts_with(&workspace_canonical) {
                return Err(WsError::PathEscape(path.display().to_string()));
            }
            return Ok(());
        }
        if !current.pop() {
            break;
        }
    }

    Ok(())
}
