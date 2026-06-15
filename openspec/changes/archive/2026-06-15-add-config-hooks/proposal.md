## Why

Agent Workspace 当前以原始字节读写工作区文件，无法在存储层与调用方之间做可配置的字符串转换。实际使用中常需要对落盘内容编解码、脱敏或格式变换，同时偶尔需要绕过转换直接操作物理内容（调试、迁移、手工修复）。在 `config.yaml` 中声明全局 read/write hook（外部命令）并提供按次请求的跳过开关，可在不改动各入口调用方式的前提下统一覆盖 CLI、MCP 与 Python。

## What Changes

- 在 `config.yaml` 顶层新增可选 `hooks` 块，每个配置文件对应一套全局 `read` / `write` 外部命令 hook。
- 新增 `HookedBackend` 装饰器，在 `WorkspaceBackend` 层于读写边界执行 hook；未配置 `hooks` 时行为与现版完全一致。
- read hook：物理内容读取后、行区间过滤前，将存储内容转换为逻辑内容返回调用方。
- write hook：行区间合并后、物理写入前，将逻辑内容转换为物理内容落盘；区间写时先在逻辑空间合并（读取已有内容时同样经 read hook）。
- 在 CLI `read`/`write`、MCP `read`/`write` 工具、Python `Workspace.read`/`write` 增加可选 `skip_hooks`（CLI 为 `--no-hooks`），单次请求跳过所有相关 hook，直接操作物理存储内容。
- 扩展 `WorkspaceBackend::read` / `write` 签名，增加 `IoOptions { skip_hooks }` 参数；`list` / `remove` 不变。
- 更新 `README.md` 与默认配置模板，记录 hook 命令协议与 `skip_hooks` 语义。

## Capabilities

### New Capabilities

- `config-hooks`: `config.yaml` 中 hooks 配置格式、外部命令执行协议、读写边界上的 hook 应用顺序、以及 `skip_hooks` 按次绕过语义。

### Modified Capabilities

<!-- 无既有主规格；本变更为新增可观察行为 -->

## Impact

- **配置**：`config/raw.rs`、`config/mod.rs`、`config/load.rs`、`config/templates.rs`；`config.yaml` 格式扩展（向后兼容）。
- **存储**：新 `storage/hooked.rs`（或 `hooks/` 模块）；`storage/handle.rs` 在 `open_backend` 后条件包装；`WorkspaceBackend` trait 及 file/mysql/scoped/handle 全部实现同步更新。
- **入口**：`cli.rs`、`commands/read.rs`、`commands/write.rs`、`mcp/tools.rs`、`python.rs` 传递 `IoOptions`。
- **测试**：hook round-trip、区间写 + hook、hook 失败/超时、`skip_hooks`、无 hooks 配置时的透传。
- **文档**：`README.md` 行为说明与配置示例。
