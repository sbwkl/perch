# 产品规格：Perch 会话来源覆盖

| 项 | 内容 |
|---|---|
| 原需求卡 | REQ-0002 |
| 文档类型 | 产品规格 |
| 当前版本 | v0.1.0 |
| 适用范围 | 当前来源覆盖行为与验收规格 |
| 关联 | [CHG-0004](../exec-plans/completed/CHG-0004-usage-ledger-agents.md)、[侧栏来源溢出修复](../exec-plans/completed/2026-11-22-sidebar-source-overflow.md)、[真实体验确定性缺陷修复](../exec-plans/completed/2026-11-22-real-ux-clarity-fixes.md)、[P0/P1 全量修复](../exec-plans/completed/2026-11-22-real-ux-p0-p1.md)、[侧边栏界面收敛](../exec-plans/completed/2026-11-22-sidebar-simplification.md)、[近 7 天侧栏排序](../exec-plans/completed/2026-11-22-sidebar-recent-activity-order.md)、[删除临时目录后缀](../exec-plans/completed/2026-11-22-remove-temporary-workspace-suffix.md) |

## 1. 摘要

Perch 当前覆盖 11 个本地 AI 编码 Agent 来源，与 usage-ledger 的来源集合一致。来源的会话元数据以只读方式进入统一任务列表；界面保持简洁：任务分组只保留最近 / 更早 / 已归档 / 搜索，用量报告仅由页脚图标进入，来源行只显示必要状态与当前视图匹配数，不显示触发方式 / 最近扫描时间或说明文字。历史“补齐四个来源”的变更过程见 CHG-0004，本文只陈述当前产品行为与验收。

## 2. 当前事实与边界

- 当前 `ALL_SOURCES` 共 11 个：qoder、claude、codex、opencode、kilo、trae-cn、cline、antigravity、minimax、codearts、codebuddy。
- 每个来源有采集器、默认路径与环境变量覆盖、probe、continue 决策、前端名称 / 颜色 / 图标，以及 fixture 测试。
- 用量报告由 usage-ledger 的 `usage html` 生成并在 Perch 应用内 iframe 展示；报告仅由侧边栏页脚图标进入，任务分组不显示报告导航项。
- 任务导航为「最近 / 更早 / 已归档 / 搜索」；内部视图仍包含 `report`，由页脚图标触发；workspace / 来源筛选叠加在三种任务视图上。侧边栏不显示 workspace 搜索框、来源触发方式 / 更新时间、来源说明和只读采集脚注。
- 来源侧栏数字、smart list 数字与 workspace 数字均按当前视图和其它筛选联动；筛选 chip 明示组合条件，列表副标题展示当前视图数量。Workspaces / 来源列表按全局近 7 天会话数倒序，排序键不显示为可见文字；workspace 名称不追加临时目录、未关联项目或分组后缀，重名时仅显示可区分的路径父目录。

## 3. 成功指标

- 主指标：`ALL_SOURCES` 与 usage-ledger `SOURCES` 一致（当前 11 个）；文档中当前状态描述与代码一致。
- 护栏指标：不写源数据；单源失败不影响其他来源；失败保留旧快照；`continue_task` 仍只接受任务 key；四轨用量分轨展示。
- 验证方式：临时目录 fixture 单元测试 + demo UI 验收 + 当前文档 / 代码核对。

## 4. 当前需求与验收（EARS）

- R1：When 某来源的本地数据目录存在, the system shall 扫描其会话并在任务列表展示，字段语义与既有来源一致（标题、项目、时间、状态）。
- R2：If 某来源的数据目录不存在或采集失败, then the system shall 标记来源状态并保留旧快照，不影响其他来源与其他功能；状态需在侧栏可读暴露，不只依赖颜色。
- R3：When 用户对某来源的会话触发“继续工作”, the system shall 按该 Agent 的固定方式恢复会话（deeplink、命令行或目录激活）；点击前需展示通道与是否可定位会话，命令 / URL 不得来自前端。
- R4：When usage-ledger 报告包含某来源的用量, the system shall 将该来源计入 `PERCH_SOURCES` 用量子集。
- R5：While 窗口可见且实时刷新未暂停, the system shall 每 3 秒检查 SQLite `data_version`，按不超过 15 秒的节流周期计算 JSONL / 日志目录签名；When 变化源经去抖 / 冷却 / 最长等待判定需要更新, the system shall 只重扫该来源；手动刷新期间后台 live scan 不争用 engine 锁。
- R11：When 用户暂停 / 恢复实时刷新, the system shall 通过固定窄命令切换；While 暂停中, the system shall 更新变化基线但不触发 live scan；来源行不显示触发方式或更新时间。
- R12：When 扫描开始 / 结束, the system shall 在应用内显示来源、触发方式与耗时；后台更新与手动刷新可区分。
- R13：When 任意任务视图超过 60 条, the system shall 分页渲染并提供“显示更多”；「待关注」不作为用户可见导航项，`Counts.attention` 仅保留在 payload 供后续诊断。
- R14：When 搜索有 N 个命中, the system shall 返回全部 N 条，并在弹层按 50 条分页展开；When 打开命中结果, the system shall 自动展开到该任务所在页并定位。
- R15：When 视图 / workspace / 来源筛选组合变化, the system shall 让侧栏计数联动当前口径并显示可清除 chip；When workspace 名称重复或来自临时 / 无项目路径, the system shall 用文字区分并归组；Workspaces 分组不显示搜索框。
- R16：While 侧栏 Workspaces / 来源列表渲染, the system shall 按近 7 天非归档、非缺失、主会话数倒序；同数时按最近活动时间倒序，最后按名称 / 特殊目录稳定排序；排序不受当前视图 / 筛选影响，且不显示排序计数文字。
- R17：When Workspaces 或来源列表超过 8 项, the system shall 折叠排序后的后续项并提供「显示更多 / 收起」；When 当前选中项排在折叠区, the system shall 仍显示该选中项。
- R18：When Workspaces 列表渲染, the system shall 不显示 `[临时目录]`、`[未关联项目]` 或任何 workspace 分组后缀；When 名称重复, the system shall 仅显示可区分的路径父目录后缀；When 排序时, the system shall 不按路径类型把 workspace 后置。
- R6：When 来源数量增加使侧栏导航内容超过可用高度, the system shall 在导航内容区内部滚动并保持侧栏页脚固定，不得压缩、覆盖或裁切来源项。
- R7：When 窗口缩小到最小尺寸 `900×560`, the system shall 允许用户通过滚动与键盘到达全部来源、来源说明和固定页脚。
- R8：When 用户切换任务视图、workspace 或来源筛选, the system shall 不保留不属于当前列表的 Inspector 选中任务；When 搜索结果打开对应任务, the system shall 仍定位并选中该任务。
- R9：When 用量报告打开, the system shall 标识当前报告视图为全局报告、提供返回任务路径，且不启动外部浏览器；任务分组不显示报告导航项。
- R10：While 当前 workspace / 来源筛选生效, the system shall 在搜索弹层明示搜索作用域；When 无筛选时, the system shall 明示忽略任务视图并搜索全部会话（含归档）。

验证位置：R1–R5 见来源采集器 / probe / continue 测试与当前代码核对；R6–R7 见侧栏布局实机验证；R8–R18 见本次变更文档的 core / Tauri 测试、demo UI 验收与 `npm run check`。

## 5. 变更索引

| 变更 | 标题 | 状态 | 链接 |
|---|---|---|---|
| CHG-0004 | 接入 Antigravity / MiniMax / CodeArts / CodeBuddy 四个来源 | 已验证 | [../exec-plans/completed/CHG-0004-usage-ledger-agents.md](../exec-plans/completed/CHG-0004-usage-ledger-agents.md) |
| 2026-11-22 | 侧边栏来源增多后的重叠与裁切 | 已验证 | [../exec-plans/completed/2026-11-22-sidebar-source-overflow.md](../exec-plans/completed/2026-11-22-sidebar-source-overflow.md) |
| 2026-11-22 | 真实体验反馈中的确定性缺陷修复 | 已验证 | [../exec-plans/completed/2026-11-22-real-ux-clarity-fixes.md](../exec-plans/completed/2026-11-22-real-ux-clarity-fixes.md) |
| 2026-11-22 | 真实体验反馈 P0 / P1 全量修复 | 已验证 | [../exec-plans/completed/2026-11-22-real-ux-p0-p1.md](../exec-plans/completed/2026-11-22-real-ux-p0-p1.md) |
| 2026-11-22 | 侧边栏与来源区界面收敛 | 已验证 | [../exec-plans/completed/2026-11-22-sidebar-simplification.md](../exec-plans/completed/2026-11-22-sidebar-simplification.md) |
| 2026-11-22 | 侧栏 Workspaces / 来源按近 7 天会话数排序 | 已验证 | [../exec-plans/completed/2026-11-22-sidebar-recent-activity-order.md](../exec-plans/completed/2026-11-22-sidebar-recent-activity-order.md) |
| 2026-11-22 | 删除 workspace 的「[临时目录]」后缀 | 已验证 | [../exec-plans/completed/2026-11-22-remove-temporary-workspace-suffix.md](../exec-plans/completed/2026-11-22-remove-temporary-workspace-suffix.md) |

## 6. 链接

- 来源定义（对齐基准）：usage-ledger `src/usage_ledger/collectors.py`、`docs/exec-plans/completed/CHG-0016..0019`
- Perch 扩展点：`ARCHITECTURE.md` §"扩展点：新增一个来源"
- 逐源设计：[docs/design-docs/source-designs/SOURCES.md](../design-docs/source-designs/SOURCES.md)
