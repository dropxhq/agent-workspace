## Why

当前代码按"技术分层"零散组织，多个文件把不相关的概念混在一起（如 `workspace.rs` 同时承载会话作用域、路径规范化、路径解析校验、元数据命名），同一概念又被拆散甚至重复定义（`is_metadata_path` 在 `workspace.rs` 与 `backend/path.rs` 各有一份；`ranges` 是被存储后端依赖的领域原语却放在 `commands/` 下）。这导致定位/修改一个"功能"时需要在多个文件间反复跳转，查找费劲。

## What Changes

按"领域/特性"重新聚合与拆分模块，**仅搬运、合并、拆分，不改变任何运行时行为或逻辑**：

- 新增 `cli.rs`：从 `main.rs` 抽出 clap 的 `Cli/Commands` 定义、`dispatch`、`scoped_backend`，`main.rs` 仅保留 `fn main`。
- 新增 `scoping.rs`：承载 `SessionScope` 全套（从 `workspace.rs` 抽出）。
- 新增顶层 `ranges.rs`：将 `commands/ranges.rs` 提到顶层，并入 `backend/content.rs` 的 `filter_lines`；`commands/ranges.rs` 与 `backend/content.rs` 删除。
- 新增 `paths/` 目录（方案 A 细拆）：`normalize.rs` / `resolve.rs` / `metadata_name.rs` / `scope_prefix.rs`，统一所有路径字符串逻辑。
- **消除重复**：`is_metadata_path`、`normalize_input_path` 等在 `backend/path.rs` 的重复/包装版本删除，统一使用 `paths::` 版本。
- 删除 `workspace.rs` 中的 dead code `strip_path_components`。
- `config.rs` 三拆为 `config/{mod.rs, raw.rs, load.rs}`；`init.rs` 中的 `DEFAULT_FILE_CONFIG`/`DEFAULT_MYSQL_CONFIG` 模板移入 `config/`（`config/templates.rs`）。
- `meta.rs` 改名搬运为 `metadata.rs`。
- `backend/` 重命名为 `storage/`，`backend/mod.rs` 二拆为 `storage/mod.rs`(trait + `ListReport`) 与 `storage/handle.rs`(`BackendHandle` 枚举 + 工厂)；`backend/mysql.rs` 二拆为 `storage/mysql/{connection.rs, mod.rs}`；`file.rs`/`scoped.rs` 平移。
- 所有 `#[cfg(test)] mod tests` 随各自代码搬运；`use crate::...` 路径机械更新。

## Capabilities

### New Capabilities
- `module-structure`: 约定源码按领域/特性组织的目录与模块边界（本次重构的交付契约，仅描述结构，不改变任何运行时行为）。

### Modified Capabilities
<!-- 纯结构重构，不改变任何运行时 spec 级行为，故无 -->

## Impact

- 影响范围：`src/` 下几乎所有模块的文件位置与 `use` 路径；`lib.rs` 的 `mod` 声明。
- 不影响：CLI 接口、配置格式、存储行为、错误码、对外可观察行为。
- 验证方式：`cargo build` 与 `cargo test` 全绿（现有测试随代码迁移，应全部通过）。
- 无依赖变更。
