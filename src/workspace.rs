use std::path::{Component, Path, PathBuf};

use crate::config::Config;
use crate::error::{WsError, WsResult};

/// Optional user/session scoping for workspace operations.
///
/// - Both `user_id` and `session_id`: `workspace_dir/user_id/session_id`
/// - Only `user_id`: `workspace_dir/user_id`
/// - Only `session_id` or neither: no subpath (`workspace_dir`)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionScope {
    prefix: Option<String>,
}

impl SessionScope {
    pub fn from_options(user_id: Option<&str>, session_id: Option<&str>) -> WsResult<Self> {
        match (user_id, session_id) {
            (Some(user_id), Some(session_id)) => {
                validate_scope_segment(user_id)?;
                validate_scope_segment(session_id)?;
                Ok(Self {
                    prefix: Some(format!("{user_id}/{session_id}")),
                })
            }
            (Some(user_id), None) => {
                validate_scope_segment(user_id)?;
                Ok(Self {
                    prefix: Some(user_id.to_string()),
                })
            }
            _ => Ok(Self { prefix: None }),
        }
    }

    pub fn prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }

    pub fn effective_root(&self, workspace_dir: &Path) -> PathBuf {
        match &self.prefix {
            Some(prefix) => workspace_dir.join(prefix),
            None => workspace_dir.to_path_buf(),
        }
    }

    pub fn storage_path(&self, path: &str) -> String {
        let relative = normalize_workspace_relative(path);
        match &self.prefix {
            Some(prefix) if relative.is_empty() => prefix.clone(),
            Some(prefix) => format!("{prefix}/{relative}"),
            None => relative,
        }
    }

    pub fn display_path(&self, storage_path: &str) -> String {
        let Some(prefix) = &self.prefix else {
            return storage_path.to_string();
        };

        if storage_path == prefix {
            return String::new();
        }

        let marker = format!("{prefix}/");
        if let Some(rest) = storage_path.strip_prefix(&marker) {
            rest.to_string()
        } else {
            storage_path.to_string()
        }
    }
}

fn validate_scope_segment(segment: &str) -> WsResult<()> {
    let segment = segment.trim();
    if segment.is_empty() {
        return Err(WsError::InvalidPath(
            "user_id and session_id must be non-empty when provided".to_string(),
        ));
    }
    if segment.contains('/') || segment.contains('\\') || segment == "." || segment == ".." {
        return Err(WsError::InvalidPath(format!(
            "invalid scope segment '{segment}': must be a single path component"
        )));
    }
    Ok(())
}

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
        assert_eq!(
            normalize_workspace_relative("./docs/foo.txt"),
            "docs/foo.txt"
        );
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
    fn session_scope_resolves_prefix_from_options() {
        let full = SessionScope::from_options(Some("alice"), Some("sess-1")).unwrap();
        assert_eq!(full.prefix(), Some("alice/sess-1"));

        let user_only = SessionScope::from_options(Some("alice"), None).unwrap();
        assert_eq!(user_only.prefix(), Some("alice"));

        let session_only = SessionScope::from_options(None, Some("sess-1")).unwrap();
        assert!(session_only.prefix().is_none());
    }

    #[test]
    fn session_scope_storage_and_display_paths() {
        let full = SessionScope::from_options(Some("u1"), Some("s2")).unwrap();
        assert_eq!(full.storage_path("docs/a.txt"), "u1/s2/docs/a.txt");
        assert_eq!(full.display_path("u1/s2/docs/a.txt"), "docs/a.txt");

        let user_only = SessionScope::from_options(Some("u1"), None).unwrap();
        assert_eq!(user_only.storage_path("docs/a.txt"), "u1/docs/a.txt");
        assert_eq!(user_only.display_path("u1/docs/a.txt"), "docs/a.txt");
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
