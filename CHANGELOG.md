# 更新日志

## 0.1.2 - 2026-06-21

- 修复 codex→claude 迁移后会话在 Claude 桌面 App 历史列表中不显示的问题：迁移时会把源 provider 的非 Claude 模型 id（如 `gpt-5.5`、`<synthetic>`）原样写入 Claude 的 `model` 字段，App 无法解析该模型而隐藏整个会话；现在非 Claude 模型一律回退到 `--claude-model`，claude→claude 迁移仍保留原 Claude 模型不变。

## 0.1.0 - 2026-06-19

- 首次公开发布。
- 支持 Claude Code 与 Codex 双向会话迁移。
- 支持文本、推理、图片、工具调用和工具结果。
- 支持扫描、检查、迁移、便携包导入导出和环境诊断。
- 增加原子写入、备份、回读验证和 Codex SQLite schema 探测。
- 修复 Claude 图片块、`stop_reason` 和助手 `model` 兼容问题。
- 增加短入口 `sw` 和常用短命令：`tc`、`tx`、`ls`、`show`、`ok`、`pack`、`unpack`、`env`。
