# 2026-11-25 Claude 会话标题

| 项 | 内容 |
| --- | --- |
| 状态 | 已验证 |
| 类型 | 变更文档 |
| 关联产品规格 | [agent-sources.md R1](../../product-specs/agent-sources.md) |
| 关联设计 | [SOURCES.md](../../design-docs/source-designs/SOURCES.md)、[Claude 标题读取范围 ADR](../../design-docs/adr/claude-session-title-read-scope.md)、[SECURITY.md](../../../SECURITY.md) |
| 关联 ticket | [#15](https://github.com/sbwkl/perch/issues/15)、[#16](https://github.com/sbwkl/perch/issues/16) |

## 背景

Claude Code CLI 会话常因缺少 JSONL 原生标题而在 Perch 中显示为「新会话」。`/resume` 能为会话显示简介，但 Perch 当前只读取 JSONL 的 `custom-title` / `ai-title`。

## 目标行为

标题优先级：JSONL 最后一条非空 `custom-title` → `<sessionId>/custom-title.json` 的 `customTitle` → JSONL 最后一条非空 `ai-title` → 首条候选用户消息派生的单行标题（`derived`）→ `新会话`。

派生标题仅检查首条候选用户消息；跳过 sidechain、`isMeta`、`toolUseResult`、系统注入包装和 slash command 包装。首条候选清洗后为空时立即回退 `新会话`，不继续查找后续消息。标题折叠空白并截断到 60 个 Unicode 字符。

不使用 `agentName`、`summary` / compaction summary 或 `history.jsonl`。每次扫描完整重算标题；原生标题或 sidecar 消失时不保留旧值。缓存只保存标题和既有元数据，不保存正文。

## 验收

### 正常路径

1. JSONL `custom-title` 高于 JSONL `ai-title`，`title_source` 分别为 `custom` / `ai`。
2. 无原生标题时，首条候选用户消息生成 `derived` 标题；单行、折叠空白、60 字符截断。
3. sidecar 的 `customTitle` 位于 JSONL `custom-title` 之后、`ai-title` 之前。
4. sidecar 创建、修改或删除后，`SourceProbe::changed_sources` 能在去抖后给出 `claude`。
5. 标题变化进入任务指纹，缓存能更新。

### 异常与边界

1. 首条候选是 sidechain / meta / tool result / 系统注入 / slash command 且清洗后为空：显示 `新会话`，不查找后续消息。
2. sidecar 为空、空白或 JSON 损坏时忽略，继续下一层候选。
3. 其他 JSON sidecar 变化不触发 Claude 重扫。
4. 原生标题或 sidecar 被删除后重扫，标题回退到当前优先级下的下一候选。
5. 所有 fixture 只使用合成数据；扫描前后源文件字节一致。

## 回滚

回滚本变更会恢复「无原生标题即显示 `新会话`」以及仅 JSONL 标题签名探测；不影响源 agent 数据，删除 `cache.db` 后重新扫描可重建旧快照。

## 验证记录

- 2026-11-25：TDD 完成。`Collector::scan` 集成测试覆盖原生标题优先、sidecar 优先级、空白 / 损坏 / 删除 sidecar、首条候选派生、侧链 / meta / tool result / 系统注入 / slash command 跳过、首条候选为空即回退、61+ 字符截断到前 60 个 Unicode 字符、原生标题接管与回退、源文件字节不变。
- 2026-11-25：产品规格、Claude 来源设计、README、安全说明、架构、FAQ 与核心信念已同步说明 Claude-only 放宽标题读取范围、缓存仍最小化。
- 2026-11-25：`SourceProbe::changed_sources` 集成测试覆盖 `custom-title.json` 创建、修改、删除触发 Claude 重扫，其他 JSON 不触发。
- 2026-11-25：新增语义化 ADR 并登记到设计文档索引；产品规格、安全说明、架构、FAQ 与变更文档状态一致。
- 2026-11-25：`cargo fmt --all --check` 通过；`cargo clippy -p perch-core --all-targets -- -D warnings` 通过；`cargo test -p perch-core` 通过；Markdown 本地链接检查 0 missing。
