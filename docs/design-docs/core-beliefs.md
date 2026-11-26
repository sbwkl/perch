# 核心工程信念

本文件定义 `docs/design-docs/` 的 agent-first operating principles。它不是产品文案，而是约束设计与实现取舍的工程原则。

## 1. 源是唯一事实来源

- 各 agent 自己落盘的数据库 / JSONL / 事件日志才是真值。
- Perch 的 `cache.db` 只是可重建的快照索引，不是权威副本。
- 扫描失败时保留旧快照；不得把缺失数据误判为“无任务”。

## 2. 只读优先

- 源 SQLite 只用只读连接并启用 `query_only`；源 JSONL / JSON 只读打开。
- 不写源目录、不回写 agent 状态、不重命名或删除源文件。
- `continue_task` 只能按固定模板打开会话深链或激活 App，前端不能传任意命令或 URL。
- 对话正文只在用户打开视图时按需、只读、内存内读取（单次快照，关闭即丢弃）；
  不做后台预读、正文索引或落盘。

## 3. 元数据最小化

- 只读取任务列表所需元数据；不存消息正文、prompt、tool 输出、附件或思考过程。
- 读取范围只有两个显式放宽：对话正文视图按需读取可见文本（按需、只读、内存内、单次快照）；
  Claude 标题派生读取首条候选用户文本，只保留单行标题结果。依据见
  [ADR：按需读取会话正文](adr/session-transcript-on-demand-read.md)。
- Claude todo 只读 `subject/status`；Trae memory 只读文件名、日期、mtime 与首条 `intent` 摘要。
- 状态推断必须有上游证据，缺失数据不伪造。

## 4. 四轨分离，永不混加

- token 真值、credits 额度、est 估算、activity 活动指标分别记录、分别展示。
- 禁止把 credits 当 token，禁止把 est 与真值相加，禁止跨来源静默合计。

## 5. 缓存可重建，覆盖层独立

- 删除 `cache.db` 只能导致重建，不能影响任何 agent。
- 未来的置顶、标签、备注、隐藏等用户数据只写独立 `overlay.db`，扫描永不写覆盖层。

## 6. 仓库是系统记录

- 聊天、issue、PR 中的结论若未回写仓库，对 agent 等于不存在。
- 设计理由、决策链和验证证据必须进入版本库。
- `AGENTS.md` 只做地图，详细事实源放在仓库内的 specs / architecture / design docs / plans 中。

## 7. 渐进披露

- `AGENTS.md` 短小稳定，只提供入口与硬规则。
- `ARCHITECTURE.md` 提供顶层实现图。
- `docs/design-docs/index.md` 提供设计知识目录与状态。
- agent 按需深入具体文档，而不是一开始吞下全部细节。

## 8. 机械约束优于口头规范

- CI / 测试 / 结构检查应尽可能 enforce 文档结构与行为约束。
- 规范若不能被检查，就容易漂移。
- 文档状态、事实源指针、目录结构都应可验证。
