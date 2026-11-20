# REQ-0002 会话来源覆盖对齐 usage-ledger

类型：REQ
状态：活跃
Owner：wayne
关联：CHG-0004

## 1. 摘要

Perch 侧边栏与任务列表应覆盖用户实际在用的全部 AI 编码 Agent。当前 Perch 内置 7 个来源（qoder、claude、codex、opencode、kilo、trae-cn、cline），而同机的 usage-ledger 已支持 11 个来源；Perch 需要跟进补齐缺失的来源，使"最近的会话任务"与"用量报告"覆盖一致。

## 2. 问题与证据

- 谁、什么场景：用户在 Perch 查看"最近任务"，并查看 usage-ledger 用量报告。
- 现在怎么做、代价是什么：Antigravity、MiniMax Code、CodeArts（码道）、CodeBuddy CN 四个 Agent 的会话不出现在 Perch 任务列表，但它们的用量出现在内嵌报告中，视图割裂。
- 证据：usage-ledger `SOURCES`（11 个，`src/usage_ledger/cli.py:32`）对比 Perch `ALL_SOURCES`（7 个，`crates/perch-core/src/models.rs`）；差异为 antigravity、codearts、minimax、codebuddy（usage-ledger CHG-0016/0017/0018/0019，2026-11-20 已全部合入其 main）。
- 不做的后果：报告里有用量、任务列表里没有会话，用户需要在另一套工具里查看这四个 Agent 的工作记录。

## 3. 目标与非目标

- 目标：Perch 任务列表覆盖与 usage-ledger 一致的来源集合；每个来源在 UI 有正确的名称、颜色与图标；usage 报告的 `PERCH_SOURCES` 子集与之同步。
- 非目标：复制 usage-ledger 的 token 统计逻辑到 Perch（用量仍由 usage-ledger 唯一渲染）；为未在 usage-ledger 登记的 Agent 新建来源；来源自动发现/插件化。

## 4. 成功指标

- 主指标：`ALL_SOURCES` 与 usage-ledger `SOURCES` 一致（当前 11 个）。
- 护栏指标：每个新增来源有采集器、探测（probe）、continue/resume 行为、UI 名称/颜色/图标、`docs/SOURCES.md` 条目；`cargo test --workspace` 全绿。
- 验证方式：临时目录 fixture 单元测试 + 实机冒烟（见 CHG-0004 §5）。

## 5. 当前需求与验收（EARS）

- R1：When 某来源的本地数据目录存在, the system shall 扫描其会话并在任务列表展示，字段语义与既有来源一致（标题、项目、时间、状态）。
- R2：If 某来源的数据目录不存在, then the system shall 将该来源标记为不可用，不影响其他来源与其他功能。
- R3：When 用户对某来源的会话触发"继续/打开", the system shall 按该 Agent 的方式恢复会话（deeplink、命令行或目录激活）。
- R4：When usage-ledger 报告包含某来源的用量, the system shall 将该来源计入 `PERCH_SOURCES` 用量子集。
- R5：When 数据变化（文件 mtime 变化）, the system shall 通过对应探测类型（SQLite/JSONL/日志）触发增量重扫。

## 6. 变更索引

| CHG ID | 标题 | 状态 | 链接 |
|---|---|---|---|
| CHG-0004 | 接入 Antigravity / MiniMax / CodeArts / CodeBuddy 四个来源 | 已验证 | [../changes/CHG-0004-usage-ledger-agents.md](../changes/CHG-0004-usage-ledger-agents.md) |

## 7. 链接

- 来源定义（对齐基准）：usage-ledger `src/usage_ledger/collectors.py`、`docs/changes/CHG-0016..0019`
- Perch 扩展点：`ARCHITECTURE.md` §"扩展点：新增一个来源"
- 既有来源文档：`docs/SOURCES.md`