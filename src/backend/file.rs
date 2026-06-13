use crate::error::{WsError, WsResult};
use std::fs;
use std::io;
use walkdir::WalkDir;

use crate::backend::content::filter_lines;
use crate::backend::path::{list_scope_prefix, path_matches_scope};
use crate::backend::{ListReport, WorkspaceBackend};
use crate::commands::ranges::{apply_write_ranges, LineRange};
use crate::lock::FileLock;
use crate::meta::{build_metadata_in, sidecar_absolute_in, FileMetadata};
use crate::workspace::{
    data_path_from_metadata, is_metadata_path, parse_ws_path_for_write_in, parse_ws_path_in,
};
use std::path::PathBuf;

pub struct FileBackend {
    pub workspace_dir: PathBuf,
    pub metadata_suffix: String,
}

impl FileBackend {
    pub fn new(workspace_dir: PathBuf, metadata_suffix: String) -> Self {
        Self {
            workspace_dir,
            metadata_suffix,
        }
    }
}

impl WorkspaceBackend for FileBackend {
    fn read(
        &self,
        path: &str,
        ranges: Option<&[crate::commands::ranges::LineRange]>,
    ) -> WsResult<String> {
        let resolved = parse_ws_path_in(&self.workspace_dir, path)?;

        if is_metadata_path(&resolved.relative, &self.metadata_suffix) {
            return Err(WsError::NotFound(resolved.relative));
        }

        if !resolved.absolute.is_file() {
            return Err(WsError::NotFound(resolved.relative));
        }

        let _lock = FileLock::shared(&resolved.absolute)?;

        let content = fs::read_to_string(&resolved.absolute).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                WsError::NotFound(resolved.relative.clone())
            } else {
                WsError::Io(e)
            }
        })?;

        if let Some(ranges) = ranges {
            Ok(filter_lines(&content, ranges))
        } else {
            Ok(content)
        }
    }

    fn write(
        &self,
        path: &str,
        ranges: Option<&LineRange>,
        content: &str,
        created_by: &str,
        desc: &str,
    ) -> WsResult<()> {
        let resolved =
            parse_ws_path_for_write_in(&self.workspace_dir, &self.metadata_suffix, path)?;

        let _lock = FileLock::exclusive(&resolved.absolute)?;

        let final_content = if let Some(range) = ranges {
            let existing = if resolved.absolute.is_file() {
                fs::read_to_string(&resolved.absolute).map_err(WsError::Io)?
            } else {
                String::new()
            };
            apply_write_ranges(&existing, range, content)
        } else {
            content.to_string()
        };

        if let Some(parent) = resolved.absolute.parent() {
            fs::create_dir_all(parent).map_err(WsError::Io)?;
        }

        fs::write(&resolved.absolute, &final_content).map_err(WsError::Io)?;

        let metadata = build_metadata_in(
            &self.workspace_dir,
            &self.metadata_suffix,
            &resolved.relative,
            final_content.as_bytes(),
            created_by,
            desc,
        )?;

        let sidecar = sidecar_absolute_in(
            &self.workspace_dir,
            &self.metadata_suffix,
            &resolved.relative,
        )?;
        if let Some(parent) = sidecar.parent() {
            fs::create_dir_all(parent).map_err(WsError::Io)?;
        }
        metadata.write_to_sidecar(&sidecar)?;

        Ok(())
    }

    fn list(&self, scope: Option<&str>) -> WsResult<ListReport> {
        let (scan_root, report_scope) =
            resolve_list_scope(scope, &self.workspace_dir, &self.metadata_suffix)?;
        let scope_prefix = list_scope_prefix(report_scope.as_deref());

        let mut files = Vec::new();
        let mut total_size: u64 = 0;

        for entry in WalkDir::new(&scan_root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !file_name.ends_with(&self.metadata_suffix) {
                continue;
            }

            let rel = path
                .strip_prefix(&self.workspace_dir)
                .map_err(|e| WsError::Other(e.to_string()))?;
            let rel_str = rel.to_string_lossy().replace('\\', "/");

            if data_path_from_metadata(&rel_str, &self.metadata_suffix).is_none() {
                continue;
            }

            match FileMetadata::read_from_sidecar(path) {
                Ok(meta) => {
                    if !path_matches_scope(&meta.relative_path, scope_prefix.as_deref()) {
                        continue;
                    }
                    total_size += meta.size_bytes;
                    files.push(meta);
                }
                Err(_) => continue,
            }
        }

        files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

        Ok(ListReport {
            scope: report_scope,
            file_count: files.len(),
            total_size_bytes: total_size,
            files,
        })
    }

    fn remove(&self, path: &str) -> WsResult<()> {
        let resolved = parse_ws_path_in(&self.workspace_dir, path)?;

        if is_metadata_path(&resolved.relative, &self.metadata_suffix) {
            return Err(WsError::NotFound(resolved.relative));
        }

        if !resolved.absolute.is_file() {
            return Err(WsError::NotFound(resolved.relative));
        }

        let _lock = FileLock::exclusive(&resolved.absolute)?;

        fs::remove_file(&resolved.absolute).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                WsError::NotFound(resolved.relative.clone())
            } else {
                WsError::Io(e)
            }
        })?;

        let sidecar = sidecar_absolute_in(
            &self.workspace_dir,
            &self.metadata_suffix,
            &resolved.relative,
        )?;
        if sidecar.exists() {
            let _ = fs::remove_file(&sidecar);
        }

        Ok(())
    }
}

fn resolve_list_scope(
    scope: Option<&str>,
    workspace_dir: &std::path::Path,
    metadata_suffix: &str,
) -> WsResult<(PathBuf, Option<String>)> {
    let Some(raw_scope) = scope else {
        return Ok((workspace_dir.to_path_buf(), None));
    };

    let resolved = parse_ws_path_for_write_in(workspace_dir, metadata_suffix, raw_scope)?;
    if resolved.relative.is_empty() {
        return Ok((workspace_dir.to_path_buf(), None));
    }

    if !resolved.absolute.is_dir() {
        return Err(WsError::NotFound(resolved.relative));
    }

    Ok((resolved.absolute, Some(resolved.relative)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn setup_backend() -> (TempDir, FileBackend) {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let backend = FileBackend::new(
            std::fs::canonicalize(workspace).unwrap(),
            ".meta.yaml".to_string(),
        );
        (tmp, backend)
    }

    fn file_path(backend: &FileBackend, relative: &str) -> PathBuf {
        backend.workspace_dir.join(relative)
    }

    #[test]
    fn write_read_and_filter_ranges() {
        let (_tmp, backend) = setup_backend();

        backend
            .write("docs/note.txt", None, "one\ntwo\nthree\n", "agent", "seed")
            .unwrap();

        let all = backend.read("docs/note.txt", None).unwrap();
        assert_eq!(all, "one\ntwo\nthree\n");

        let ranges = [LineRange { start: 2, end: 3 }];
        let filtered = backend.read("docs/note.txt", Some(&ranges)).unwrap();
        assert_eq!(filtered, "two\nthree\n");

        let sidecar = file_path(&backend, "docs/note.txt.meta.yaml");
        assert!(sidecar.is_file());
    }

    #[test]
    fn write_with_single_range_replaces_only_target_lines() {
        let (_tmp, backend) = setup_backend();

        backend
            .write("partial.txt", None, "a\nb\nc\n", "agent", "")
            .unwrap();
        backend
            .write(
                "partial.txt",
                Some(&LineRange { start: 2, end: 2 }),
                "B\n",
                "agent",
                "",
            )
            .unwrap();

        let content = backend.read("partial.txt", None).unwrap();
        assert_eq!(content, "a\nB\nc\n");
    }

    #[test]
    fn list_scope_and_remove_work_end_to_end() {
        let (_tmp, backend) = setup_backend();

        backend.write("docs/a.txt", None, "A", "agent", "").unwrap();
        backend
            .write("other/b.txt", None, "BB", "agent", "")
            .unwrap();

        let scoped = backend.list(Some("docs")).unwrap();
        assert_eq!(scoped.scope.as_deref(), Some("docs"));
        assert_eq!(scoped.file_count, 1);
        assert_eq!(scoped.files[0].relative_path, "docs/a.txt");

        let all = backend.list(None).unwrap();
        assert_eq!(all.file_count, 2);
        assert_eq!(all.total_size_bytes, 3);

        backend.remove("docs/a.txt").unwrap();
        assert!(!file_path(&backend, "docs/a.txt").exists());
        assert!(!file_path(&backend, "docs/a.txt.meta.yaml").exists());
    }

    #[test]
    fn metadata_path_is_hidden_for_read_and_remove() {
        let (_tmp, backend) = setup_backend();
        fs::write(file_path(&backend, "secret.txt.meta.yaml"), "dummy").unwrap();

        let read_err = backend.read("secret.txt.meta.yaml", None).unwrap_err();
        assert!(matches!(read_err, WsError::NotFound(_)));

        let remove_err = backend.remove("secret.txt.meta.yaml").unwrap_err();
        assert!(matches!(remove_err, WsError::NotFound(_)));
    }
}
