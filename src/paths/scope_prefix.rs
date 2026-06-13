use crate::paths::normalize_workspace_relative;

/// `docs` -> `docs/`, `docs/` -> `docs/`, `""` -> None (whole workspace)
pub fn list_scope_prefix(scope: Option<&str>) -> Option<String> {
    let Some(raw) = scope else {
        return None;
    };
    let normalized = normalize_workspace_relative(raw);
    if normalized.is_empty() {
        return None;
    }
    if normalized.ends_with('/') {
        Some(normalized)
    } else {
        Some(format!("{normalized}/"))
    }
}

pub fn path_matches_scope(relative_path: &str, scope_prefix: Option<&str>) -> bool {
    match scope_prefix {
        None => true,
        Some(prefix) => {
            let dir = prefix.strip_suffix('/').unwrap_or(prefix);
            relative_path == dir || relative_path.starts_with(prefix)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_scope_prefix_normalizes() {
        assert_eq!(list_scope_prefix(Some("docs")).as_deref(), Some("docs/"));
        assert_eq!(list_scope_prefix(Some("docs/")).as_deref(), Some("docs/"));
        assert_eq!(list_scope_prefix(None), None);
        assert_eq!(list_scope_prefix(Some("")).as_deref(), None);
    }

    #[test]
    fn path_matches_scope_prefix() {
        assert!(path_matches_scope("docs/foo.txt", Some("docs/")));
        assert!(path_matches_scope("docs", Some("docs/")));
        assert!(!path_matches_scope("docs-extra/foo.txt", Some("docs/")));
    }
}
