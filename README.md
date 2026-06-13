# Agent Workspace (`ws`)

基于 Rust 的受限文件操作 CLI，供 Agent 在指定工作目录内安全地读写文件。所有路径均相对于配置的工作区解析，无法越界访问宿主机其他路径。

## 特性

- **双后端**：本地文件（`file`）或 MySQL（`mysql`），通过 `config.yaml` 切换
- **路径隔离**（file 后端）：读写操作限制在 `workspace_dir` 内，含符号链接逃逸检测
- **元数据**：file 后端使用 `*.meta.yaml` sidecar；mysql 后端将元数据存入数据库
- **并发安全**：file 后端使用 advisory 文件锁；mysql 后端使用 InnoDB 行锁
- **按行操作**：支持按行区间读取或局部替换写入

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

1. 若设置了环境变量 `AGENT_WORKSPACE_CONFIG`，使用其指向的配置文件
2. 否则读取**当前工作目录**下的 `config.yaml`

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

内容来自 stdin 或 `--content`：

```bash
echo "hello" | ws write docs/foo.txt --created-by agent-x --desc "需求草稿"

ws write docs/foo.txt --content "hello\n" --created-by agent-x
```

按行区间局部替换（删除 `START..END` 行，在该位置插入新内容）：

```bash
ws write docs/foo.txt --ranges 2-5 --content "替换内容\n"
```

| 选项 | 说明 |
|------|------|
| `--ranges START-END` | 1-indexed，含端点；省略则整文件覆盖 |
| `--created-by` | 写入元数据，首次创建时记录 |
| `--desc` | 文件描述 |
| `--content` | 直接指定内容；省略则从 stdin 读取 |

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
echo "# 标题" | ws write docs/readme.md --created-by me --desc "示例文件"
ws read docs/readme.md --human
ws list
ws remove docs/readme.md
```
