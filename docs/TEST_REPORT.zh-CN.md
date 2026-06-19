# Session Weaver 0.1.0 测试报告

测试日期：2026-06-19

## 环境

- macOS
- rustc 1.95.0
- cargo 1.95.0
- Claude Code 2.1.183
- Codex CLI 0.142.0-alpha.1

## 自动化测试

执行：

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

覆盖范围：

- 类型化会话模型序列化与语义比较。
- 空模型、孤立工具结果、重复 ID 和无原生 ID。
- Claude/Codex 消息、图片、推理、工具调用和结果解析生成。
- Claude 图片块、`end_turn`、`tool_use` 和 model 三项回归。
- 路径发现、重复 ID、原子写入、原生索引和 SQLite 动态列。
- 便携包版本与 SHA-256 篡改检测。
- CLI help、短入口 `sw`、短迁移命令、doctor、check 和原生 ID 解析。
- 两种原生格式往返与随机文本属性测试。
- 损坏 Claude 文本块、缺失 model 和空 stop reason。

## 外部样本黑盒测试

额外兼容性样本仅在仓库外作为黑盒输入使用，没有复制进本项目。Session Weaver
成功解析多组 Claude 样本和 Codex 样本，并完成两个方向的生成与 `check`。

黑盒过程中发现并修复：Codex 消息没有原生事件 ID 时，不应把多个空 ID 判为重复。

## 真实客户端探测

### Claude Code

在隔离 `CLAUDE_CONFIG_DIR` 中生成 cwd 与测试目录一致的会话，以 `--resume --print` 恢复，
并把预算限制为 `0.000001 USD`。Claude 成功完成会话发现和反序列化，最终因预算限制退出。
调试日志未出现：

- `undefined.includes`
- 反序列化 `TypeError`
- 缺少 model
- 非法 stop reason

### Codex

在隔离 `CODEX_HOME` 中生成会话和索引，通过伪终端运行 `codex resume`。进程进入交互 TUI，
8 秒后由测试主动终止；此前未出现会话不存在、JSONL 解析或反序列化错误。

## 限制

真实客户端探测不发送完整付费对话，不验证模型回答质量。CI 无法访问用户客户端和认证，
因此真实客户端探测由 `scripts/compat-smoke.sh` 在本机显式运行，CI 运行全部离线测试。
