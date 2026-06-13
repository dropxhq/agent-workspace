//! Optional MySQL integration tests.
//!
//! Requires a running MySQL instance and `MYSQL_TEST_URL`, e.g.:
//! `mysql://user:pass@localhost:3306/agent_workspace_test`
//!
//! Run with:
//! ```bash
//! MYSQL_TEST_URL='mysql://user:pass@localhost:3306/agent_workspace_test' \
//!   cargo test --test mysql_integration -- --ignored
//! ```

use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use agent_workspace::backend::mysql::MySqlBackend;
use agent_workspace::backend::{BackendHandle, WorkspaceBackend};

struct MysqlTestConfig {
    host: String,
    port: u16,
    user: String,
    password: String,
    database: String,
}

fn parse_mysql_test_url(url: &str) -> Option<MysqlTestConfig> {
    let rest = url.strip_prefix("mysql://")?;
    let (auth, host_and_db) = match rest.split_once('@') {
        Some(pair) => pair,
        None => ("", rest),
    };

    let (user, password) = if auth.is_empty() {
        ("root".to_string(), String::new())
    } else {
        match auth.split_once(':') {
            Some((u, p)) => (u.to_string(), p.to_string()),
            None => (auth.to_string(), String::new()),
        }
    };

    let (host_port, database) = host_and_db.split_once('/')?;
    if database.is_empty() {
        return None;
    }

    let (host, port) = match host_port.split_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().ok()?),
        None => (host_port.to_string(), 3306),
    };

    Some(MysqlTestConfig {
        host,
        port,
        user,
        password,
        database: database.to_string(),
    })
}

fn mysql_backend() -> Option<BackendHandle> {
    let url = env::var("MYSQL_TEST_URL").ok()?;
    let cfg = parse_mysql_test_url(&url)?;
    let backend = MySqlBackend::connect(
        &cfg.host,
        cfg.port,
        &cfg.user,
        &cfg.password,
        &cfg.database,
    )
    .ok()?;
    Some(BackendHandle::Mysql(backend))
}

fn unique_test_path(suffix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("test/{suffix}_{nanos}.txt")
}

fn require_mysql_backend() -> BackendHandle {
    mysql_backend().unwrap_or_else(|| {
        panic!(
            "MYSQL_TEST_URL must be set to run ignored mysql integration tests, e.g. \
             mysql://user:pass@localhost:3306/agent_workspace_test"
        )
    })
}

#[test]
#[ignore = "requires MYSQL_TEST_URL and a running MySQL instance"]
fn mysql_write_read_remove_lifecycle() {
    let backend = require_mysql_backend();
    let path = unique_test_path("lifecycle");

    agent_workspace::commands::write::run(
        &path,
        None,
        "agent-x",
        "test file",
        Some("hello\nworld\n"),
        &backend,
    )
    .unwrap();

    let content = backend.read(&path, None).unwrap();
    assert_eq!(content, "hello\nworld\n");

    agent_workspace::commands::read::run(&path, None, false, &backend).unwrap();

    agent_workspace::commands::list::run(None, false, &backend).unwrap();
    let report = backend.list(None).unwrap();
    assert!(report.files.iter().any(|f| f.relative_path == path));

    agent_workspace::commands::remove::run(&path, &backend).unwrap();
    assert!(matches!(
        backend.read(&path, None),
        Err(agent_workspace::error::WsError::NotFound(_))
    ));
}

#[test]
#[ignore = "requires MYSQL_TEST_URL and a running MySQL instance"]
fn mysql_write_with_ranges_partial_replace() {
    let backend = require_mysql_backend();
    let path = unique_test_path("partial");

    agent_workspace::commands::write::run(
        &path,
        None,
        "agent",
        "",
        Some("a\nb\nc\n"),
        &backend,
    )
    .unwrap();

    agent_workspace::commands::write::run(
        &path,
        Some("2-2"),
        "",
        "",
        Some("B\n"),
        &backend,
    )
    .unwrap();

    let content = backend.read(&path, None).unwrap();
    assert_eq!(content, "a\nB\nc\n");

    agent_workspace::commands::remove::run(&path, &backend).unwrap();
}

#[test]
#[ignore = "requires MYSQL_TEST_URL and a running MySQL instance"]
fn mysql_metadata_path_hidden_from_read_and_remove() {
    let backend = require_mysql_backend();
    let meta_path = unique_test_path("secret").replace(".txt", ".meta.yaml");

    let err =
        agent_workspace::commands::read::run(&meta_path, None, false, &backend).unwrap_err();
    assert!(matches!(
        err,
        agent_workspace::error::WsError::NotFound(_)
    ));

    let err = agent_workspace::commands::remove::run(&meta_path, &backend).unwrap_err();
    assert!(matches!(
        err,
        agent_workspace::error::WsError::NotFound(_)
    ));
}

#[test]
#[ignore = "requires MYSQL_TEST_URL and a running MySQL instance"]
fn mysql_list_subdirectory_scope() {
    let backend = require_mysql_backend();
    let docs_path = unique_test_path("docs_a").replace("test/", "test/docs/");
    let other_path = unique_test_path("other_b").replace("test/", "test/other/");

    agent_workspace::commands::write::run(&docs_path, None, "agent", "", Some("a"), &backend)
        .unwrap();
    agent_workspace::commands::write::run(&other_path, None, "agent", "", Some("b"), &backend)
        .unwrap();

    let report = backend.list(Some("test/docs")).unwrap();
    assert_eq!(report.file_count, 1);
    assert_eq!(report.files[0].relative_path, docs_path);

    agent_workspace::commands::remove::run(&docs_path, &backend).unwrap();
    agent_workspace::commands::remove::run(&other_path, &backend).unwrap();
}
