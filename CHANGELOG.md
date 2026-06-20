# 更新日志

## 0.1.2 - 2026-06-21

让 codex→claude 迁移的会话能在 Claude 桌面 App（实测 2.1.181 / 2.1.183）里正常显示并继续对话，修复三个独立问题：

- **发消息报 `Invalid request`**：迁移到 Claude 时不再输出 `thinking` 块。Anthropic API 要求历史中的 `thinking` 块携带原始加密 `signature`，迁移伪造的（甚至空的）thinking 块会使下一轮请求被 400 拒绝；reasoning 无法携带合法签名，故丢弃，并保持 parentUuid 链完整。
- **会话不出现在历史列表**：桌面 App 读取的是自身注册表 `claude-code-sessions/<账号>/<工作区>/local_<id>.json` 而非 `~/.claude/projects/*.jsonl`。迁移到 Claude 时自动写入该注册文件（复用同 `cwd` 已有会话的工作区目录，找不到则跳过），权限取最保守默认值，不预授权任何工具或目录。
- **模型 id 兼容性**：不再把源 provider 的非 Claude 模型 id（如 `gpt-5.5`、`<synthetic>`）写入 Claude 的 `model` 字段，统一回退到 `--claude-model`；claude→claude 迁移仍保留原 Claude 模型。

附带：迁移会话标题改为取第一条真实用户消息，跳过注入的权限/开发者样板。

## 0.1.0 - 2026-06-19

- 首次公开发布。
- 支持 Claude Code 与 Codex 双向会话迁移。
- 支持文本、推理、图片、工具调用和工具结果。
- 支持扫描、检查、迁移、便携包导入导出和环境诊断。
- 增加原子写入、备份、回读验证和 Codex SQLite schema 探测。
- 修复 Claude 图片块、`stop_reason` 和助手 `model` 兼容问题。
- 增加短入口 `sw` 和常用短命令：`tc`、`tx`、`ls`、`show`、`ok`、`pack`、`unpack`、`env`。
