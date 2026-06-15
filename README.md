# Agent Workspace (`ws`)

基于 Rust 的受限文件操作 CLI，供 Agent 在指定工作目录内安全地读写文件。所有路径均相对于配置的工作区解析，无法越界访问宿主机其他路径。

## 特性

- **双后端**：本地文件（`file`）或 MySQL（`mysql`），通过 `config.yaml` 切换
- **路径隔离**（file 后端）：读写操作限制在 `workspace_dir` 内，含符号链接逃逸检测
- **元数据**：file 后端使用 `*.meta.yaml` sidecar；mysql 后端将元数据存入数据库
- **并发安全**：file 后端使用 advisory 文件锁；mysql 后端使用 InnoDB 行锁
- **按行操作**：支持按行区间读取或局部替换写入
- **本地 MCP 服务**：`ws mcp` 通过 stdio 暴露 JSON-RPC（MCP 协议）工具，供 MCP 客户端调用工作区读写列删操作

## 安装

需要 [Rust](https://rustup.rs/) 工具链。

```bash
# 克隆或进入项目目录后
cargo build --release

# 安装到 ~/.cargo/bin
cargo install --path .
```

构建产物位于 `target/release/ws`。安装后可直接在终端运行 `ws`。

## 初始化

使用 `init` 命令创建新的工作区（生成 `config.yaml`，file 后端还会创建 `data/` 目录）：

```bash
# 在当前目录初始化（默认 file 后端）
ws init

# 在指定目录初始化（不存在则自动创建）
ws init ./my-agent-workspace
ws init /path/to/workspace

# 使用 MySQL 后端（生成 mysql 配置模板并自动建库建表）
ws init --backend mysql
ws init ./my-agent-workspace --backend mysql
```

`--backend mysql` 会写入 MySQL 连接配置模板，并尝试连接数据库、创建库（若不存在）及 `workspace_files` 表。请编辑 `config.yaml` 中的 `host`、`user`、`password`、`database` 后再使用其他命令。

初始化完成后，进入该目录即可使用其他命令。若目录下已存在 `config.yaml`，会报错以避免覆盖。

## 配置

> **破坏性变更**：旧版顶层 `workspace_dir` / `metadata_suffix` 已移除。请改用 `backend` 块，见下方示例。

在项目根目录（或任意启动目录）放置 `config.yaml`。

### File 后端（默认）

```yaml
backend:
  type: file
  workspace_dir: ./data         # 相对 config 文件所在目录解析
  metadata_suffix: ".meta.yaml" # 可省略（默认值）
```

启动时会校验 `workspace_dir` 存在且可写。

### MySQL 后端

```yaml
backend:
  type: mysql
  host: localhost
  port: 3306                    # 可省略（默认 3306）
  user: ws_user
  password: change_me
  database: agent_workspace
```

连接时会自动确保数据库和 `workspace_files` 表存在。元数据（创建者、描述、时间戳、SHA256）与文件内容存储在同一张表中，不再使用 sidecar 文件。

### 加载顺序

除 `init` 外，其余命令均通过配置文件加载后端。可按以下优先级指定配置文件：

1. 命令行 `--config /path/to/config.yaml`
2. 环境变量 `AGENT_WORKSPACE_CONFIG`
3. **当前工作目录**下的 `config.yaml`

`init` 会生成新的 `config.yaml`，不使用上述加载逻辑；传入的 `--config` 对其无效。

## 路径规则

所有命令的路径参数均为**工作区相对路径**，例如 `docs/foo.txt`。

路径会按 POSIX 语义归一化，且不能逃出工作区根目录：

| 输入 | 归一化结果 |
|------|-----------|
| `docs/foo.txt` | `docs/foo.txt` |
| `/docs/foo.txt` | `docs/foo.txt` |
| `../docs/foo.txt` | `docs/foo.txt` |
| `foo/../bar` | `bar` |
| `../etc/passwd` | `etc/passwd`（访问的是工作区内的 `etc/passwd`，不是系统 `/etc/passwd`） |

元数据文件（如 `foo.txt.meta.yaml`）在 `read` / `remove` 中视为不存在，不会泄露其内容。

## 命令

所有需加载配置的命令均支持全局 `--config` 选项，例如：

```bash
ws --config /path/to/config.yaml read docs/foo.txt
ws read docs/foo.txt --config /path/to/config.yaml
```

两种写法等价；未指定时按上文「加载顺序」解析。

### `init` — 初始化工作区

```bash
ws init
ws init ./my-agent-workspace
ws init --backend mysql
ws init ./my-agent-workspace --backend mysql
```

| 参数 | 说明 |
|------|------|
| `[path]` | 可选，目标目录；省略则在当前工作目录初始化 |
| `--backend` | 后端类型：`file`（默认）或 `mysql` |

### `read` — 读取文件

```bash
ws read docs/foo.txt
ws read docs/foo.txt --human
ws read docs/foo.txt --ranges 1-10,20-30
```

| 选项 | 说明 |
|------|------|
| `--human` | 首行输出路径，每行前加行号（如 `    12 \| content`） |
| `--ranges` | 1-indexed 行区间，逗号分隔；human 模式下仍显示真实行号 |

默认输出文件原文（raw）。

### `write` — 写入文件

`--content`、`--created-by`、`--desc` 均为必选参数：

```bash
ws write docs/foo.txt --content "hello\n" --created-by agent-x --desc "需求草稿"
```

按行区间局部替换（删除 `START..END` 行，在该位置插入新内容）：

```bash
ws write docs/foo.txt --ranges 2-5 --content "替换内容\n" --created-by agent-x --desc "局部更新"
```

| 选项 | 说明 |
|------|------|
| `--content` | 写入内容（必选） |
| `--created-by` | 写入元数据，首次创建时记录（必选） |
| `--desc` | 文件描述（必选） |
| `--ranges START-END` | 1-indexed，含端点；省略则整文件覆盖 |

更新已有文件时，元数据中的 `created_by` / `created_at` 会保留，其余字段更新。

### `list` — 列出文件

```bash
ws list                  # 列出工作区全部文件
ws list docs             # 只列出 docs/ 下文件
ws list --json
ws list docs --json
```

默认输出人类可读表格；`--json` 输出结构化 JSON（含 `scope`、`file_count`、`total_size_bytes`、`files`）。

### `remove` — 删除文件

```bash
ws remove docs/foo.txt
```

同时删除数据文件及对应元数据 sidecar。

### `mcp` — 本地 MCP 服务

以 [Model Context Protocol](https://modelcontextprotocol.io/) 服务运行，通过 **stdio** 收发换行分隔的 JSON-RPC 2.0 消息。MCP 客户端将 `ws mcp` 作为子进程启动后，即可调用工作区工具。

```bash
ws mcp
ws mcp --config /path/to/config.yaml
```

进程读取 stdin 直到 EOF，所有协议输出写入 stdout（因此该模式下命令本身不向 stdout 打印其他内容）。后端与作用域规则由配置文件决定。

支持的方法：`initialize`、`tools/list`、`tools/call`、`ping`，以及忽略的 `notifications/*`。

暴露的工具（每次调用可选 `user_id` / `session_id` 进行作用域隔离）：

| 工具 | 必选参数 | 可选参数 | 说明 |
|------|---------|---------|------|
| `read` | `path` | `ranges` | 读取文件，可按 1-indexed 行区间过滤 |
| `write` | `path`、`content`、`created_by`、`desc` | `ranges`（单个 `START-END`） | 写入或局部替换 |
| `list` | — | `path` | 列出文件，返回 JSON 报告 |
| `remove` | `path` | — | 删除文件及元数据 |

工具**执行**失败（如路径越界、未找到）按 MCP 约定返回 `isError: true` 的结果而非 JSON-RPC 协议错误；仅调用格式本身非法（缺少工具名、未知工具）才返回协议错误。

示例（手动喂入请求）：

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"write","arguments":{"path":"a.txt","content":"hello\n","created_by":"agent","desc":"demo"}}}' \
  | ws mcp
```

在 MCP 客户端中的典型配置（以可执行路径启动）：

```json
{
  "mcpServers": {
    "agent-workspace": {
      "command": "/path/to/ws",
      "args": ["mcp", "--config", "/path/to/config.yaml"]
    }
  }
}
```

也可通过环境变量 `AGENT_WORKSPACE_CONFIG` 指定配置（省略 `--config` 时生效）：

```json
{
  "mcpServers": {
    "agent-workspace": {
      "command": "/path/to/ws",
      "args": ["mcp"],
      "env": { "AGENT_WORKSPACE_CONFIG": "/path/to/config.yaml" }
    }
  }
}
```

> 启动目录、`--config` 或 `AGENT_WORKSPACE_CONFIG` 需指向有效的 `config.yaml`，否则服务无法加载后端。

## 元数据

每个数据文件对应一个 sidecar，命名规则：`foo.txt` → `foo.txt.meta.yaml`。

示例：

```yaml
relative_path: docs/foo.txt
created_by: agent-x
desc: 需求文档草稿
created_at: 2026-06-13T13:00:00+08:00
updated_at: 2026-06-13T13:35:00+08:00
size_bytes: 1024
sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
```

## 退出码

| 码 | 含义 |
|----|------|
| 0 | 成功 |
| 1 | 一般错误 |
| 2 | 路径非法 / 越界 |
| 3 | 未找到（含元数据保护触发） |
| 4 | 锁冲突 |

错误信息输出到 stderr。

## 并发说明

### File 后端

文件锁为进程级 advisory lock，仅对同样使用本工具的进程有效。同一文件的并发 `write` 会串行化；`write` 在独占锁下同时更新数据文件和元数据 sidecar，保证一致性。

### MySQL 后端

`write` / `remove` 在事务内对目标行执行 `SELECT ... FOR UPDATE`，依赖 InnoDB 行锁串行化并发写入。锁等待超时或死锁时返回退出码 **4**（`LockConflict`），与 file 后端的锁冲突行为一致。`read` 为普通 SELECT，不加行锁。

## 开发

```bash
cargo test          # 单元测试 + 集成测试（不含需 MySQL 的 ignored 测试）
cargo run -- list   # 开发模式运行
```

### 项目结构

源码按**领域/特性**组织，每个概念对应一个清晰位置：

```
src/
├── main.rs       仅 fn main()，调用 cli::run()
├── lib.rs        模块声明
├── error.rs      错误类型与退出码
├── lock.rs       file 后端的 advisory 文件锁
├── cli.rs        Cli/Commands 定义、命令分发、按作用域打开后端
├── scoping.rs    SessionScope（user/session 作用域解析）
├── ranges.rs     行区间解析、写入替换、过滤
├── metadata.rs   FileMetadata、sidecar 读写、SHA256/时间戳
├── paths/        路径领域
│   ├── normalize.rs     工作区相对路径归一化
│   ├── resolve.rs       路径解析与越界校验
│   ├── metadata_name.rs sidecar 命名与识别
│   └── scope_prefix.rs  list 作用域前缀匹配
├── config/       配置领域
│   ├── mod.rs       Config / BackendConfig
│   ├── raw.rs       反序列化 DTO 与默认值
│   ├── load.rs      配置发现、加载、校验
│   └── templates.rs init 写出的配置模板
├── storage/      存储领域
│   ├── mod.rs       WorkspaceBackend trait + ListReport
│   ├── handle.rs    BackendHandle 枚举与工厂
│   ├── file.rs      file 后端
│   ├── scoped.rs    带作用域的 mysql 后端包装
│   └── mysql/       mysql 后端（connection 连接层 + mod CRUD 实现）
├── mcp/          本地 MCP 服务
│   ├── mod.rs       模块入口
│   ├── protocol.rs  JSON-RPC 2.0 消息类型与错误码
│   ├── server.rs    stdio 同步循环与方法分发
│   └── tools.rs     工具定义与执行（映射到 WorkspaceBackend）
└── commands/     各子命令实现（init/read/write/list/remove）
```

### MySQL 集成测试（可选）

需要本地或 CI 中可访问的 MySQL 实例。设置 `MYSQL_TEST_URL` 后运行 ignored 测试：

```bash
export MYSQL_TEST_URL='mysql://user:pass@localhost:3306/agent_workspace_test'
cargo test --test mysql_integration -- --ignored
```

未设置 `MYSQL_TEST_URL` 时，`cargo test` 会跳过这些测试，不影响常规 CI。

## 快速上手

```bash
# 新建工作区
ws init ./my-workspace
cd my-workspace

# 写入、读取、列出、删除
ws write docs/readme.md --content "# 标题\n" --created-by me --desc "示例文件"
ws read docs/readme.md --human
ws list
ws remove docs/readme.md
```
