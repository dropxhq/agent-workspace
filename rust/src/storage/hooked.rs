use std::path::PathBuf;

use crate::config::{HookConfig, IoOptions};
use crate::error::{WsError, WsResult};
use crate::hooks::{run_hook, HookContext};
use crate::metadata::FileMetadata;
use crate::ranges::{apply_write_ranges, filter_lines, LineRange};
use crate::storage::{BackendHandle, ListReport, WorkspaceBackend};

pub struct HookedBackend {
    inner: Box<BackendHandle>,
    hooks: HookConfig,
    config_dir: PathBuf,
}

impl HookedBackend {
    pub fn new(inner: BackendHandle, hooks: HookConfig, config_dir: PathBuf) -> Self {
        Self {
            inner: Box::new(inner),
            hooks,
            config_dir,
        }
    }

    fn apply_read_hook(&self, path: &str, physical: &str, opts: IoOptions) -> WsResult<String> {
        if opts.skip_hooks {
            return Ok(physical.to_string());
        }
        let Some(cmd) = &self.hooks.read else {
            return Ok(physical.to_string());
        };
        let ctx = HookContext {
            hook_kind: "read",
            path,
            work_dir: &self.config_dir,
        };
        run_hook(cmd, physical, &ctx)
    }

    fn apply_write_hook(&self, path: &str, logical: &str, opts: IoOptions) -> WsResult<String> {
        if opts.skip_hooks {
            return Ok(logical.to_string());
        }
        let Some(cmd) = &self.hooks.write else {
            return Ok(logical.to_string());
        };
        let ctx = HookContext {
            hook_kind: "write",
            path,
            work_dir: &self.config_dir,
        };
        run_hook(cmd, logical, &ctx)
    }

    fn read_physical(&self, path: &str) -> WsResult<String> {
        self.inner
            .read(path, None, IoOptions { skip_hooks: true })
    }
}

impl WorkspaceBackend for HookedBackend {
    fn read(
        &self,
        path: &str,
        ranges: Option<&[LineRange]>,
        opts: IoOptions,
    ) -> WsResult<String> {
        if opts.skip_hooks {
            return self.inner.read(path, ranges, opts);
        }

        let physical = self.read_physical(path)?;
        let logical = self.apply_read_hook(path, &physical, opts)?;
        Ok(match ranges {
            Some(ranges) => filter_lines(&logical, ranges),
            None => logical,
        })
    }

    fn write(
        &self,
        path: &str,
        ranges: Option<&LineRange>,
        content: &str,
        created_by: &str,
        desc: &str,
        opts: IoOptions,
    ) -> WsResult<()> {
        if opts.skip_hooks {
            return self
                .inner
                .write(path, ranges, content, created_by, desc, opts);
        }

        let physical = if let Some(range) = ranges {
            let raw_existing = match self.read_physical(path) {
                Ok(content) => content,
                Err(WsError::NotFound(_)) => String::new(),
                Err(e) => return Err(e),
            };
            let logical_existing = self.apply_read_hook(path, &raw_existing, IoOptions::default())?;
            let merged_logical = apply_write_ranges(&logical_existing, range, content);
            self.apply_write_hook(path, &merged_logical, opts)?
        } else {
            self.apply_write_hook(path, content, opts)?
        };

        self.inner.write(
            path,
            None,
            &physical,
            created_by,
            desc,
            IoOptions { skip_hooks: true },
        )
    }

    fn list(&self, scope: Option<&str>) -> WsResult<ListReport> {
        self.inner.list(scope)
    }

    fn remove(&self, path: &str) -> WsResult<()> {
        self.inner.remove(path)
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    pub const ENCODE_PY: &str = "import sys; print('ENC:'+sys.stdin.read(), end='')";
    pub const DECODE_PY: &str =
        "import sys; s=sys.stdin.read(); print(s[4:] if s.startswith('ENC:') else s, end='')";
    pub const FAIL_PY: &str = "import sys; sys.exit(1)";
}

#[cfg(test)]
mod tests {
    use super::test_support::{DECODE_PY, ENCODE_PY, FAIL_PY};
    use super::*;
    use crate::config::{BackendConfig, Config, HookCommand, HookConfig};
    use crate::error::WsError;
    use crate::scoping::SessionScope;
    use crate::storage::{open_scoped_backend, BackendHandle, WorkspaceBackend};
    use std::fs;
    use tempfile::TempDir;

    fn python_cmd(code: &str) -> HookCommand {
        HookCommand {
            command: vec!["python3".to_string(), "-c".to_string(), code.to_string()],
            timeout_ms: 5_000,
        }
    }

    fn setup_config(tmp: &TempDir) -> (Config, std::path::PathBuf) {
        let data = tmp.path().join("data");
        fs::create_dir_all(&data).unwrap();
        let workspace_dir = fs::canonicalize(&data).unwrap();
        let config_path = tmp.path().join("config.yaml");
        fs::write(
            &config_path,
            format!(
                r#"
backend:
  type: file
  workspace_dir: {}
hooks:
  read:
    command: ["python3", "-c", "{}"]
  write:
    command: ["python3", "-c", "{}"]
"#,
                data.display(),
                DECODE_PY.replace('"', "\\\""),
                ENCODE_PY.replace('"', "\\\"")
            ),
        )
        .unwrap();

        let config = Config {
            config_path: config_path.clone(),
            backend: BackendConfig::File {
                workspace_dir: workspace_dir.clone(),
                metadata_suffix: ".meta.yaml".to_string(),
            },
            hooks: Some(HookConfig {
                read: Some(python_cmd(DECODE_PY)),
                write: Some(python_cmd(ENCODE_PY)),
            }),
        };
        (config, workspace_dir)
    }

    #[test]
    fn hook_round_trip() {
        let tmp = TempDir::new().unwrap();
        let (config, workspace_dir) = setup_config(&tmp);
        let backend = open_scoped_backend(&config, SessionScope::default()).unwrap();

        backend
            .write("note.txt", None, "hello\n", "agent", "", IoOptions::default())
            .unwrap();

        let physical = fs::read_to_string(workspace_dir.join("note.txt")).unwrap();
        assert_eq!(physical, "ENC:hello\n");

        let logical = backend
            .read("note.txt", None, IoOptions::default())
            .unwrap();
        assert_eq!(logical, "hello\n");
    }

    #[test]
    fn range_write_merges_in_logical_space() {
        let tmp = TempDir::new().unwrap();
        let (config, workspace_dir) = setup_config(&tmp);
        let backend = open_scoped_backend(&config, SessionScope::default()).unwrap();

        backend
            .write("partial.txt", None, "a\nb\nc\n", "agent", "", IoOptions::default())
            .unwrap();
        backend
            .write(
                "partial.txt",
                Some(&LineRange { start: 2, end: 2 }),
                "B\n",
                "agent",
                "",
                IoOptions::default(),
            )
            .unwrap();

        let physical = fs::read_to_string(workspace_dir.join("partial.txt")).unwrap();
        assert_eq!(physical, "ENC:a\nB\nc\n");

        let logical = backend
            .read("partial.txt", None, IoOptions::default())
            .unwrap();
        assert_eq!(logical, "a\nB\nc\n");
    }

    #[test]
    fn skip_hooks_reads_and_writes_physical_content() {
        let tmp = TempDir::new().unwrap();
        let (config, workspace_dir) = setup_config(&tmp);
        let backend = open_scoped_backend(&config, SessionScope::default()).unwrap();

        backend
            .write(
                "raw.txt",
                None,
                "ENC:plain\n",
                "agent",
                "",
                IoOptions { skip_hooks: true },
            )
            .unwrap();

        let physical = fs::read_to_string(workspace_dir.join("raw.txt")).unwrap();
        assert_eq!(physical, "ENC:plain\n");

        let content = backend
            .read("raw.txt", None, IoOptions { skip_hooks: true })
            .unwrap();
        assert_eq!(content, "ENC:plain\n");
    }

    #[test]
    fn hook_failure_does_not_partially_write() {
        let tmp = TempDir::new().unwrap();
        let (config, workspace_dir) = setup_config(&tmp);
        let mut config = config;
        config.hooks = Some(HookConfig {
            read: Some(python_cmd(DECODE_PY)),
            write: Some(python_cmd(FAIL_PY)),
        });
        let backend = open_scoped_backend(&config, SessionScope::default()).unwrap();

        let err = backend
            .write("fail.txt", None, "hello", "agent", "", IoOptions::default())
            .unwrap_err();
        assert!(matches!(err, WsError::Other(_)));
        assert!(!workspace_dir.join("fail.txt").exists());
    }

    #[test]
    fn config_without_hooks_behaves_unchanged() {
        let tmp = TempDir::new().unwrap();
        let data = tmp.path().join("data");
        fs::create_dir_all(&data).unwrap();
        let config_path = tmp.path().join("config.yaml");
        fs::write(
            &config_path,
            format!(
                r#"
backend:
  type: file
  workspace_dir: {}
"#,
                data.display()
            ),
        )
        .unwrap();

        let config = crate::config::Config::load_from_path(&config_path).unwrap();
        assert!(config.hooks.is_none());
        let backend = open_scoped_backend(&config, SessionScope::default()).unwrap();
        assert!(matches!(backend, BackendHandle::File(_)));

        backend
            .write("plain.txt", None, "hello", "agent", "", IoOptions::default())
            .unwrap();
        let content = backend
            .read("plain.txt", None, IoOptions::default())
            .unwrap();
        assert_eq!(content, "hello");
    }
}
