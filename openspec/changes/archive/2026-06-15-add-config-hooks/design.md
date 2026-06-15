## Context

Agent Workspace（`ws`）通过 `WorkspaceBackend` trait 统一 file / mysql 后端的读写，CLI、`ws mcp` 与 Python 绑定均经此 trait 访问工作区。当前读写路径为：读取物理内容 →（可选）行区间过滤 → 返回；写入时（可选）区间合并 → 直接落盘，元数据 `sha256` / `size_bytes` 针对物理字节计算。

本变更在 trait 层引入可选的 **hook 装饰器**，使每个 `config.yaml` 可声明一对全局外部命令，在逻辑内容与物理存储之间转换；同时提供按次请求的 `skip_hooks` 绕过开关。

## Goals / Non-Goals

**Goals:**

- 支持在 `config.yaml` 顶层配置可选 `hooks.read` / `hooks.write` 外部命令。
- hook 在 `WorkspaceBackend` 装饰器层执行，CLI / MCP / Python 自动一致。
- 明确物理/逻辑分层：read hook 在区间过滤前；write hook 在区间合并后；区间写在逻辑空间合并。
- 提供 `IoOptions.skip_hooks`（CLI `--no-hooks`）按次跳过 hook，直接操作物理内容。
- 未配置 `hooks` 时零行为变化、零额外开销（不包装装饰器）。
- 向后兼容：旧 `config.yaml` 无需修改。

**Non-Goals:**

- 内置变换（base64、regex 等）——仅外部命令。
- 按路径 glob 匹配不同 hook——每个 config 全局一套。
- read / write 分别独立的 skip 开关——单一 `skip_hooks` 跳过该次操作涉及的所有 hook。
- hook 作用于 `list` / `remove`。
- 配置热重载；hook 命令在 `open_backend` 时从 config 读取，进程生命周期内不变。

## Decisions

### 1. 装饰器模式：`HookedBackend` 包装 inner backend

**选择**：在 `open_backend` / `open_scoped_backend` 返回前，若 `config.hooks` 存在则包装为 `HookedBackend { inner, hooks }`。

**理由**：单一接入点覆盖全部入口；file / mysql / scoped 无需各自实现 hook 逻辑。

**备选**：在 `commands/read.rs` 与各入口分别调用 hook——重复且 MCP/Python 易遗漏。

### 2. 外部命令协议

**选择**：

| 项 | 约定 |
|----|------|
| 配置字段 | `command: ["executable", "arg1", ...]`（argv 数组，不经 shell） |
| 输入 | 完整内容经 **stdin** 传入（UTF-8 字符串；空文件传空字符串） |
| 输出 | **stdout** 全文作为转换结果；不 trim |
| 错误 | 非零退出码或超时 → 操作失败，返回 `WsError` |
| 工作目录 | `config.yaml` 所在目录 |
| 环境变量 | `WS_HOOK=read\|write`、`WS_PATH=<相对路径>`（便于共用脚本） |
| 超时 | 可选 `timeout_ms`，默认 `30000` |

**理由**：stdin/stdout 避免 shell 转义与参数长度限制；argv 数组避免注入；与 `workspace_dir` 相对路径解析惯例一致。

**备选**：shell 字符串 `command: "python hooks/decode.py"`——拒绝，安全风险高。

### 3. 物理/逻辑分层与执行顺序

```
READ (默认):
  inner.read(physical) → read_hook → logical → filter_lines(ranges) → return

READ (skip_hooks):
  inner.read(physical) → filter_lines(ranges) → return

WRITE 全量 (默认):
  write_hook(logical) → physical → inner.write(physical)

WRITE 区间 (默认):
  inner.read(physical) → read_hook → logical_existing
  apply_write_ranges(logical_existing, range, logical_new)
  → write_hook → physical → inner.write(physical)

WRITE (skip_hooks):
  inner.read(physical) → apply_write_ranges(physical, ...) → inner.write(physical)
```

**理由**：Agent 始终在逻辑空间看行号与内容；磁盘/DB 存物理内容；`sha256` 继续针对物理字节。

### 4. `IoOptions` 扩展 trait 签名

**选择**：

```rust
pub struct IoOptions {
    pub skip_hooks: bool,
}
```

`read` / `write` 增加 `opts: IoOptions` 参数（默认 `Default::default()` → `skip_hooks: false`）。非 `Hooked` 实现忽略该字段。

**理由**：按次控制，适配 MCP/Python 长生命周期 backend；比 backend 实例级开关更清晰。

**备选**：thread-local——隐式、难测、多线程不安全。

### 5. 各入口参数命名

| 入口 | 参数 |
|------|------|
| CLI `read` / `write` | `--no-hooks` → `IoOptions { skip_hooks: true }` |
| MCP `read` / `write` | `skip_hooks: boolean`（可选，默认 false） |
| Python `read` / `write` | `skip_hooks=False` 关键字参数 |

`list` / `remove` 不暴露该参数。

### 6. 配置模型

```yaml
backend:
  type: file
  workspace_dir: ./data
  metadata_suffix: ".meta.yaml"

hooks:                    # 可选整块
  read:
    command: ["python", "hooks/decode.py"]
    timeout_ms: 10000     # 可选
  write:
    command: ["python", "hooks/encode.py"]
```

- 仅配 `read` 或仅配 `write`：允许；加载时 `warn` 日志提示可能无法 round-trip。
- `command` 非空数组；`timeout_ms` 若存在须 > 0。

### 7. 模块布局

- `hooks/mod.rs`（或 `storage/hooks.rs`）：`run_hook(cmd, input, ctx) -> WsResult<String>`
- `storage/hooked.rs`：`HookedBackend` 实现 `WorkspaceBackend`
- `config/raw.rs` + `config/mod.rs`：`HookConfig`、`HookCommand`
- `BackendHandle::Hooked` 变体，或统一在工厂函数层包装（实现时二选一，优先工厂层包装以减少 `match` 分支）

### 8. UTF-8 与错误类型

hook stdout 非法 UTF-8 → `WsError::Other` 明确报错。hook 失败不部分写入。

## Risks / Trade-offs

| 风险 | 缓解 |
|------|------|
| 外部命令执行任意代码 | 与信任 `config.yaml` 同级；文档说明仅部署可信 hook |
| 大文件 + 子进程拷贝 stdin/stdout 内存与延迟 | 默认 30s 超时；文档建议 hook 流式处理（未来可扩展，本次不实现） |
| `skip_hooks` 混用导致存储损坏 | 文档明确：raw 读写应对 read/write 一致使用 `skip_hooks` |
| trait 签名变更影响所有实现 | 机械更新 file/mysql/scoped/handle；集成测试覆盖 |
| 区间写 + hook 顺序错误导致行号错乱 | 规格与单测锁定逻辑空间合并顺序 |
| 仅单侧 hook 配置 | 加载时 warning，不阻断启动 |

## Migration Plan

1. 部署新版本 `ws`；旧 config 无 `hooks` 块，行为不变。
2. 需要转换时追加 `hooks` 配置与 hook 脚本；先用 `--no-hooks` 验证物理内容，再默认启用。
3. 回滚：移除 `hooks` 块或降级二进制；已按物理格式存储的文件需用户自行处理（hook 责任边界）。

## Open Questions

- 是否在 `warn` 之外对「仅配单侧 hook」提供 `strict_hooks: true` 加载失败选项？**本次不做**，仅 warning。
- hook 失败是否需专用退出码？**本次复用退出码 1**（`WsError::Other`），不新增变体。
