# Agent 工作目录工具 — 实现计划

基于 Rust 的受限文件操作 CLI。核心约束：所有读写只能发生在配置指定的工作目录内，并通过文件锁保证多进程并发安全。

## 1. 总体目标

Rust CLI 二进制名 **`ws`**，提供受限的文件操作（read / write / info / remove）。用户传入的路径必须以 **`ws+file://`** 开头（如 `ws+file://docs/foo.txt`），解析后映射为 workspace 内相对路径；禁止越界访问；并发写入通过文件锁保护。

## 2. 技术选型

| 用途 | Crate | 说明 |
|------|-------|------|
| CLI 解析 | `clap` v4 (derive) | 子命令 read/write/info/remove |
| 配置/元数据序列化 | `serde` + `serde_yaml` | config.yaml 及元数据文件 |
| 文件锁 | `fs4`（fs2 的维护分支） | 跨平台 advisory flock |
| 时间戳 | `chrono` | created_at / updated_at |
| 内容哈希（可选） | `sha2` | 元数据中记录文件指纹 |
| 目录遍历 | `walkdir` | info 命令扫描 |
| 错误处理 | `anyhow` + `thiserror` | 库内用 thiserror，main 用 anyhow |

## 3. 模块结构

```
src/
  main.rs          # clap 入口与子命令分发
  config.rs        # 配置加载（cwd/config.yaml 或 env 覆盖）
  workspace.rs     # 路径解析 + 越界防护（核心安全层）
  meta.rs          # 元数据模型、sidecar 读写
  lock.rs          # 加锁辅助（共享/独占）
  error.rs         # 统一错误类型
  commands/
    read.rs
    write.rs
    info.rs
    remove.rs
```

## 4. 配置加载

```yaml
# config.yaml
workspace_dir: ./data         # 工作目录，相对 config 文件解析为绝对路径
metadata_suffix: ".meta.yaml" # 可选，默认值
```

加载顺序：
1. 若环境变量 `AGENT_WORKSPACE_CONFIG` 存在 → 用它指向的配置文件。
2. 否则读取命令启动目录（cwd）下的 `config.yaml`。
3. `workspace_dir` 相对路径以 config 文件所在目录为基准解析为绝对 canonical 路径。启动时校验该目录存在且可写。

## 5. 安全模型（最关键部分）

所有用户传入的路径必须经过 `workspace.rs` 统一处理：
1. **前缀校验**：必须以 `ws+file://` 开头，否则退出码 `2`。剥掉前缀后进入路径归一化。
2. **路径归一化**（POSIX 语义 + 工作区根截断）：
   - 按 `/` 拆分为段，从左到右扫描：
     - 段为 `.` → 跳过；
     - 段为 `..` 且栈非空 → 弹出栈顶（回退一级）；
     - 段为 `..` 且栈为空 → 跳过（已在工作区根，不能再往上）；
     - 其它 → 入栈。
   - 栈拼接为 workspace 相对路径；栈空 → 表示工作区根目录。
3. 拼接到 workspace 后，校验最终路径 `starts_with(workspace_canonical)`。
4. 防符号链接逃逸：对已存在路径 canonicalize 后再次校验；对父目录同样校验。
5. 元数据文件保护：任何命中元数据后缀（如 `*.meta.yaml`）的路径，在 read/remove 中一律按"文件不存在"处理，不泄露其存在。

**等价关系**：`../a/b/c.md` ≡ `/a/b/c.md` ≡ `a/b/c.md`（均归一化为 `a/b/c.md`）；`foo/../bar` 归一化为 `bar`（非 `foo/bar`）。

**说明**：`ws+file://` 下的路径**永远是 workspace 相对路径**；前导 `/` 或前导 `..` 仅表示「从工作区根算起」，**不会**逃逸到宿主机文件系统（如 `/etc/passwd`）。

## 6. 元数据 sidecar 文件

命名方案（**已确认：方案 A**）：
- `foo.txt` → `foo.txt.meta.yaml`。避免 `foo.txt`/`foo.md` 元数据互相覆盖。

元数据内容：

```yaml
relative_path: docs/foo.txt
created_by: "agent-x"
desc: "需求文档草稿"
created_at: 2026-06-13T13:00:00+08:00
updated_at: 2026-06-13T13:35:00+08:00
size_bytes: 1024
sha256: "..."        # 可选
```

`write` 时若元数据已存在，保留 `created_by`/`created_at`，更新其余字段。

## 7. 并发与文件锁

- 每个目标文件用 `fs4` 的 advisory lock：`read` 取共享锁，`write`/`remove` 取独占锁。
- 写数据文件 + 写元数据文件需保持一致性：在同一把独占锁（锁数据文件）下完成两次写入，避免"数据写了但元数据没更新"。
- 固定加锁顺序（先数据文件后元数据）防死锁。
- 锁为进程级 advisory，仅对同样走本工具的进程有效——文档中写明。

## 8. 各命令详细行为

### read
`read ws+file://<relative-path> [--ranges 1-10,20-30] [--human]`

示例：`ws read "ws+file://docs/foo.txt" --human`
- 默认返回文件原文（raw）。
- `--human`：开头加一行相对路径，每行加行号（如 `   12 | content`）。
- `--ranges`：1-indexed 行区间，逗号分隔；human 模式下仍显示真实行号。
- 命中元数据文件 → "not found"。

### write
`write ws+file://<relative-path> [--ranges START-END] [--created-by X] [--desc "..."]`，内容从 stdin 或 `--content` 读入

示例：`echo "hello" | ws write "ws+file://docs/foo.txt" --created-by agent-x`

**ranges 语义（已确认）**：
- **无 ranges**：删除整个文件原有内容，写入 stdin/`--content` 的新内容（等价于整文件覆盖）。
- **有 ranges**：删除 `START..END`（含端点，1-indexed）行区间内的内容，在该位置贴入新内容；其余行保持不变。新内容按行插入，可含多行。
- 不支持 `--insert-at` 纯插入（后续如需再加）。

写完更新/创建 sidecar 元数据。整个过程持独占锁。

### info
`info [--json]`
- `walkdir` 扫描 workspace 下所有元数据文件，聚合成报告：文件数、总大小、每个文件的 path/created_by/desc/时间。
- 默认人类可读表格，`--json` 输出结构化结果。

### remove
`remove ws+file://<relative-path>`

示例：`ws remove "ws+file://docs/foo.txt"`
- 删除数据文件 + 对应元数据文件（独占锁下）。
- 目标本身是元数据文件 → "not found"。

## 9. 错误处理

统一退出码：`0` 成功；`1` 一般错误；`2` 路径越界/非法；`3` 未找到（含元数据保护触发）；`4` 锁冲突/超时。错误信息写 stderr。

## 10. 测试计划

- 单元测试：路径归一化（POSIX `..` 回退、工作区根截断、前导 `/`、等价路径）、符号链接逃逸、元数据隐藏、ranges 解析。
- 集成测试（`tempfile` 建临时 workspace）：四个命令端到端。
- 并发测试：多线程/多进程同时 write 同一文件，验证锁有效、元数据一致。

## 11. CLI 命名与路径协议（已确认）

| 项 | 约定 |
|----|------|
| 二进制名 | `ws` |
| 路径前缀 | **`ws+file://`**（常量 `WS_FILE_PREFIX`） |
| 示例 | `ws read "ws+file://docs/foo.txt"` |

### 解析规则

1. 输入必须以 `ws+file://` 开头，否则报错（退出码 `2`）。
2. 剥掉前缀，得到路径片段。
3. 按 `/` 拆段，**POSIX 归一化 + 工作区根截断**（见 §5 算法）。
4. 栈拼接为 workspace 相对路径；栈空表示工作区根目录。
5. 拼接到 workspace 后走安全层（边界校验、符号链接逃逸等）。
6. Shell 中路径含 `://`，**始终加引号**。

伪代码：

```
stack = []
for seg in path.split('/'):
    if seg == '' or seg == '.': continue
    elif seg == '..':
        if stack: stack.pop()
        # else: 已在工作区根，忽略
    else:
        stack.push(seg)
relative = join(stack, '/')
```

### 路径示例

| 输入 | 归一化后的 workspace 相对路径 | 实际访问（workspace=`/data`） |
|------|------------------------------|-------------------------------|
| `ws+file://a/b/c.md` | `a/b/c.md` | `/data/a/b/c.md` |
| `ws+file:///a/b/c.md` | `a/b/c.md` | `/data/a/b/c.md` |
| `ws+file://../a/b/c.md` | `a/b/c.md` | `/data/a/b/c.md` |
| `ws+file://./docs/foo.txt` | `docs/foo.txt` | `/data/docs/foo.txt` |
| `ws+file://../etc/passwd` | `etc/passwd` | `/data/etc/passwd`（非系统 `/etc/passwd`） |
| `ws+file://foo/../bar` | `bar` | `/data/bar` |

### 为何不用 `ws://`

`ws://` 是 WebSocket 的 IANA 注册 scheme（RFC 6455），URL 库与 Agent 易误判为网络地址。`ws+file://` 沿用 `scheme+transport://` 惯例（类似 `git+ssh://`），语义明确且无协议冲突。

### `info` 命令

`info` 无路径参数；输出中的 `relative_path` 字段为裸相对路径（如 `docs/foo.txt`），不带 `ws+file://` 前缀，便于程序消费。人类可读模式下可选加前缀展示。

## 已确认项

1. ✅ 元数据后缀：方案 A（`foo.txt.meta.yaml`）
2. ✅ write ranges：有 ranges → 删区间内行再贴新内容；无 ranges → 删全文件再写新内容
3. ✅ CLI：二进制 `ws`；路径前缀 **`ws+file://`**
