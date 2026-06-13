# Agent Workspace (`ws`)

基于 Rust 的受限文件操作 CLI，供 Agent 在指定工作目录内安全地读写文件。所有路径均相对于配置的工作区解析，无法越界访问宿主机其他路径。

## 特性

- **路径隔离**：读写操作限制在 `workspace_dir` 内，含符号链接逃逸检测
- **元数据 sidecar**：每个文件自动维护 `*.meta.yaml`（创建者、描述、时间戳、SHA256）
- **并发安全**：`read` 使用共享锁，`write` / `remove` 使用独占锁
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

使用 `init` 命令创建新的工作区（生成 `config.yaml` 和 `data/` 目录）：

```bash
# 在当前目录初始化
ws init

# 在指定目录初始化（不存在则自动创建）
ws init ./my-agent-workspace
ws init /path/to/workspace
```

初始化完成后，进入该目录即可使用其他命令。若目录下已存在 `config.yaml`，会报错以避免覆盖。

## 配置

在项目根目录（或任意启动目录）放置 `config.yaml`：

```yaml
workspace_dir: ./data         # 工作目录，相对 config 文件所在目录解析
metadata_suffix: ".meta.yaml" # 元数据后缀，可省略（默认值）
```

加载顺序：

1. 若设置了环境变量 `AGENT_WORKSPACE_CONFIG`，使用其指向的配置文件
2. 否则读取**当前工作目录**下的 `config.yaml`

启动时会校验 `workspace_dir` 存在且可写。本仓库默认工作目录为 `./data/`。

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
```

| 参数 | 说明 |
|------|------|
| `[path]` | 可选，目标目录；省略则在当前工作目录初始化 |

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

文件锁为进程级 advisory lock，仅对同样使用本工具的进程有效。同一文件的并发 `write` 会串行化；`write` 在独占锁下同时更新数据文件和元数据，保证一致性。

## 开发

```bash
cargo test          # 单元测试 + 集成测试
cargo run -- list   # 开发模式运行
```

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
