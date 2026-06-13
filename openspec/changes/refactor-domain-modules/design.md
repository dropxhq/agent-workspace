## Context

`agent-workspace` 是一个约 3400 行的 Rust CLI crate。当前组织方式介于"技术分层"与"领域"之间，但不彻底：

- 单文件混杂多概念：`workspace.rs`（作用域 + 路径规范化 + 路径解析/越界校验 + 元数据命名 + dead code）、`config.rs`（模型 + Raw DTO + 校验 + 配置发现）、`backend/mysql.rs`（连接/runtime/schema + CRUD + SQL 辅助）、`backend/mod.rs`（trait + ListReport + Handle 枚举 + 工厂）。
- 概念分散/重复：路径逻辑分布在 `workspace.rs` 与 `backend/path.rs`，`is_metadata_path` 重复定义，`normalize_input_path` 只是 `normalize_workspace_relative` 的包装别名。
- 归类错误：`ranges`（`LineRange`/`parse`/`apply`）是被 `storage` 后端依赖的领域原语，却位于 `commands/` 下。

约束：本次只做"重新聚合 + 拆分"，**不改变任何运行时行为、对外接口、错误码或算法**。

## Goals / Non-Goals

**Goals:**
- 按领域/特性重组模块，使"一个功能 = 一个清晰位置"。
- 消除重复定义与错误归类，路径逻辑收敛到单一 `paths/` 领域。
- 保持 `cargo build` / `cargo test` 全绿，测试随代码迁移。

**Non-Goals:**
- 不优化或修改任何算法、SQL、锁策略、错误处理逻辑。
- 不改变 CLI 参数、配置文件格式、存储 schema。
- 不新增/删除功能能力（无 spec 行为变更）。
- 不引入或移除依赖。

## Decisions

### D1：按领域而非技术分层组织
最终顶层布局：

```
src/
├── main.rs        仅 fn main → cli::run()
├── lib.rs         mod 声明
├── error.rs       (不变)
├── lock.rs        (不变)
├── cli.rs         Cli/Commands 定义 + dispatch + scoped_backend
├── scoping.rs     SessionScope
├── ranges.rs      LineRange + parse + apply + filter_lines
├── metadata.rs    FileMetadata + sidecar + build_metadata + compute_sha256 + now_local
├── paths/
│   ├── mod.rs           re-export
│   ├── normalize.rs     normalize_workspace_relative
│   ├── resolve.rs       ResolvedPath + parse_ws_path* + resolve_relative* + validate_*within_workspace
│   ├── metadata_name.rs is_metadata_path / metadata_path_for / data_path_from_metadata
│   └── scope_prefix.rs  list_scope_prefix / path_matches_scope
├── config/
│   ├── mod.rs       Config / BackendConfig + 取值方法
│   ├── raw.rs       Raw*Backend + serde default 函数
│   ├── load.rs      load / load_from_path / parse_*_backend / resolve_config_path
│   └── templates.rs DEFAULT_FILE_CONFIG / DEFAULT_MYSQL_CONFIG
├── storage/
│   ├── mod.rs       WorkspaceBackend trait + ListReport
│   ├── handle.rs    BackendHandle + open_backend / open_scoped_backend / apply_session_scope
│   ├── file.rs      FileBackend
│   ├── scoped.rs    ScopedMySqlBackend
│   └── mysql/
│       ├── mod.rs        MySqlBackend CRUD impl
│       └── connection.rs connect/ensure_schema/runtime/block_on + DDL + map_db_err/quote/datetime
└── commands/
    ├── mod.rs
    ├── read.rs / write.rs / list.rs / remove.rs
    └── init.rs   (模板常量移走，改用 config::templates)
```

**理由**：用户明确选择"按领域/特性"。领域边界（paths / scoping / ranges / metadata / config / storage / cli / commands）比技术分层更贴近"我要改某个功能"的心智模型。

### D2：`paths/` 细拆为 4 个文件（方案 A）
即使总量不大，也按子概念拆成独立文件，优先导航清晰度。`backend/path.rs` 解散：路径字符串逻辑并入 `paths/`，`is_metadata_path`/`normalize_input_path` 重复版删除。
- 备选：合成单个 `paths.rs`（被否，因用户希望避免"单文件多块"）。

### D3：`config` 模板归入 config 领域
`init.rs` 的两个 `DEFAULT_*_CONFIG` 常量移入 `config/templates.rs`，`init.rs` 改为引用。模板属于 config 领域知识，集中后便于维护。

### D4：`storage/mysql` 拆"连接层 / 存储层"
`connection.rs` 放连接、runtime、schema DDL 及 `map_db_err`/`quote_mysql_identifier`/`naive_utc_to_fixed` 辅助；`mod.rs` 放 `WorkspaceBackend` 的 CRUD 实现。二者通过 `MySqlBackend` 结构体衔接。

### D5：测试就近迁移
每个文件的 `#[cfg(test)] mod tests` 随其覆盖的代码移动到新位置，不集中、不改断言。

## Risks / Trade-offs

- [大面积 `use` 路径改动易遗漏] → 以 `cargo build` 反复编译驱动，编译器报错即定位；分模块小步提交。
- [`pub` 可见性在跨模块移动后不足] → 移动后按编译错误补齐 `pub`/`pub(crate)`，不扩大不必要的公开面。
- [`paths/` 过碎增加跳转] → 用 `paths/mod.rs` 统一 re-export，调用方写 `paths::xxx` 不感知内部文件划分。
- [`#[cfg(not(test))]` 的 `init` mysql 分支] → 迁移时保留 `cfg` 双实现，行为不变。
- [行为意外改变] → 通过"测试集全绿 + 不触碰任何函数体逻辑"双重约束规避；仅移动与改 `use`。

## Migration Plan

按低耦合→高耦合顺序，每步保持可编译：
1. 抽 `cli.rs`，瘦身 `main.rs`。
2. `meta.rs` → `metadata.rs`。
3. 建 `ranges.rs`（合并 `content.rs`），删旧文件。
4. 建 `paths/`，迁入并消重，解散 `backend/path.rs`，从 `workspace.rs` 移出路径逻辑。
5. 建 `scoping.rs`，从 `workspace.rs` 移出 `SessionScope`；`workspace.rs` 清空后删除。
6. `config.rs` → `config/{mod,raw,load,templates}.rs`，`init.rs` 改引用。
7. `backend/` → `storage/`，拆 `mod.rs`/`handle.rs` 与 `mysql/{connection,mod}.rs`。
8. 更新 `lib.rs` 模块声明；`cargo build && cargo test` 全绿。

回滚：纯文件移动，`git revert`/`git checkout` 即可恢复。
