# 贡献指南

感谢你改进 Session Weaver。

## 开发流程

1. Fork 仓库并从 `main` 创建分支。
2. 为行为变化先添加失败测试。
3. 实现最小修复，并运行格式、clippy 和全部测试。
4. PR 中说明客户端版本、复现步骤、预期行为和实际行为。

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## 会话夹具

只提交自行构造或完全脱敏的最小夹具。删除用户名、绝对路径、仓库地址、token、cookie、
API Key、账号 ID、真实对话和商业源码。不要从第三方项目复制测试夹具。

## 兼容修复

兼容问题必须包含：

- Claude Code 或 Codex 的准确版本。
- 能独立触发问题的最小 JSONL。
- 修复前失败、修复后通过的回归测试。
- 对旧格式和未知字段的影响说明。
