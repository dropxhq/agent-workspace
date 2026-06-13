# MySQL Backend Design

**Date:** 2026-06-13  
**Status:** Approved  
**Project:** agent-workspace (`ws` CLI)

## Summary

Extend agent-workspace to support a MySQL storage backend in addition to the existing local file backend. Backend selection is configured via a `backend` object in `config.yaml`. The CLI surface (`read`, `write`, `list`, `remove`, `init`) remains unchanged; MySQL is transparent to users.

### Goals

- Multi-agent / multi-machine shared workspace via centralized MySQL storage
- Database-backed metadata for query, audit, and backup (structured columns, no sidecar files)
- Transparent backend switch: same commands, same output formats, same exit codes

### Non-Goals (v1)

- File ↔ MySQL migration tooling
- New CLI commands (`query`, `audit`, `export`, etc.)
- Encrypted credentials in config (plaintext in `config.yaml`; deployment-managed)
- Read replicas, sharding, or multi-database routing

## Requirements Traceability

| User choice | Design decision |
|-------------|-----------------|
| Use case C (shared + DB benefits) | Single `workspace_files` table with indexed path + metadata columns; InnoDB transactions |
| CLI scope A (transparent switch) | `WorkspaceBackend` trait; commands unchanged at CLI level |
| Credentials A (all in config.yaml) | `backend.mysql` block with host, port, user, password, database |
| Init A (auto schema) | `ws init` creates database and table if missing |
| Config C (separate structures) | `backend.type: file` vs `backend.type: mysql`; no shared `workspace_dir` at top level |

## Configuration

### File backend

```yaml
backend:
  type: file
  workspace_dir: ./data
  metadata_suffix: ".meta.yaml"   # optional, default ".meta.yaml"
```

### MySQL backend

```yaml
backend:
  type: mysql
  host: localhost
  port: 3306          # optional, default 3306
  user: ws_user
  password: secret
  database: agent_workspace
```

### Parsing

- `Config::load()` deserializes into `BackendConfig` enum (`File` | `Mysql`).
- Factory: `backend_from_config(&config)` returns `BackendHandle` (enum wrapping implementations).
- **Legacy config:** Top-level `workspace_dir` without `backend` may be supported for one release by mapping to `backend.type: file`. If not implemented in v1, document breaking change and update default `config.yaml` template.

### MySQL mode

- No `workspace_dir` validation or local directory creation.
- Connection validated at startup (ping or simple query).

## Architecture

### Module layout

```
src/
  backend/
    mod.rs          # WorkspaceBackend trait, BackendHandle enum
    file.rs         # FileBackend (fs + sidecar logic moved here)
    mysql.rs        # MySqlBackend
  config.rs         # BackendConfig parsing
  commands/         # Thin layer: parse args, call backend, format output
```

### WorkspaceBackend trait

```rust
pub trait WorkspaceBackend {
    fn read(&self, path: &str, ranges: Option<&ParsedRanges>) -> WsResult<String>;
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
```

- Reuse existing types: `LineRange`, `ListReport`, `FileMetadata`, range parsing in `commands/ranges.rs`.
- `main.rs`: load config → build backend → dispatch commands with `&BackendHandle`.

### Migration from current code

- Move filesystem operations from `commands/read.rs`, `write.rs`, `list.rs`, `remove.rs` into `backend/file.rs`.
- Commands retain CLI output formatting (`--human`, `--json`).
- `meta.rs` sidecar helpers remain used by `FileBackend`; `MySqlBackend` maps rows to `FileMetadata`.

## MySQL Schema

### Table: `workspace_files`

```sql
CREATE TABLE IF NOT EXISTS workspace_files (
    relative_path   VARCHAR(1024)  NOT NULL PRIMARY KEY,
    content         LONGTEXT       NOT NULL,
    created_by      VARCHAR(255)   NOT NULL DEFAULT '',
    description     TEXT           NOT NULL,
    created_at      DATETIME(6)    NOT NULL,
    updated_at      DATETIME(6)    NOT NULL,
    size_bytes      BIGINT UNSIGNED NOT NULL,
    sha256          CHAR(64)       NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
```

- Single table: content and metadata updated atomically in one transaction.
- `relative_path` is the logical workspace path (e.g. `docs/foo.txt`).
- Column `description` maps to YAML/JSON field `desc` in API output.

### Init (`ws init`)

For MySQL template generation:

1. Optional `[path]` argument: create directory and `config.yaml` with `backend.type: mysql` placeholders.
2. When run against existing MySQL config (or dedicated init path — implementation detail):
   - Connect to MySQL server.
   - `CREATE DATABASE IF NOT EXISTS <database>`.
   - `USE <database>`.
   - `CREATE TABLE IF NOT EXISTS workspace_files (...)` as above.
3. Do not create local `data/` directory.

Requires MySQL account with `CREATE` privilege on database (and `CREATE TABLE`).

## Operations Mapping

### read

- **File:** shared `FileLock`, read file, apply line ranges.
- **MySQL:** `SELECT content FROM workspace_files WHERE relative_path = ?`; apply ranges in memory.
- Metadata paths (`*.meta.yaml`) → `NotFound` (exit 3).

### write

- **File:** exclusive lock, range merge or full replace, write file + sidecar.
- **MySQL:** transaction:
  1. `SELECT content, created_by, created_at FROM workspace_files WHERE relative_path = ? FOR UPDATE`
  2. Merge content (ranges or full replace).
  3. Preserve `created_by` / `created_at` on update; set on insert.
  4. Compute `size_bytes`, `sha256`, `updated_at`.
  5. `INSERT` or `UPDATE` single row; `COMMIT`.

### list

- **File:** walk `workspace_dir`, skip sidecars, read metadata from sidecar files.
- **MySQL:** `SELECT` metadata columns; scope `docs` → normalized prefix `docs/` → `WHERE relative_path LIKE 'docs/%'` (or `=` for exact file if needed — scope is directory only per current CLI).
- Sort by `relative_path`; JSON shape unchanged.

### remove

- **File:** exclusive lock, delete data file and sidecar.
- **MySQL:** transaction `DELETE FROM workspace_files WHERE relative_path = ?` (after `FOR UPDATE` or direct delete in transaction).

## Concurrency

| Operation | File backend | MySQL backend |
|-----------|--------------|---------------|
| read | Advisory shared lock (`fs4`) | Read-only SELECT |
| write | Advisory exclusive lock | Transaction + `SELECT FOR UPDATE` |
| remove | Advisory exclusive lock | Transaction + row lock |

- InnoDB row locks serialize concurrent writes to the same path across agents.
- Lock/deadlock timeout → `WsError` with exit code **4** (align with file lock conflicts).
- One transaction per CLI invocation; no cross-path batching in v1.

## Path Rules

### Shared (both backends)

- `normalize_workspace_relative()`: POSIX normalization (`../`, `./`, leading `/`).
- Metadata path protection on read/remove: paths ending with `metadata_suffix` → not found.
- Write range semantics unchanged (single `START-END`, 1-indexed inclusive).

### File only

- `validate_within_workspace()` and symlink escape checks.
- `create_dir_all` for parent paths and sidecar paths.

### MySQL only

- No filesystem escape validation.
- List scope: normalize scope to directory prefix (e.g. `docs` → `docs/`).
- New path on write → insert; existing → update with preserved creation fields.

## Error Handling & CLI Consistency

| Scenario | Exit code | Notes |
|----------|-----------|-------|
| Success | 0 | |
| General error (e.g. DB connection) | 1 | stderr: `error: ...` |
| Path escape (file backend) | 2 | |
| Not found | 3 | |
| Lock / lock wait timeout | 4 | |

- `list --json`: `scope`, `file_count`, `total_size_bytes`, `files[]` unchanged.
- `read --human` / `--ranges`: same output format.
- `FileMetadata` serde fields: `relative_path`, `created_by`, `desc`, `created_at`, `updated_at`, `size_bytes`, `sha256`.

## Dependencies

- Add MySQL client crate (recommended: `sqlx` with `mysql` feature).
- CLI is currently synchronous; use `sqlx` blocking pool or minimal async runtime — chosen during implementation without changing external behavior.

## Testing

1. **Unit tests:** path normalization, list prefix logic, write range merge (existing tests retained).
2. **FileBackend:** existing `tests/integration.rs` adapted to use `FileBackend` or unchanged paths through new abstraction.
3. **MySqlBackend integration:** optional `MYSQL_TEST_URL` or testcontainers; tests skipped when unavailable.
4. **Parity tests:** shared test cases run against both backends where environment allows.

## Open Implementation Notes

- Decide v1 legacy config: auto-map old `workspace_dir` top-level YAML vs breaking change only.
- `init` command: clarify whether MySQL schema creation runs from `ws init` in workspace dir with mysql config, or separate subcommand — default: same `init` reads config type and branches.
- Connection pool size: default small pool (e.g. 2–5) for CLI process lifetime.
