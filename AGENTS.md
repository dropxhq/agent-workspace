# AGENTS.md

Agent Workspace (`ws`) — 基于 Rust 的受限文件操作 CLI，供 Agent 在配置的工作区内安全读写文件。所有路径均按工作区相对路径解析，禁止越界访问宿主机。

面向用户的完整说明见 `README.md`。本文件为在本仓库工作的 AI agent 提供工程约定与导航。

## 构建与测试

```bash
cargo build                 # 构建（release: cargo build --release，产物 target/release/ws）
cargo test                  # 单元 + 集成测试（mysql 测试默认 ignored）
cargo clippy --all-targets  # lint
cargo run -- <cmd>          # 开发模式运行，如 cargo run -- list
```

MySQL 集成测试需 `MYSQL_TEST_URL` 环境变量并运行 `cargo test --test mysql_integration -- --ignored`；未设置时自动跳过。

## 架构（源码按领域/特性组织）

| 模块 | 职责 |
|------|------|
| `cli.rs` | clap `Cli`/`Commands` 定义、命令分发、按作用域打开后端 |
| `commands/` | 各子命令实现：`init`/`read`/`write`/`list`/`remove` |
| `scoping.rs` | `SessionScope`（user/session 作用域解析与路径映射） |
| `ranges.rs` | 行区间解析、写入替换 `apply_write_ranges`、过滤 `filter_lines` |
| `metadata.rs` | `FileMetadata`、sidecar 读写、SHA256/时间戳 |
| `paths/` | 路径领域：`normalize` 归一化 / `resolve` 解析与越界校验 / `metadata_name` sidecar 命名 / `scope_prefix` 作用域前缀 |
| `config/` | 配置领域：`mod`(模型) / `raw`(DTO+默认值) / `load`(发现/加载/校验) / `templates`(init 模板) |
| `storage/` | 存储领域：`mod`(`WorkspaceBackend` trait + `ListReport`) / `handle`(`BackendHandle` 枚举+工厂) / `file` / `scoped` / `mysql/`(`connection` 连接层 + `mod` CRUD) |
| `mcp/` | 本地 MCP 服务：`protocol`(JSON-RPC 2.0 类型/错误码) / `server`(stdio 同步循环与方法分发) / `tools`(工具定义与执行，直接调用 `WorkspaceBackend`) |
| `error.rs` | `WsError` 错误类型与退出码映射 |
| `lock.rs` | file 后端的进程级 advisory 文件锁 |

入口：`main.rs` 仅 `fn main()`，调用 `cli::run()`。新增模块需在 `lib.rs` 声明。

## 关键约定与不变量

- **路径安全**：所有外部路径先经 `paths::normalize_workspace_relative` 归一化，写/读经 `resolve` 校验不得逃出工作区根；不要绕过这些函数直接拼接路径。
- **元数据隐藏**：`*.meta.yaml`（file 后端）在 `read`/`remove` 中视为不存在，避免泄露 sidecar。
- **错误与退出码**：错误统一返回 `WsResult<T>`（`error.rs`）。退出码：`2` 路径非法/越界、`3` 未找到、`4` 锁冲突、`1` 其他。新增错误场景请复用已有 `WsError` 变体以保持退出码语义。
- **双后端一致性**：`file` 与 `mysql` 后端均实现 `WorkspaceBackend` trait，行为应保持一致（含锁冲突返回码 4）。改动 trait 时同步更新所有实现与 `BackendHandle` 转发。
- **并发**：file 后端写操作在独占文件锁下同时更新数据与 sidecar；mysql 写/删在事务内 `SELECT ... FOR UPDATE`。
- **配置格式**：使用 `backend:` 块（`type: file|mysql`）。顶层 `workspace_dir`/`metadata_suffix` 为已移除的旧格式，勿恢复。
- **MCP stdout 纯净**：`ws mcp` 用 stdout 作 JSON-RPC 传输通道。工具实现必须直接调用 `WorkspaceBackend` trait 方法，**不得**复用 `commands::*::run`（它们会 `print!` 到 stdout 污染协议）。循环保持同步：mysql 后端内部自带 tokio runtime 并 `block_on`，在 runtime 内再跑会 panic。工具执行失败按 MCP 约定返回 `isError: true` 结果，仅调用格式非法才返回 JSON-RPC 协议错误。

## 修改注意事项

- 改动 CLI 参数、配置格式、退出码或存储 schema 属于对外行为变更，需谨慎并同步更新 `README.md`。
- 测试就近放置：每个模块的 `#[cfg(test)] mod tests` 与其代码同文件；跨后端行为在 `tests/integration.rs` / `tests/mysql_integration.rs`。
- 本项目使用 `openspec` 管理变更，提案/规格位于 `openspec/`。
