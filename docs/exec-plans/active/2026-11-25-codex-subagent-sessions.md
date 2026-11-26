# 2026-11-25 Codex 子会话（subagent / tui_side_chat）

| 项 | 内容 |
| --- | --- |
| 状态 | 开发中 |
| 类型 | 变更文档 |
| 关联产品规格 | [agent-sources.md R21](../../product-specs/agent-sources.md) |
| 关联设计 | [SOURCES.md Codex 章节](../../design-docs/source-designs/SOURCES.md)、[GLOSSARY.md](../../../GLOSSARY.md)、[FAQ.md](../../FAQ.md) |
| 关联 ticket | [#18](https://github.com/sbwkl/perch/issues/18)、[#19](https://github.com/sbwkl/perch/issues/19)、[#20](https://github.com/sbwkl/perch/issues/20)、[#21](https://github.com/sbwkl/perch/issues/21) |

## 背景

Codex 会把子代理派生的会话也当作独立任务采集。这些会话大多没有正式标题，在主列表里显示为大量
「新会话」，把真正需要跟进的主任务淹没、造成任务碎片化。Perch 已有子会话机制
（Qoder / Cline / OpenCode / Kilo 已接入），本次把 Codex 纳入同一口径，前端与数据结构零改动。

## 目标行为

- 识别：`thread_source = subagent` 的 rollout 是子代理会话；仅当 `thread_source` 缺失时，
  才以首行 `source` 为含 `subagent` 键的对象兜底判定，`thread_source` 为其他值
  （如 `user` / `automation`）时一律不兜底。`thread_source = tui_side_chat` 的侧聊会话
  同为子会话。
- 父链接：subagent 优先 `parent_thread_id`、缺省退 `forked_from_id`；side_chat 优先
  `forked_from_id`、缺省退 `history_base.thread_id`。父 id 非空且不等于自身 id 才写入
  `parent_key`（自身以 rollout `payload.id` 为准）；父会话不在本次扫描结果中时照样写入。
- 防误伤：其余会话（`user` / `automation` / 未被识别为子会话）即使带 `forked_from_id`
  也保持主任务，不建父子关系。
- 归类不受层级与位置影响：多层级派生以即时父为准，嵌套深度不影响归类；归档目录
  `archived_sessions/` 下的子代理会话同样按子会话处理。
- 标题：`session_index.thread_name` → `thread_goals.objective` →（subagent 且
  `agent_nickname` 非空）「子代理 · <昵称>」（`title_source = derived`，截断 240 字符）
  → `新会话`（`fallback`）。
- 展示沿用既有机制：子会话不进主列表、不计入来源 / 工作区 / 最近 / 更早计数，仍在快照中、
  可被搜索，详情与搜索结果带「子会话」徽标。
- `agent_kind` 与继续工作不变：subagent / tui_side_chat 仍按 `originator` 归类，
  继续工作沿用既有通道（按 `agent_kind` 打开或激活 Codex），不为子会话另设深链。

## 验收

### 正常路径

1. subagent 会话带 `parent_thread_id` 与 `agent_nickname`：`parent_key` 指向父任务，
   标题为「子代理 · <昵称>」（`derived`）。
2. subagent 会话仅有 `forked_from_id`：父链接回退 `forked_from_id`。
3. 侧聊会话带 `forked_from_id`：父链接生效；仅有 `history_base.thread_id` 时同样回退生效。
4. 有 `session_index` 标题的 subagent 会话仍用原标题（`source`），昵称派生不接管。

### 异常与边界

1. `user` / `automation` / 未带子代理或侧聊标记且带 `forked_from_id` 的会话保持主任务，
   不建父子关系。
2. 父 id 空白或等于自身 id 时不建父子关系；子代理 rollout 的 `session_id` 指向父、
   `payload.id` 才是自身。
3. 父会话不在本次扫描结果中：子会话仍写入父链接并隐去，可被搜索。
4. 多层级派生（子代理再派生会话）：以即时父为准，深度不影响归类。
5. 归档目录下的子代理会话同样按子会话处理。
6. subagent 无 `agent_nickname` 时标题回退「新会话」（`fallback`）。
7. 所有 fixture 只用合成数据；扫描前后源文件字节一致。
8. `thread_source` 明确为其他值（如 `user`）时，即使首行 `source` 含 `subagent` 键，
   也不建父子关系、不派生昵称标题（`source` 兜底仅在 `thread_source` 缺失时生效）。

## 回滚

回滚本变更会恢复「Codex 子代理 / 侧聊会话以独立任务进入主列表」的旧行为。父子关系复用既有
`parent_key` 能力，不新增表 / 列、不涉及数据迁移；不影响源 agent 数据，删除 `cache.db`
后重新扫描可重建快照。

## 验证记录

- 2026-11-25：T1（#19）父链接与 T2（#20）标题派生已合入集成分支 tip `4290cf7`；collector
  集成测试覆盖上述正常 / 异常 / 边界各条（子代理、侧聊与 `history_base` 回退、user fork
  防误伤、父缺失、空白与自身父 id、多层级、归档目录、无昵称回退、索引标题优先）。
- 2026-11-25：Rust 门禁（`cargo fmt --all --check` / `cargo clippy -p perch-core --all-targets
  -- -D warnings` / `cargo test -p perch-core`）在 `4290cf7` 通过；T3（#21）仅同步文档，不改代码。
- 2026-11-25：SOURCES.md Codex「子会话与自动化」与「标题优先级」、产品规格 R21、FAQ 子会话清单
  与 Claude 表述修正、GLOSSARY「子会话」词条、CHANGELOG 条目已同步，措辞与 `codex.rs`
  实现逐条核对一致。
- 2026-11-25：按规格收紧 `source.subagent` 兜底门控（仅 `thread_source` 缺失时生效），
  新增用例 `codex_explicit_thread_source_blocks_subagent_source_fallback`；dedupe 用例
  升级为快照全部 fixture 文件，断言扫描前后源文件字节一致。SOURCES.md 与本文档的识别口径
  同步改为门控语义。
