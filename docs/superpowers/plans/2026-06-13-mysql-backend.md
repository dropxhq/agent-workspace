# MySQL Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a MySQL storage backend selectable via `backend` in `config.yaml`, with identical CLI behavior to the existing file backend.

**Architecture:** Introduce a `WorkspaceBackend` trait with `FileBackend` and `MySqlBackend` implementations. `Config::load()` parses a `BackendConfig` enum (`file` | `mysql`). Commands become thin wrappers that parse CLI args, call the backend, and format output. MySQL stores content + metadata in a single `workspace_files` InnoDB table with row-level locking for writes.

**Tech Stack:** Rust 2021, `sqlx` (mysql + runtime-tokio), `tokio`, existing `serde`/`serde_yaml`/`chrono`/`sha2`/`clap`

**Spec:** `docs/superpowers/specs/2026-06-13-mysql-backend-design.md`

---

## File Structure

| File | Responsibility |
|------|----------------|
| `src/backend/mod.rs` | `WorkspaceBackend` trait, `BackendHandle` enum, `open_backend()` factory |
| `src/backend/file.rs` | File + sidecar implementation (logic moved from commands) |
| `src/backend/mysql.rs` | Connection pool, schema DDL, CRUD with transactions |
| `src/backend/path.rs` | Shared path helpers: normalize, metadata guard, list scope prefix |
| `src/config.rs` | `BackendConfig` enum parsing, legacy-free `Config::load()` |
| `src/commands/read.rs` | Parse ranges/human flags, call `backend.read()`, print output |
| `src/commands/write.rs` | Parse args, call `backend.write()` |
| `src/commands/list.rs` | `ListReport` type + human/json printing; `build_report` moves to backends |
| `src/commands/remove.rs` | Call `backend.remove()` |
| `src/commands/init.rs` | Branch: file template vs mysql template + schema bootstrap |
| `src/main.rs` | `open_backend()` once, pass `&BackendHandle` to commands |
| `src/lib.rs` | Export `backend` module |
| `config.yaml` | Update to new `backend.type: file` shape |
| `tests/integration.rs` | Adapt to `BackendHandle`; add mysql tests behind env gate |
| `README.md` | Document both backend configs |

**Breaking change (v1):** Remove top-level `workspace_dir` / `metadata_suffix` without `backend` wrapper. `init` and repo `config.yaml` use new format only.

---

### Task 1: Shared path helpers

**Files:**
- Create: `src/backend/path.rs`
- Modify: `src/backend/mod.rs` (stub)
- Modify: `src/lib.rs`

- [ ] **Step 1: Create `src/backend/mod.rs` stub**

```rust
pub mod file;
pub mod mysql;
pub mod path;
```

- [ ] **Step 2: Create `src/backend/path.rs`**

```rust
use crate::workspace::normalize_workspace_relative;

pub fn normalize_input_path(input: &str) -> String {
    normalize_workspace_relative(input)
}

pub fn is_metadata_path(relative: &str, metadata_suffix: &str) -> bool {
    relative.ends_with(metadata_suffix)
}

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
```

- [ ] **Step 3: Register module in `src/lib.rs`**

Add `pub mod backend;` alongside existing modules.

- [ ] **Step 4: Run tests**

Run: `cargo test backend::path -- --nocapture`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/backend/mod.rs src/backend/path.rs src/lib.rs
git commit -m "refactor: add shared backend path helpers"
```

---

### Task 2: BackendConfig parsing

**Files:**
- Modify: `src/config.rs`
- Modify: `config.yaml`
- Test: `src/config.rs` (unit tests)

- [ ] **Step 1: Write failing config parse test**

Add to bottom of `src/config.rs`:

```rust
#[cfg(test)]
mod config_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn parses_file_backend_config() {
        let tmp = TempDir::new().unwrap();
        let data = tmp.path().join("data");
        fs::create_dir_all(&data).unwrap();
        let cfg_path = tmp.path().join("config.yaml");
        fs::write(
            &cfg_path,
            r#"
backend:
  type: file
  workspace_dir: ./data
  metadata_suffix: ".meta.yaml"
"#,
        )
        .unwrap();
        let config = Config::load_from_path(&cfg_path).unwrap();
        assert!(matches!(config.backend, BackendConfig::File { .. }));
    }
}
```

Temporarily add `load_from_path` stub that returns error so test fails compile or run.

- [ ] **Step 2: Replace `Config` with `BackendConfig` enum**

Replace contents of `src/config.rs` with:

```rust
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{WsError, WsResult};

const DEFAULT_METADATA_SUFFIX: &str = ".meta.yaml";
const DEFAULT_MYSQL_PORT: u16 = 3306;

#[derive(Debug, Clone)]
pub struct Config {
    pub config_path: PathBuf,
    pub backend: BackendConfig,
}

#[derive(Debug, Clone)]
pub enum BackendConfig {
    File {
        workspace_dir: PathBuf,
        metadata_suffix: String,
    },
    Mysql {
        host: String,
        port: u16,
        user: String,
        password: String,
        database: String,
    },
}

fn default_metadata_suffix() -> String {
    DEFAULT_METADATA_SUFFIX.to_string()
}

fn default_mysql_port() -> u16 {
    DEFAULT_MYSQL_PORT
}

#[derive(Debug, Deserialize)]
struct RawFileBackend {
    #[serde(default = "default_type_file")]
    r#type: String,
    workspace_dir: PathBuf,
    #[serde(default = "default_metadata_suffix")]
    metadata_suffix: String,
}

fn default_type_file() -> String {
    "file".to_string()
}

#[derive(Debug, Deserialize)]
struct RawMysqlBackend {
    #[serde(default = "default_type_mysql")]
    r#type: String,
    host: String,
    #[serde(default = "default_mysql_port")]
    port: u16,
    user: String,
    password: String,
    database: String,
}

fn default_type_mysql() -> String {
    "mysql".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawBackend {
    Wrapped { backend: RawBackendInner },
    File(RawFileBackend),
    Mysql(RawMysqlBackend),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawBackendInner {
    File(RawFileBackend),
    Mysql(RawMysqlBackend),
}

impl Config {
    pub fn load() -> WsResult<Self> {
        let config_path = resolve_config_path()?;
        Self::load_from_path(&config_path)
    }

    pub fn load_from_path(config_path: &Path) -> WsResult<Self> {
        let contents = fs::read_to_string(config_path).map_err(|e| {
            WsError::Other(format!(
                "failed to read config {}: {e}",
                config_path.display()
            ))
        })?;

        let raw: RawBackend = serde_yaml::from_str(&contents).map_err(|e| {
            WsError::Other(format!(
                "failed to parse config {}: {e}",
                config_path.display()
            ))
        })?;

        let config_dir = config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let backend = match raw {
            RawBackend::Wrapped { backend } => match backend {
                RawBackendInner::File(f) => parse_file_backend(f, &config_dir, config_path)?,
                RawBackendInner::Mysql(m) => parse_mysql_backend(m)?,
            },
            RawBackend::File(f) => parse_file_backend(f, &config_dir, config_path)?,
            RawBackend::Mysql(m) => parse_mysql_backend(m)?,
        };

        Ok(Config {
            config_path: config_path.to_path_buf(),
            backend,
        })
    }
}

fn parse_file_backend(
    raw: RawFileBackend,
    config_dir: &Path,
    config_path: &Path,
) -> WsResult<BackendConfig> {
    if raw.r#type != "file" {
        return Err(WsError::Other(format!(
            "unknown backend type '{}' in {}",
            raw.r#type,
            config_path.display()
        )));
    }

    let workspace_dir = if raw.workspace_dir.is_absolute() {
        raw.workspace_dir
    } else {
        config_dir.join(raw.workspace_dir)
    };

    let workspace_dir = fs::canonicalize(&workspace_dir).map_err(|e| {
        WsError::Other(format!(
            "workspace_dir {} does not exist or is inaccessible: {e}",
            workspace_dir.display()
        ))
    })?;

    if !workspace_dir.is_dir() {
        return Err(WsError::Other(format!(
            "workspace_dir {} is not a directory",
            workspace_dir.display()
        )));
    }

    let test_file = workspace_dir.join(".ws_write_test");
    fs::write(&test_file, b"").map_err(|e| {
        WsError::Other(format!(
            "workspace_dir {} is not writable: {e}",
            workspace_dir.display()
        ))
    })?;
    let _ = fs::remove_file(test_file);

    Ok(BackendConfig::File {
        workspace_dir,
        metadata_suffix: raw.metadata_suffix,
    })
}

fn parse_mysql_backend(raw: RawMysqlBackend) -> WsResult<BackendConfig> {
    if raw.r#type != "mysql" {
        return Err(WsError::Other(format!(
            "unknown backend type '{}', expected 'mysql'",
            raw.r#type
        )));
    }
  if raw.host.is_empty() || raw.user.is_empty() || raw.database.is_empty() {
        return Err(WsError::Other(
            "mysql backend requires host, user, and database".to_string(),
        ));
    }

    Ok(BackendConfig::Mysql {
        host: raw.host,
        port: raw.port,
        user: raw.user,
        password: raw.password,
        database: raw.database,
    })
}

fn resolve_config_path() -> WsResult<PathBuf> {
    if let Ok(path) = env::var("AGENT_WORKSPACE_CONFIG") {
        let path = PathBuf::from(path);
        if !path.is_file() {
            return Err(WsError::Other(format!(
                "AGENT_WORKSPACE_CONFIG points to non-existent file: {}",
                path.display()
            )));
        }
        return Ok(path);
    }

    let cwd_config = env::current_dir()
        .map_err(WsError::Io)?
        .join("config.yaml");
    if !cwd_config.is_file() {
        return Err(WsError::Other(format!(
            "config not found: set AGENT_WORKSPACE_CONFIG or place config.yaml in cwd ({})",
            cwd_config.display()
        )));
    }
    Ok(cwd_config)
}
```

- [ ] **Step 3: Update repo `config.yaml`**

```yaml
backend:
  type: file
  workspace_dir: ./data
  metadata_suffix: ".meta.yaml"
```

- [ ] **Step 4: Run tests**

Run: `cargo test config_tests -- --nocapture`  
Expected: PASS (other tests may fail until later tasks fix callers)

- [ ] **Step 5: Commit**

```bash
git add src/config.rs config.yaml
git commit -m "feat: parse backend config for file and mysql"
```

---

### Task 3: WorkspaceBackend trait + BackendHandle

**Files:**
- Modify: `src/backend/mod.rs`
- Modify: `src/commands/list.rs` (move `ListReport` here or to `backend/mod.rs`)

- [ ] **Step 1: Move `ListReport` to `src/backend/mod.rs`**

In `src/backend/mod.rs`:

```rust
use crate::commands::ranges::LineRange;
use crate::error::WsResult;
use crate::meta::FileMetadata;

pub mod file;
pub mod mysql;
pub mod path;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ListReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub file_count: usize,
    pub total_size_bytes: u64,
    pub files: Vec<FileMetadata>,
}

pub trait WorkspaceBackend {
    fn read(
        &self,
        path: &str,
        ranges: Option<&[crate::commands::ranges::LineRange]>,
    ) -> WsResult<String>;

    fn write(
        &self,
        path: &str,
        ranges: Option<&LineRange>,
        content: &str,
        created_by: &str,
        desc: &str,
    ) -> WsResult<()>;

    fn list(&self, scope: Option<&str>) -> WsResult<ListReport>;

    fn remove(&self, path: &str) -> WsResult<()>;
}

pub enum BackendHandle {
    File(file::FileBackend),
    Mysql(mysql::MySqlBackend),
}

impl WorkspaceBackend for BackendHandle {
    fn read(&self, path: &str, ranges: Option<&[crate::commands::ranges::LineRange]>) -> WsResult<String> {
        match self {
            BackendHandle::File(b) => b.read(path, ranges),
            BackendHandle::Mysql(b) => b.read(path, ranges),
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
        match self {
            BackendHandle::File(b) => b.write(path, ranges, content, created_by, desc),
            BackendHandle::Mysql(b) => b.write(path, ranges, content, created_by, desc),
        }
    }

    fn list(&self, scope: Option<&str>) -> WsResult<ListReport> {
        match self {
            BackendHandle::File(b) => b.list(scope),
            BackendHandle::Mysql(b) => b.list(scope),
        }
    }

    fn remove(&self, path: &str) -> WsResult<()> {
        match self {
            BackendHandle::File(b) => b.remove(path),
            BackendHandle::Mysql(b) => b.remove(path),
        }
    }
}

pub fn open_backend(config: &crate::config::Config) -> WsResult<BackendHandle> {
    match &config.backend {
        crate::config::BackendConfig::File {
            workspace_dir,
            metadata_suffix,
        } => Ok(BackendHandle::File(file::FileBackend::new(
            workspace_dir.clone(),
            metadata_suffix.clone(),
        ))),
        crate::config::BackendConfig::Mysql {
            host,
            port,
            user,
            password,
            database,
        } => Ok(BackendHandle::Mysql(
            mysql::MySqlBackend::connect(host, *port, user, password, database)?,
        )),
    }
}
```

- [ ] **Step 2: Add stub `FileBackend` and `MySqlBackend`**

`src/backend/file.rs`:

```rust
use crate::backend::{ListReport, WorkspaceBackend};
use crate::commands::ranges::LineRange;
use crate::error::{WsError, WsResult};
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
    fn read(&self, _path: &str, _ranges: Option<&[crate::commands::ranges::LineRange]>) -> WsResult<String> {
        Err(WsError::Other("FileBackend::read not implemented".into()))
    }
    fn write(&self, _path: &str, _ranges: Option<&LineRange>, _content: &str, _created_by: &str, _desc: &str) -> WsResult<()> {
        Err(WsError::Other("FileBackend::write not implemented".into()))
    }
    fn list(&self, _scope: Option<&str>) -> WsResult<ListReport> {
        Err(WsError::Other("FileBackend::list not implemented".into()))
    }
    fn remove(&self, _path: &str) -> WsResult<()> {
        Err(WsError::Other("FileBackend::remove not implemented".into()))
    }
}
```

`src/backend/mysql.rs` — same stub pattern with `MySqlBackend::connect(...) -> WsResult<Self>` returning stub.

- [ ] **Step 3: Update `src/commands/list.rs`**

Re-export `ListReport` from backend:

```rust
pub use crate::backend::ListReport;
```

Remove local `ListReport` struct definition; keep `print_human` and `matches_scope` tests (move `matches_scope` test to `backend/path.rs` if duplicate).

- [ ] **Step 4: Verify compile**

Run: `cargo check`  
Expected: compiles with stubs

- [ ] **Step 5: Commit**

```bash
git add src/backend/ src/commands/list.rs
git commit -m "feat: add WorkspaceBackend trait and BackendHandle"
```

---

### Task 4: Implement FileBackend (move existing logic)

**Files:**
- Modify: `src/backend/file.rs`
- Reference: `src/commands/read.rs`, `write.rs`, `list.rs`, `remove.rs`, `workspace.rs`, `meta.rs`, `lock.rs`

- [ ] **Step 1: Implement `FileBackend::read`**

```rust
fn read(&self, path: &str, ranges: Option<&[LineRange]>) -> WsResult<String> {
    let config = self.as_config();
    let resolved = parse_ws_path(path, &config)?;

    if backend_path::is_metadata_path(&resolved.relative, &self.metadata_suffix) {
        return Err(WsError::NotFound(resolved.relative));
    }
    if !resolved.absolute.is_file() {
        return Err(WsError::NotFound(resolved.relative));
    }

    let _lock = FileLock::shared(&resolved.absolute)?;
    let content = fs::read_to_string(&resolved.absolute).map_err(WsError::Io)?;

    if let Some(ranges) = ranges {
        let filtered = filter_lines(&content, ranges);
        Ok(filtered)
    } else {
        Ok(content)
    }
}
```

Add private helpers on `FileBackend`:
- `as_config()` builds temporary `Config` shim OR pass `workspace_dir`/`metadata_suffix` directly to workspace helpers (preferred: add `parse_ws_path_for_backend(workspace_dir, path)` helpers to avoid fake Config).
- `filter_lines(content, ranges)` — extract from `read.rs` `print_raw_filtered` logic returning `String`.

**Recommended approach:** Add to `workspace.rs`:

```rust
pub fn parse_ws_path_in(workspace_dir: &Path, metadata_suffix: &str, input: &str) -> WsResult<ResolvedPath> { ... }
```

Mirror existing `parse_ws_path` but take `workspace_dir` instead of `Config`.

- [ ] **Step 2: Implement `FileBackend::write`**

Move body from `commands/write.rs::run` into `FileBackend::write`, using `parse_ws_path_for_write_in(...)`.

- [ ] **Step 3: Implement `FileBackend::list`**

Move `build_report` logic from `commands/list.rs` into `FileBackend::list`, using `backend::path::list_scope_prefix` and `path_matches_scope`.

- [ ] **Step 4: Implement `FileBackend::remove`**

Move body from `commands/remove.rs::run`.

- [ ] **Step 5: Run existing integration tests (will fail until Task 5)**

Run: `cargo test`  
Fix `workspace.rs` helpers and any `Config` field access in tests.

- [ ] **Step 6: Commit**

```bash
git add src/backend/file.rs src/workspace.rs
git commit -m "feat: implement FileBackend with existing file logic"
```

---

### Task 5: Thin command layer + main.rs

**Files:**
- Modify: `src/commands/read.rs`, `write.rs`, `list.rs`, `remove.rs`
- Modify: `src/main.rs`
- Modify: `tests/integration.rs`

- [ ] **Step 1: Refactor `read.rs`**

```rust
pub fn run(
    path: &str,
    ranges: Option<&str>,
    human: bool,
    backend: &BackendHandle,
) -> WsResult<()> {
    let parsed_ranges = ranges.map(parse_ranges).transpose()?;
    let content = backend.read(path, parsed_ranges.as_deref())?;

    if human {
        let relative = backend_path::normalize_input_path(path);
        print_human(&relative, &content, parsed_ranges.as_deref())?;
    } else {
        print!("{content}");
    }
    Ok(())
}
```

Keep `print_human` in `read.rs`.

- [ ] **Step 2: Refactor `write.rs`, `list.rs`, `remove.rs` similarly**

`write.rs::run(..., backend: &BackendHandle)`  
`list.rs::run(..., backend: &BackendHandle)` — call `backend.list`, then print  
`remove.rs::run(..., backend: &BackendHandle)`

- [ ] **Step 3: Update `main.rs`**

```rust
fn run() -> Result<(), WsError> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init { path } => commands::init::run(path.as_deref()),
        command => {
            let config = Config::load()?;
            let backend = open_backend(&config)?;
            dispatch(command, &backend)
        }
    }
}

fn dispatch(command: Commands, backend: &BackendHandle) -> Result<(), WsError> { ... }
```

- [ ] **Step 4: Update `tests/integration.rs`**

Replace manual `Config { workspace_dir, ... }` with:

```rust
fn setup_file_backend() -> (TempDir, BackendHandle) {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("data");
    fs::create_dir_all(&workspace).unwrap();
    let backend = FileBackend::new(
        fs::canonicalize(&workspace).unwrap(),
        ".meta.yaml".to_string(),
    );
    (tmp, BackendHandle::File(backend))
}
```

Update all command calls to pass `&backend` instead of `&config`.

- [ ] **Step 5: Run full test suite**

Run: `cargo test`  
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add src/commands/ src/main.rs tests/integration.rs
git commit -m "refactor: route commands through WorkspaceBackend"
```

---

### Task 6: Add sqlx + tokio dependencies

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/error.rs`

- [ ] **Step 1: Add dependencies**

```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "mysql", "chrono"] }
tokio = { version = "1", features = ["rt-multi-thread"] }
```

- [ ] **Step 2: Map DB lock errors in `error.rs`**

Add helper in `mysql.rs` (not error.rs) to map sqlx errors containing "Lock wait timeout" or MySQL error 1205/1213 to `WsError::LockConflict`.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "build: add sqlx and tokio for mysql backend"
```

---

### Task 7: MySQL schema + connection

**Files:**
- Modify: `src/backend/mysql.rs`

- [ ] **Step 1: Define DDL constant**

```rust
const CREATE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS workspace_files (
    relative_path   VARCHAR(1024)  NOT NULL PRIMARY KEY,
    content         LONGTEXT       NOT NULL,
    created_by      VARCHAR(255)   NOT NULL DEFAULT '',
    description     TEXT           NOT NULL,
    created_at      DATETIME(6)    NOT NULL,
    updated_at      DATETIME(6)    NOT NULL,
    size_bytes      BIGINT UNSIGNED NOT NULL,
    sha256          CHAR(64)       NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci
"#;
```

- [ ] **Step 2: Implement `MySqlBackend` struct**

```rust
pub struct MySqlBackend {
    pool: sqlx::MySqlPool,
}

impl MySqlBackend {
    pub fn connect(host: &str, port: u16, user: &str, password: &str, database: &str) -> WsResult<Self> {
        let url = format!(
            "mysql://{user}:{password}@{host}:{port}/{database}"
        );
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(3)
            .connect_lazy(&url)
            .map_err(|e| WsError::Other(format!("mysql connect failed: {e}")))?;
        Ok(Self { pool })
    }

    pub async fn ensure_schema(pool: &sqlx::MySqlPool, database: &str) -> WsResult<()> {
        sqlx::query(&format!("CREATE DATABASE IF NOT EXISTS `{database}`"))
            .execute(pool)
            .await
            .map_err(map_db_err)?;
        sqlx::query(CREATE_TABLE_SQL)
            .execute(pool)
            .await
            .map_err(map_db_err)?;
        Ok(())
    }
}
```

Use `tokio::runtime::Runtime::new()` in `connect` to block_on `pool` health check `SELECT 1` on first open.

- [ ] **Step 3: Commit**

```bash
git add src/backend/mysql.rs
git commit -m "feat: mysql connection and schema DDL"
```

---

### Task 8: MySQL read + remove

**Files:**
- Modify: `src/backend/mysql.rs`

- [ ] **Step 1: Implement `read`**

```rust
fn read(&self, path: &str, ranges: Option<&[LineRange]>) -> WsResult<String> {
    let relative = backend_path::normalize_input_path(path);
    if backend_path::is_metadata_path(&relative, ".meta.yaml") {
        return Err(WsError::NotFound(relative));
    }

    let row: Option<(String,)> = self.block_on(async {
        sqlx::query_as::<_, (String,)>(
            "SELECT content FROM workspace_files WHERE relative_path = ?"
        )
        .bind(&relative)
        .fetch_optional(&self.pool)
        .await
    })?;

    let content = match row {
        Some((c,)) => c,
        None => return Err(WsError::NotFound(relative)),
    };

    if let Some(ranges) = ranges {
        Ok(filter_lines(&content, ranges))
    } else {
        Ok(content)
    }
}
```

Share `filter_lines` via small `src/backend/content.rs` or duplicate minimally in file.rs/mysql.rs.

- [ ] **Step 2: Implement `remove`**

Transaction:

```sql
DELETE FROM workspace_files WHERE relative_path = ?
```

If `rows_affected() == 0` → `NotFound`.

- [ ] **Step 3: Manual smoke test** (if local MySQL available)

```bash
# after init task creates mysql config
ws write docs/t.txt --content "hi" --created-by me
ws read docs/t.txt
ws remove docs/t.txt
```

- [ ] **Step 4: Commit**

```bash
git add src/backend/mysql.rs
git commit -m "feat: mysql read and remove"
```

---

### Task 9: MySQL write + list

**Files:**
- Modify: `src/backend/mysql.rs`

- [ ] **Step 1: Implement `write` with transaction**

Inside transaction:

1. `SELECT content, created_by, created_at FROM workspace_files WHERE relative_path = ? FOR UPDATE`
2. Merge content (full or range via `apply_write_ranges`)
3. Build metadata: preserve `created_by`/`created_at` on update
4. `INSERT ... ON DUPLICATE KEY UPDATE` OR separate insert/update branches

```rust
let now = meta::now_local();
let size_bytes = final_content.len() as u64;
let sha256 = meta::compute_sha256(final_content.as_bytes());

// INSERT or UPDATE with all columns
```

Map lock timeout errors to `LockConflict`.

- [ ] **Step 2: Implement `list`**

```rust
let scope_prefix = backend_path::list_scope_prefix(scope);
let rows = if let Some(prefix) = &scope_prefix {
    sqlx::query_as::<_, FileRow>(
        "SELECT relative_path, created_by, description, created_at, updated_at, size_bytes, sha256
         FROM workspace_files WHERE relative_path LIKE ?"
    )
    .bind(format!("{prefix}%"))
    .fetch_all(&self.pool)
    .await?
} else {
    sqlx::query_as::<_, FileRow>( "... without WHERE ...").fetch_all(...)?
};
```

Map `FileRow` to `FileMetadata` (`description` → `desc` via serde rename or manual mapping):

```rust
struct FileRow {
    relative_path: String,
    created_by: String,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    size_bytes: u64,
    sha256: Option<String>,
}
```

Convert timestamps to `FixedOffset` for JSON compatibility with file backend.

- [ ] **Step 3: Commit**

```bash
git add src/backend/mysql.rs
git commit -m "feat: mysql write and list"
```

---

### Task 10: Init command — file + mysql templates

**Files:**
- Modify: `src/commands/init.rs`

- [ ] **Step 1: Add `--backend` flag to CLI**

In `main.rs` `Init` subcommand:

```rust
Init {
    path: Option<String>,
    /// Backend type: file or mysql
    #[arg(long, default_value = "file")]
    backend: String,
},
```

Pass to `init::run(path, backend_type)`.

- [ ] **Step 2: File init template (update DEFAULT_CONFIG)**

```rust
const DEFAULT_FILE_CONFIG: &str = r#"backend:
  type: file
  workspace_dir: ./data
  metadata_suffix: ".meta.yaml"
"#;
```

- [ ] **Step 3: MySQL init template**

```rust
const DEFAULT_MYSQL_CONFIG: &str = r#"backend:
  type: mysql
  host: localhost
  port: 3306
  user: ws_user
  password: change_me
  database: agent_workspace
"#;
```

When `backend == "mysql"`: write mysql template, do NOT create `data/`. Then load config and call `MySqlBackend::ensure_schema`.

When `backend == "file"`: existing behavior with new template.

- [ ] **Step 4: Update init tests**

```rust
#[test]
fn init_mysql_writes_config_without_data_dir() { ... }

#[test]
fn init_file_still_creates_data_dir() { ... }
```

- [ ] **Step 5: Run tests**

Run: `cargo test init -- --nocapture`  
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/commands/init.rs src/main.rs
git commit -m "feat: init supports file and mysql backend templates"
```

---

### Task 11: MySQL integration tests (env-gated)

**Files:**
- Create: `tests/mysql_integration.rs`

- [ ] **Step 1: Add gated integration test file**

```rust
//! Run with: MYSQL_TEST_URL='mysql://user:pass@localhost:3306/agent_workspace_test' cargo test --test mysql_integration -- --ignored

use std::env;

fn mysql_backend() -> Option<agent_workspace::backend::BackendHandle> {
    let url = env::var("MYSQL_TEST_URL").ok()?;
  // parse or use fixed test credentials from env
    None // implement full setup
}

#[test]
#[ignore]
fn mysql_write_read_remove_lifecycle() {
    let backend = mysql_backend().expect("MYSQL_TEST_URL not set");
    // mirror tests/integration.rs lifecycle
}
```

- [ ] **Step 2: Document in README**

- [ ] **Step 3: Commit**

```bash
git add tests/mysql_integration.rs README.md
git commit -m "test: add optional mysql integration tests"
```

---

### Task 12: README documentation

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update Configuration section**

Document both `backend.type: file` and `backend.type: mysql` blocks per spec. Note breaking change from old top-level `workspace_dir`.

- [ ] **Step 2: Update Init section**

Document `ws init --backend mysql` and schema auto-creation.

- [ ] **Step 3: Add MySQL concurrency note**

Row locks vs file advisory locks; same exit codes.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: document mysql backend configuration"
```

---

## Plan Self-Review

### Spec coverage

| Spec section | Task |
|--------------|------|
| Backend config (file/mysql) | Task 2 |
| WorkspaceBackend trait | Task 3 |
| FileBackend migration | Task 4 |
| MySQL schema + init | Task 7, 10 |
| read/write/list/remove mapping | Task 8, 9 |
| Concurrency / exit codes | Task 6, 8, 9 |
| Path rules | Task 1, 4, 8 |
| Testing | Task 5, 11 |
| README | Task 12 |
| Legacy config breaking change | Task 2, 12 |

### Placeholder scan

No TBD/TODO steps. Each task includes concrete code or SQL.

### Type consistency

- `ListReport` lives in `backend/mod.rs`; commands re-export.
- `BackendHandle` used consistently in commands and tests after Task 5.
- `FileMetadata.desc` mapped from MySQL `description` column in Task 9.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-13-mysql-backend.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks, fast iteration
2. **Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach do you want?
