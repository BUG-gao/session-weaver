# Session Weaver

> 安全迁移 Claude Code 与 Codex 会话：保留消息、推理、图片和工具调用，写入前后自动校验。

[![CI](https://github.com/BUG-gao/session-weaver/actions/workflows/ci.yml/badge.svg)](https://github.com/BUG-gao/session-weaver/actions/workflows/ci.yml)
[![License: Non--Commercial](https://img.shields.io/badge/License-Non--Commercial-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org/)

Session Weaver 是一个面向 Claude Code 会话迁移、Codex 会话迁移和 Claude/Codex
双向转换的源码公开命令行工具。它不会只判断“每行是不是合法 JSON”，还会检查目标客户端
真正依赖的字段，并在写入后重新读取验证。

```bash
# Codex -> Claude Code
sw tc <CODEX_SESSION_ID>

# Claude Code -> Codex
sw tx <CLAUDE_SESSION_ID>
```

如果 Session Weaver 帮你保住了一段重要会话，欢迎点一个 Star，让更多需要迁移 AI
编程上下文的人能找到它。

## 为什么需要它

Claude Code 和 Codex 都使用 JSONL 保存会话，但记录结构、内容块、工具调用、索引和
恢复约束并不相同。简单改字段或复制文件可能得到“转换成功、恢复崩溃”的结果。

Session Weaver 针对这些问题提供：

- Claude Code 与 Codex 双向原生会话迁移。
- 文本、推理摘要、图片、工具调用和工具结果转换。
- 迁移前语义校验、目标格式校验和写入后回读。
- 临时文件写入、原子提交、覆盖前备份。
- Claude `history.jsonl`、Codex `session_index.jsonl` 和可选 SQLite 注册。
- 原生会话 ID 或直接 JSONL 路径输入。
- 便携、带 SHA-256 校验的会话包。
- 人类可读输出和适合脚本的 `--json`。

## 已修复的兼容问题

项目包含三项针对 Claude Code 2.1.183 的回归测试：

1. 图片使用合法 `image` 内容块，不伪装成缺少 `text` 的文本块。
2. 普通助手回复写入 `stop_reason: "end_turn"`。
3. 每条助手消息写入非空 `model`。

这些问题无法被普通 JSON 语法检查发现。Session Weaver 的 `check` 命令会验证到字段路径。

**0.1.2** 修复了 codex→claude 迁移后会话不显示在 Claude 桌面 App 历史列表的问题。根因：迁移会把源模型 id（如 `gpt-5.5`、`<synthetic>`）写入 Claude 的 `model` 字段，而 Claude 桌面 App（实测 2.1.181 / 2.1.183）会隐藏无法解析模型的会话。现已将非 Claude 模型回退到 `--claude-model`，claude→claude 迁移仍保留原 Claude 模型不变。

## 兼容矩阵

| 来源 | 目标 | 状态 | 验证方式 |
| --- | --- | --- | --- |
| Claude Code | Codex | 支持 | 解析、往返、索引、Codex 0.142.0-alpha.1 TUI 探测 |
| Codex | Claude Code | 支持 | 解析、往返、字段校验、Claude Code 2.1.183 恢复探测 |
| 便携包 | Claude Code | 支持 | SHA-256、目标校验、回读 |
| 便携包 | Codex | 支持 | SHA-256、目标校验、回读 |

客户端格式会持续变化。建议升级客户端后先运行 `session-weaver doctor` 和
`session-weaver check`。

## 安装

### 从源码安装

```bash
git clone https://github.com/BUG-gao/session-weaver.git
cd session-weaver
cargo install --path .
```

### 下载 Release

在 [Releases](https://github.com/BUG-gao/session-weaver/releases) 下载对应平台压缩包，
解压后把 `session-weaver` 和 `sw` 加入 `PATH`。

要求 Rust 1.88 或更高版本。

## 快速开始

### 按会话 ID 迁移

```bash
sw tc 019d...  # Codex -> Claude Code
sw tx 6367...  # Claude Code -> Codex
```

默认目标目录：

- Claude Code：`CLAUDE_CONFIG_DIR`、`CLAUDE_HOME` 或 `~/.claude`
- Codex：`CODEX_HOME` 或 `~/.codex`

可用 `SESSION_WEAVER_CLAUDE_HOME` 和 `SESSION_WEAVER_CODEX_HOME` 覆盖。

### 按文件迁移

```bash
sw tc ./source.jsonl -o ./tmp/claude-home
```

命令完成后会打印新会话 ID、保存位置和恢复命令。默认不会自动启动客户端；显式传入
`--open` 才会启动。

### 迁移前检查

```bash
sw ok claude <SESSION_ID>
sw ok codex ./rollout.jsonl --json
```

### 查看本机会话

```bash
sw ls claude
sw ls codex --json
sw show codex <SESSION_ID>
```

## 便携包

便携包先把原生会话解析为 Session Weaver 的类型化模型，再保存版本和摘要，不是简单复制
原始 JSONL。

```bash
sw pack claude <SESSION_ID> ./session.sw.json
sw unpack codex ./session.sw.json
```

导入时会验证 schema、版本和 SHA-256 摘要。摘要不一致时拒绝写入。

## 命令

| 命令 | 用途 |
| --- | --- |
| `tc` / `to-claude` | Codex -> Claude Code |
| `tx` / `to-codex` | Claude Code -> Codex |
| `ls` | 扫描本机 Claude/Codex 会话 |
| `show` | 查看会话元数据和事件数量 |
| `ok` | 执行语义与目标兼容检查 |
| `pack` | 导出 Session Weaver 便携包 |
| `unpack` | 从便携包生成原生会话 |
| `env` | 查看 Rust、客户端版本和存储根目录 |
| `move` / `scan` / `inspect` / `check` / `export` / `import` / `doctor` | 兼容旧脚本的完整命令 |

完整参数：

```bash
sw --help
sw tc --help
```

## 数据安全

- 默认不删除源会话。
- 已存在目标默认拒绝覆盖。
- `--overwrite` 覆盖前创建带时间戳的备份。
- 主会话先写临时文件、flush、sync，再原子重命名。
- 目标写入后重新解析，失败则报告目标验证错误。
- SQLite 使用事务并先探测当前表字段。
- 工具不迁移账号、API Key、OAuth 凭证或计费数据。

会话内容可能包含源码、密钥、个人信息和终端输出。提交 Issue 时只上传最小、脱敏的复现
数据，不要公开真实会话。

## 当前边界

- 首个版本只支持 Claude Code 与 Codex。
- 不迁移 Claude 子代理目录、shell snapshot、缓存或客户端账号状态。
- Codex 加密推理内容和客户端专有计费字段不进入便携模型。
- 未知内容块会尽量保留；无法安全表达的块会产生诊断。
- 真实兼容性基于测试报告中的明确客户端版本，不代表未来版本永久兼容。

## 测试

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

测试覆盖模型、字段校验、解析、生成、CLI、路径发现、SQLite、便携包、双向往返、随机文本、
损坏输入和真实客户端启动探测。

详细结果见 [测试报告](docs/TEST_REPORT.zh-CN.md)。

## 参与贡献

欢迎提交新客户端版本的脱敏夹具、兼容问题和平台测试结果。开始前请阅读
[CONTRIBUTING.md](CONTRIBUTING.md) 和 [SECURITY.md](SECURITY.md)。

路线图：

- 扩大 Claude/Codex 历史版本夹具矩阵。
- 增加结构化迁移差异报告。
- 提供可审计的兼容规则版本表。
- 评估更多本地 AI 编程客户端。

## 许可证

[Session Weaver Non-Commercial Source License](LICENSE)。本项目允许个人、教育、研究和评估等非商业用途；未经版权持有人书面许可，不允许商业使用。
