## 1. 抽出 CLI 入口层

- [x] 1.1 新建 `src/cli.rs`，移入 `main.rs` 的 `Cli`/`Commands` 定义、`run`、`dispatch`、`scoped_backend`
- [x] 1.2 瘦身 `src/main.rs` 为仅 `fn main()` 调用 `agent_workspace::cli::run()`
- [x] 1.3 在 `lib.rs` 声明 `pub mod cli;`，`cargo build` 通过

## 2. metadata 领域改名

- [x] 2.1 将 `src/meta.rs` 内容迁至 `src/metadata.rs`（含 `compute_sha256`/`now_local`/`build_metadata`/sidecar 及其 tests）
- [x] 2.2 删除 `src/meta.rs`，`lib.rs` 改为 `pub mod metadata;`
- [x] 2.3 全局更新 `crate::meta::` → `crate::metadata::`，`cargo build` 通过

## 3. ranges 提升到顶层

- [x] 3.1 新建 `src/ranges.rs`，移入 `commands/ranges.rs` 全部内容
- [x] 3.2 将 `backend/content.rs` 的 `filter_lines`（含 test）并入 `src/ranges.rs`
- [x] 3.3 删除 `src/commands/ranges.rs` 与 `src/backend/content.rs`
- [x] 3.4 更新引用：`crate::commands::ranges::` → `crate::ranges::`，`crate::backend::content::filter_lines` → `crate::ranges::filter_lines`；从 `commands/mod.rs` 移除 `pub mod ranges;`，`lib.rs` 增 `pub mod ranges;`，`cargo build` 通过

## 4. 建立 paths 领域并消除重复

- [x] 4.1 新建 `src/paths/` 与 `paths/mod.rs`（re-export 子模块）
- [x] 4.2 `paths/normalize.rs`：迁入 `normalize_workspace_relative`（及相关 test）
- [x] 4.3 `paths/resolve.rs`：迁入 `ResolvedPath`、`parse_ws_path*`、`resolve_relative*`、`validate_within_workspace`、`validate_parent_within_workspace`
- [x] 4.4 `paths/metadata_name.rs`：迁入 `is_metadata_path`/`metadata_path_for`/`data_path_from_metadata`（合并 `workspace.rs` 与 `backend/path.rs` 两处重复，仅保留一份）
- [x] 4.5 `paths/scope_prefix.rs`：迁入 `list_scope_prefix`/`path_matches_scope`（及 test）
- [x] 4.6 删除 `src/backend/path.rs` 及其中的 `normalize_input_path` 包装别名；调用处改用 `paths::normalize_workspace_relative`
- [x] 4.7 删除 `workspace.rs` 中的 dead code `strip_path_components`
- [x] 4.8 在 `lib.rs` 声明 `pub mod paths;`，全局更新引用，`cargo build` 通过

## 5. 建立 scoping 领域

- [x] 5.1 新建 `src/scoping.rs`，迁入 `SessionScope`（含 test 中作用域相关用例）
- [x] 5.2 删除已清空的 `src/workspace.rs`；`lib.rs` 移除 `pub mod workspace;`，新增 `pub mod scoping;`
- [x] 5.3 全局更新 `crate::workspace::SessionScope` → `crate::scoping::SessionScope` 等引用，`cargo build` 通过

## 6. config 领域三拆 + 模板归位

- [x] 6.1 新建 `src/config/mod.rs`：保留 `Config`/`BackendConfig` 及取值方法
- [x] 6.2 `config/raw.rs`：迁入 `Raw*Backend` 与 serde default 函数
- [x] 6.3 `config/load.rs`：迁入 `load`/`load_from_path`/`parse_*_backend`/`resolve_config_path`（及 test）
- [x] 6.4 `config/templates.rs`：迁入 `DEFAULT_FILE_CONFIG`/`DEFAULT_MYSQL_CONFIG`
- [x] 6.5 删除 `src/config.rs`（单文件）；`init.rs` 改用 `config::templates::*`，移除自带常量
- [x] 6.6 `cargo build` 通过

## 7. backend → storage 领域重组

- [x] 7.1 新建 `src/storage/`；`storage/mod.rs` 保留 `WorkspaceBackend` trait 与 `ListReport`
- [x] 7.2 `storage/handle.rs`：迁入 `BackendHandle` 枚举、`open_backend`、`open_scoped_backend`、`apply_session_scope`
- [x] 7.3 平移 `file.rs`→`storage/file.rs`、`scoped.rs`→`storage/scoped.rs`
- [x] 7.4 `storage/mysql/connection.rs`：迁入 `connect`/`ensure_schema`/`block_on`/runtime/DDL 与 `map_db_err`/`quote_mysql_identifier`/`naive_utc_to_fixed`
- [x] 7.5 `storage/mysql/mod.rs`：迁入 `MySqlBackend` 的 `WorkspaceBackend` CRUD 实现
- [x] 7.6 删除 `src/backend/` 目录；`lib.rs` 改 `pub mod storage;`
- [x] 7.7 全局更新 `crate::backend::` → `crate::storage::` 引用，`cargo build` 通过

## 8. 收尾验证

- [x] 8.1 核对 `lib.rs` 模块声明与目标结构一致
- [x] 8.2 `cargo build` 与 `cargo test` 全绿
- [x] 8.3 `cargo clippy`（若可用）无因移动引入的新告警（仅剩 2 条预先存在的告警，非本次移动引入）
- [x] 8.4 确认对外行为、CLI、配置格式、错误码均未改变
