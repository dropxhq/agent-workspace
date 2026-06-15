use std::path::{Path, PathBuf};

use crate::error::{WsError, WsResult};
use crate::paths::normalize_workspace_relative;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
