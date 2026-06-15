//! Integration tests for content hooks using the example scripts in `hooks/`.

use std::fs;
use std::path::{Path, PathBuf};

use agent_workspace::config::{Config, IoOptions};
use agent_workspace::error::WsError;
use agent_workspace::ranges::LineRange;
use agent_workspace::scoping::SessionScope;
use agent_workspace::storage::{open_scoped_backend, BackendHandle, WorkspaceBackend};
use tempfile::TempDir;

fn repo_hooks_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../hooks")
}

fn hook_script(name: &str) -> PathBuf {
    fs::canonicalize(repo_hooks_dir().join(name)).expect("hook script must exist")
}

fn setup_hooked_workspace(
    tmp: &TempDir,
    read_script: &str,
    write_script: &str,
) -> (Config, PathBuf, BackendHandle) {
    let data = tmp.path().join("data");
    fs::create_dir_all(&data).unwrap();
    let workspace_dir = fs::canonicalize(&data).unwrap();
    let config_path = tmp.path().join("config.yaml");
    let decode = hook_script(read_script);
    let encode = hook_script(write_script);

    fs::write(
        &config_path,
        format!(
            r#"
backend:
  type: file
  workspace_dir: {}
hooks:
  read:
    command: ["python3", "{}"]
  write:
    command: ["python3", "{}"]
"#,
            data.display(),
            decode.display(),
            encode.display()
        ),
    )
    .unwrap();

    let config = Config::load_from_path(&config_path).unwrap();
    let backend = open_scoped_backend(&config, SessionScope::default()).unwrap();
    (config, workspace_dir, backend)
}

#[test]
fn example_scripts_round_trip() {
    let tmp = TempDir::new().unwrap();
    let (_config, workspace_dir, backend) =
        setup_hooked_workspace(&tmp, "decode.py", "encode.py");

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
fn read_ranges_apply_after_decode() {
    let tmp = TempDir::new().unwrap();
    let (_config, _workspace_dir, backend) =
        setup_hooked_workspace(&tmp, "decode.py", "encode.py");

    backend
        .write(
            "lines.txt",
            None,
            "one\ntwo\nthree\n",
            "agent",
            "",
            IoOptions::default(),
        )
        .unwrap();

    let partial = backend
        .read(
            "lines.txt",
            Some(&[LineRange { start: 2, end: 2 }]),
            IoOptions::default(),
        )
        .unwrap();
    assert_eq!(partial, "two\n");
}

#[test]
fn range_write_merges_in_logical_space() {
    let tmp = TempDir::new().unwrap();
    let (_config, workspace_dir, backend) =
        setup_hooked_workspace(&tmp, "decode.py", "encode.py");

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
fn skip_hooks_bypasses_transform() {
    let tmp = TempDir::new().unwrap();
    let (_config, workspace_dir, backend) =
        setup_hooked_workspace(&tmp, "decode.py", "encode.py");

    backend
        .write(
            "raw.txt",
            None,
            "ENC:plain\n",
            "agent",
            "",
            IoOptions {
                skip_hooks: true,
            },
        )
        .unwrap();

    let physical = fs::read_to_string(workspace_dir.join("raw.txt")).unwrap();
    assert_eq!(physical, "ENC:plain\n");

    let content = backend
        .read(
            "raw.txt",
            None,
            IoOptions {
                skip_hooks: true,
            },
        )
        .unwrap();
    assert_eq!(content, "ENC:plain\n");
}

#[test]
fn passthrough_hooks_leave_content_unchanged() {
    let tmp = TempDir::new().unwrap();
    let (_config, workspace_dir, backend) =
        setup_hooked_workspace(&tmp, "passthrough.py", "passthrough.py");

    backend
        .write("plain.txt", None, "unchanged\n", "agent", "", IoOptions::default())
        .unwrap();

    let physical = fs::read_to_string(workspace_dir.join("plain.txt")).unwrap();
    assert_eq!(physical, "unchanged\n");

    let logical = backend
        .read("plain.txt", None, IoOptions::default())
        .unwrap();
    assert_eq!(logical, "unchanged\n");
}

#[test]
fn hook_failure_does_not_partially_write() {
    let tmp = TempDir::new().unwrap();
    let data = tmp.path().join("data");
    fs::create_dir_all(&data).unwrap();
    let workspace_dir = fs::canonicalize(&data).unwrap();
    let config_path = tmp.path().join("config.yaml");

    let fail_script = tmp.path().join("fail.py");
    fs::write(
        &fail_script,
        "#!/usr/bin/env python3\nimport sys\nsys.exit(1)\n",
    )
    .unwrap();

    let decode = hook_script("decode.py");
    fs::write(
        &config_path,
        format!(
            r#"
backend:
  type: file
  workspace_dir: {}
hooks:
  read:
    command: ["python3", "{}"]
  write:
    command: ["python3", "{}"]
"#,
            data.display(),
            decode.display(),
            fail_script.display()
        ),
    )
    .unwrap();

    let config = Config::load_from_path(&config_path).unwrap();
    let backend = open_scoped_backend(&config, SessionScope::default()).unwrap();

    let err = backend
        .write("fail.txt", None, "hello", "agent", "", IoOptions::default())
        .unwrap_err();
    assert!(matches!(err, WsError::Other(_)));
    assert!(!workspace_dir.join("fail.txt").exists());
}
