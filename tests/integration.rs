use std::fs;

use tempfile::TempDir;

use agent_workspace::backend::{file::FileBackend, BackendHandle, WorkspaceBackend};
use agent_workspace::workspace::{normalize_workspace_relative, parse_ws_path_in};

fn setup_workspace() -> (TempDir, std::path::PathBuf, BackendHandle) {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("data");
    fs::create_dir_all(&workspace).unwrap();
    let workspace_dir = fs::canonicalize(&workspace).unwrap();

    let backend = BackendHandle::File(FileBackend::new(
        workspace_dir.clone(),
        ".meta.yaml".to_string(),
    ));

    (tmp, workspace_dir, backend)
}

#[test]
fn write_read_remove_lifecycle() {
    let (_tmp, workspace_dir, backend) = setup_workspace();

    agent_workspace::commands::write::run(
        "docs/foo.txt",
        None,
        "agent-x",
        "test file",
        Some("hello\nworld\n"),
        &backend,
    )
    .unwrap();

    let data_path = workspace_dir.join("docs/foo.txt");
    assert!(data_path.is_file());
    assert!(workspace_dir.join("docs/foo.txt.meta.yaml").is_file());

    agent_workspace::commands::read::run("docs/foo.txt", None, false, &backend).unwrap();

    agent_workspace::commands::list::run(None, false, &backend).unwrap();

    agent_workspace::commands::remove::run("docs/foo.txt", &backend).unwrap();
    assert!(!data_path.exists());
    assert!(!workspace_dir.join("docs/foo.txt.meta.yaml").exists());
}

#[test]
fn write_with_ranges_partial_replace() {
    let (_tmp, workspace_dir, backend) = setup_workspace();

    agent_workspace::commands::write::run(
        "partial.txt",
        None,
        "agent",
        "",
        Some("a\nb\nc\n"),
        &backend,
    )
    .unwrap();

    agent_workspace::commands::write::run(
        "partial.txt",
        Some("2-2"),
        "",
        "",
        Some("B\n"),
        &backend,
    )
    .unwrap();

    let content = fs::read_to_string(workspace_dir.join("partial.txt")).unwrap();
    assert_eq!(content, "a\nB\nc\n");
}

#[test]
fn metadata_path_hidden_from_read_and_remove() {
    let (_tmp, workspace_dir, backend) = setup_workspace();

    let meta_relative = "secret.txt.meta.yaml";
    fs::write(
        workspace_dir.join(meta_relative),
        "relative_path: secret.txt\n",
    )
    .unwrap();

    let err =
        agent_workspace::commands::read::run(meta_relative, None, false, &backend).unwrap_err();
    assert!(matches!(err, agent_workspace::error::WsError::NotFound(_)));

    let err = agent_workspace::commands::remove::run(meta_relative, &backend).unwrap_err();
    assert!(matches!(err, agent_workspace::error::WsError::NotFound(_)));
}

#[test]
fn path_normalization_equivalence() {
    assert_eq!(
        normalize_workspace_relative("a/b/c.md"),
        normalize_workspace_relative("/a/b/c.md")
    );
    assert_eq!(
        normalize_workspace_relative("a/b/c.md"),
        normalize_workspace_relative("../a/b/c.md")
    );
    assert_eq!(normalize_workspace_relative("foo/../bar"), "bar");
    assert_eq!(normalize_workspace_relative("../etc/passwd"), "etc/passwd");
}

#[test]
fn symlink_escape_blocked() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("data");
    fs::create_dir_all(&workspace).unwrap();

    let outside = tmp.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("secret.txt"), "secret").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let link_path = workspace.join("escape-link");
        symlink(&outside, &link_path).unwrap();

        let resolved = parse_ws_path_in(
            fs::canonicalize(&workspace).unwrap().as_path(),
            "escape-link/secret.txt",
        );
        match resolved {
            Err(agent_workspace::error::WsError::PathEscape(_)) => {}
            other => panic!("expected PathEscape, got {other:?}"),
        }
    }
}

#[test]
fn metadata_preserves_created_fields_on_update() {
    let (_tmp, workspace_dir, backend) = setup_workspace();

    agent_workspace::commands::write::run(
        "keep.txt",
        None,
        "original-agent",
        "first",
        Some("v1\n"),
        &backend,
    )
    .unwrap();

    let meta1 = agent_workspace::meta::FileMetadata::read_from_sidecar(
        &workspace_dir.join("keep.txt.meta.yaml"),
    )
    .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(10));

    agent_workspace::commands::write::run(
        "keep.txt",
        None,
        "new-agent",
        "second",
        Some("v2\n"),
        &backend,
    )
    .unwrap();

    let meta2 = agent_workspace::meta::FileMetadata::read_from_sidecar(
        &workspace_dir.join("keep.txt.meta.yaml"),
    )
    .unwrap();

    assert_eq!(meta2.created_by, "original-agent");
    assert_eq!(meta2.created_at, meta1.created_at);
    assert_eq!(meta2.desc, "second");
}

#[test]
fn list_json_output() {
    let (_tmp, _workspace_dir, backend) = setup_workspace();

    agent_workspace::commands::write::run("a.txt", None, "agent", "", Some("x"), &backend).unwrap();

    agent_workspace::commands::list::run(None, true, &backend).unwrap();
}

#[test]
fn list_subdirectory_scope() {
    let (_tmp, _workspace_dir, backend) = setup_workspace();

    agent_workspace::commands::write::run("docs/a.txt", None, "agent", "", Some("a"), &backend)
        .unwrap();
    agent_workspace::commands::write::run("other/b.txt", None, "agent", "", Some("b"), &backend)
        .unwrap();

    let report = backend.list(Some("docs")).unwrap();
    assert_eq!(report.file_count, 1);
    assert_eq!(report.files[0].relative_path, "docs/a.txt");
    assert_eq!(report.scope.as_deref(), Some("docs"));

    let report = backend.list(None).unwrap();
    assert_eq!(report.file_count, 2);
    assert!(report.scope.is_none());
}

#[test]
fn read_with_ranges() {
    let (_tmp, _workspace_dir, backend) = setup_workspace();

    agent_workspace::commands::write::run(
        "lines.txt",
        None,
        "agent",
        "",
        Some("one\ntwo\nthree\nfour\n"),
        &backend,
    )
    .unwrap();

    agent_workspace::commands::read::run("lines.txt", Some("2-3"), false, &backend).unwrap();
}

#[test]
fn concurrent_writes_do_not_corrupt() {
    use std::sync::Arc;
    use std::thread;

    let (_tmp, workspace_dir, backend) = setup_workspace();
    let backend = Arc::new(backend);

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let backend = Arc::clone(&backend);
            thread::spawn(move || {
                agent_workspace::commands::write::run(
                    "concurrent.txt",
                    None,
                    "agent",
                    "",
                    Some(&format!("iteration {i}\n")),
                    &backend,
                )
                .unwrap();
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let content = fs::read_to_string(workspace_dir.join("concurrent.txt")).unwrap();
    assert!(content.starts_with("iteration "));
    assert!(content.ends_with('\n'));

    let meta = agent_workspace::meta::FileMetadata::read_from_sidecar(
        &workspace_dir.join("concurrent.txt.meta.yaml"),
    )
    .unwrap();
    assert_eq!(meta.size_bytes, content.len() as u64);
}

#[test]
fn init_creates_workspace_layout() {
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join("new-ws");

    agent_workspace::commands::init::run(Some(target.to_str().unwrap())).unwrap();

    assert!(target.join("config.yaml").is_file());
    assert!(target.join("data").is_dir());

    let err = agent_workspace::commands::init::run(Some(target.to_str().unwrap())).unwrap_err();
    assert!(matches!(err, agent_workspace::error::WsError::Other(_)));
}
