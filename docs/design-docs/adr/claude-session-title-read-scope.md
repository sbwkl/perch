# ADR：Claude 会话标题读取范围

| 项 | 内容 |
| --- | --- |
| 状态 | current |
| 领域 | Claude 标题读取范围、缓存最小化、sidecar 生命周期 |
| 当前事实源 | `crates/perch-core/src/collectors/claude.rs`、`SECURITY.md`、`docs/product-specs/agent-sources.md` |

## 背景

Claude Code CLI 会话常缺少 JSONL 原生标题，在 Perch 中统一显示「新会话」。`/resume` 能从其他元数据得到简介，但其中 `agentName` 与 `summary` 不稳定，`history.jsonl` 与会话 transcript 重叠率低；用户要求这些字段都不参与 Perch 标题。Claude 2.1.319 的 `/rename` 结果会写入会话目录下的 `custom-title.json`。

## 决策

- 标题优先级每次扫描完整重算：JSONL `custom-title` → `<sessionId>/custom-title.json` 的 `customTitle` → JSONL `ai-title` → 首条候选用户消息派生的 `derived` 标题 → `新会话`。
- 只为标题派生读取首条候选用户文本；跳过 sidechain、`isMeta`、`toolUseResult`、系统注入包装与 slash command 包装。首条候选清洗后为空时立即回退，不继续向后寻找。
- 缓存 `cache.db` 只保存标题结果和既有任务元数据，不复制正文；其他来源继续遵守既有最小读取范围。
- Claude probe 只监测 basename 为 `custom-title.json` 的文件创建、修改与删除；其他 JSON sidecar 不触发 Claude 重扫。
- 不使用 `agentName`、`summary` / compaction summary 或 `history.jsonl`。

## 后果

- Claude 是唯一放宽读取范围的来源，放宽仅限标题用途；所有源文件仍只读打开，Perch 不创建、修改、重命名或删除 Claude 源数据。
- 原生标题或 sidecar 消失后，下一次扫描会按完整优先级回退，不保留旧人工标题。
- 标题可能包含用户输入片段；日志、截图和 issue 仍需脱敏。
- 其他来源的读取范围与 probe 行为不变。
