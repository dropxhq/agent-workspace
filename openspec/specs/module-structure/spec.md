### Requirement: 源码按领域组织为单一职责模块

代码库 SHALL 按领域/特性而非技术分层组织模块，且每个领域概念有唯一归属位置。本需求仅约束代码结构，MUST NOT 改变任何运行时行为、CLI 接口、配置格式、存储 schema 或错误码。

顶层模块边界 SHALL 为：`cli`、`scoping`、`ranges`、`metadata`、`paths`、`config`、`storage`、`commands`，以及保持不变的 `error`、`lock`。

#### Scenario: 入口职责最小化
- **WHEN** 查看 `src/main.rs`
- **THEN** 它仅包含 `fn main`，CLI 定义（`Cli`/`Commands`）、`dispatch` 与 `scoped_backend` 位于 `src/cli.rs`

#### Scenario: 会话作用域独立成域
- **WHEN** 查找 `SessionScope` 相关逻辑
- **THEN** 全部位于 `src/scoping.rs`，且不再存在 `src/workspace.rs`

#### Scenario: 路径逻辑收敛到 paths 领域且无重复
- **WHEN** 查找路径规范化、解析/越界校验、元数据命名、列表前缀匹配逻辑
- **THEN** 分别位于 `src/paths/{normalize,resolve,metadata_name,scope_prefix}.rs`
- **AND** `is_metadata_path`、`normalize_workspace_relative` 等在整个代码库各只定义一次（删除原 `backend/path.rs` 中的重复与包装别名）

#### Scenario: ranges 作为领域原语提升到顶层
- **WHEN** 查找行区间解析/应用与按区间过滤内容的逻辑
- **THEN** `LineRange`、`parse_ranges`、`line_in_ranges`、`apply_write_ranges`、`filter_lines` 全部位于 `src/ranges.rs`
- **AND** 不再存在 `src/commands/ranges.rs` 与 `src/backend/content.rs`

#### Scenario: config 领域聚合模型、加载与模板
- **WHEN** 查找配置模型、Raw 反序列化、加载/校验/发现、默认配置模板
- **THEN** 分别位于 `src/config/{mod,raw,load,templates}.rs`
- **AND** `init` 命令引用 `config` 中的模板常量，而非自带定义

#### Scenario: storage 领域区分连接层与存储层
- **WHEN** 查找存储后端相关代码
- **THEN** 位于 `src/storage/`：`mod.rs`(trait + `ListReport`)、`handle.rs`(`BackendHandle` + 工厂)、`file.rs`、`scoped.rs`、`mysql/connection.rs`(连接/runtime/schema/SQL 辅助)、`mysql/mod.rs`(CRUD 实现)
- **AND** 不再存在 `src/backend/` 目录

#### Scenario: 行为与测试保持不变
- **WHEN** 在重构后执行 `cargo build` 与 `cargo test`
- **THEN** 全部通过，原有测试随各自代码迁移到新位置且断言不变
