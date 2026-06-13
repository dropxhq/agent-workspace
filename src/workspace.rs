use std::path::{Component, Path, PathBuf};

use crate::config::Config;
use crate::error::{WsError, WsResult};

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

pub fn normalize_workspace_relative(path: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();

    for segment in path.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            if !stack.is_empty() {
                stack.pop();
            }
            continue;
        }
        stack.push(segment);
    }

    stack.join("/")
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

pub fn is_metadata_path(relative: &str, metadata_suffix: &str) -> bool {
    relative.ends_with(metadata_suffix)
}

pub fn metadata_path_for(relative: &str, metadata_suffix: &str) -> String {
    format!("{relative}{metadata_suffix}")
}

pub fn data_path_from_metadata(relative: &str, metadata_suffix: &str) -> Option<String> {
    relative
        .strip_suffix(metadata_suffix)
        .map(|s| s.to_string())
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

    Ok(ResolvedPath {
        relative,
        absolute,
    })
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

/// Strip redundant components using std path logic (for tests).
#[allow(dead_code)]
pub fn strip_path_components(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(c) => result.push(c),
            Component::CurDir => {}
            _ => {}
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_basic_paths() {
        assert_eq!(normalize_workspace_relative("a/b/c.md"), "a/b/c.md");
        assert_eq!(normalize_workspace_relative("/a/b/c.md"), "a/b/c.md");
        assert_eq!(normalize_workspace_relative("../a/b/c.md"), "a/b/c.md");
        assert_eq!(normalize_workspace_relative("./docs/foo.txt"), "docs/foo.txt");
        assert_eq!(normalize_workspace_relative("../etc/passwd"), "etc/passwd");
        assert_eq!(normalize_workspace_relative("foo/../bar"), "bar");
    }

    #[test]
    fn normalize_root() {
        assert_eq!(normalize_workspace_relative(""), "");
        assert_eq!(normalize_workspace_relative("/"), "");
        assert_eq!(normalize_workspace_relative(".."), "");
        assert_eq!(normalize_workspace_relative("../.."), "");
    }

    #[test]
    fn metadata_path_detection() {
        assert!(is_metadata_path("foo.txt.meta.yaml", ".meta.yaml"));
        assert!(!is_metadata_path("foo.txt", ".meta.yaml"));
        assert_eq!(
            metadata_path_for("docs/foo.txt", ".meta.yaml"),
            "docs/foo.txt.meta.yaml"
        );
        assert_eq!(
            data_path_from_metadata("docs/foo.txt.meta.yaml", ".meta.yaml"),
            Some("docs/foo.txt".to_string())
        );
    }
}
