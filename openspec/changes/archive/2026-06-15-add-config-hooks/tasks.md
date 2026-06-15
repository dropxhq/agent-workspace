## 1. 配置模型

- [x] 1.1 在 `config/raw.rs` 增加 `RawHooks`、`RawHookCommand`（`command: Vec<String>`、`timeout_ms: Option<u64>`）及顶层 `hooks` 可选字段
- [x] 1.2 在 `config/mod.rs` 增加 `HookConfig`、`HookCommand`、`IoOptions`（`skip_hooks: bool`，`Default` 为 false）
- [x] 1.3 在 `config/load.rs` 解析 `hooks`、校验 `command` 非空；仅配单侧 hook 时输出 warning
- [x] 1.4 更新 `config/templates.rs` 默认模板，增加注释掉的 `hooks` 示例

## 2. Hook 执行引擎

- [x] 2.1 新增 `hooks/mod.rs`：实现 `run_hook(cmd, input, ctx) -> WsResult<String>`（stdin/stdout、cwd、`WS_HOOK`/`WS_PATH` 环境变量、超时、UTF-8 校验）
- [x] 2.2 为 `run_hook` 添加单元测试（mock 脚本：echo、非零退出、超时）

## 3. HookedBackend 装饰器

- [x] 3.1 新增 `storage/hooked.rs`：`HookedBackend` 持有 `inner` + `HookConfig`
- [x] 3.2 实现 read 路径：physical → read_hook → logical → `filter_lines`；`skip_hooks` 时透传
- [x] 3.3 实现 write 全量路径：logical → write_hook → physical → inner.write
- [x] 3.4 实现 write 区间路径：读 physical → read_hook → 逻辑合并 → write_hook → inner.write；`skip_hooks` 时物理空间合并
- [x] 3.5 在 `storage/handle.rs` 的 `open_backend` 中，当 `config.hooks` 存在时包装 `HookedBackend`；更新 `BackendHandle` 转发
- [x] 3.6 在 `lib.rs` 声明新模块

## 4. WorkspaceBackend trait 签名更新

- [x] 4.1 扩展 `WorkspaceBackend::read` / `write` 签名，增加 `opts: IoOptions` 参数
- [x] 4.2 更新 `storage/file.rs`、`storage/mysql/mod.rs`、`storage/scoped.rs`、`storage/handle.rs` 实现（非 Hooked 实现忽略 `opts`）

## 5. 入口层传递 IoOptions

- [x] 5.1 `cli.rs`：`Read`/`Write` 子命令增加 `--no-hooks`；`dispatch` 传入 `IoOptions`
- [x] 5.2 `commands/read.rs`、`commands/write.rs` 接受并转发 `IoOptions`
- [x] 5.3 `mcp/tools.rs`：`read`/`write` schema 增加 `skip_hooks`；工具实现解析并转发
- [x] 5.4 `python.rs`：`read`/`write` 增加 `skip_hooks` 关键字参数并转发

## 6. 测试

- [x] 6.1 `storage/hooked.rs` 或集成测试：hook round-trip（外部 echo/cat 脚本）
- [x] 6.2 测试区间写 + hook 顺序（逻辑空间合并）
- [x] 6.3 测试 `skip_hooks` read/write 返回/写入物理内容
- [x] 6.4 测试无 `hooks` 配置时行为不变
- [x] 6.5 测试 hook 失败不部分写入

## 7. 文档

- [x] 7.1 更新 `README.md`：`hooks` 配置格式、命令协议、`--no-hooks`/`skip_hooks` 语义与注意事项
- [x] 7.2 更新 `AGENTS.md` 配置格式与 MCP 相关说明（若适用）
