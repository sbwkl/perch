# 产品规格：Perch 会话来源覆盖

| 项 | 内容 |
|---|---|
| 原需求卡 | REQ-0002 |
| 文档类型 | 产品规格 |
| 当前版本 | v0.1.0 |
| 适用范围 | 当前来源覆盖行为与验收规格 |
| 关联 | [CHG-0004](../exec-plans/completed/CHG-0004-usage-ledger-agents.md)、[侧栏来源溢出修复](../exec-plans/completed/2026-11-22-sidebar-source-overflow.md)、[真实体验确定性缺陷修复](../exec-plans/completed/2026-11-22-real-ux-clarity-fixes.md)、[P0/P1 全量修复](../exec-plans/completed/2026-11-22-real-ux-p0-p1.md)、[侧边栏界面收敛](../exec-plans/completed/2026-11-22-sidebar-simplification.md)、[近 7 天侧栏排序](../exec-plans/completed/2026-11-22-sidebar-recent-activity-order.md)、[删除临时目录后缀](../exec-plans/completed/2026-11-22-remove-temporary-workspace-suffix.md)、[删除 workspace 名称后缀](../exec-plans/completed/2026-11-22-remove-workspace-name-suffix.md)、[删除筛选 chip](../exec-plans/completed/2026-11-22-remove-filter-chips.md)、[代码审核修复](../exec-plans/completed/2026-11-23-code-review-fixes.md)、[Qoder 会话深链](../exec-plans/completed/2026-11-23-qoder-session-deeplink.md)、[Claude 会话标题](../exec-plans/completed/2026-11-25-claude-session-titles.md)、[Codex 子会话](../exec-plans/active/2026-11-25-codex-subagent-sessions.md)、[三组选中收敛与会话对话视图](../exec-plans/completed/2026-11-26-ui-convergence-and-transcript-view.md) |

## 1. 摘要

Perch 当前覆盖 11 个本地 AI 编码 Agent 来源，与 usage-ledger 的来源集合一致。来源的会话元数据以只读方式进入统一任务列表；界面保持简洁：侧边栏三组（任务 / Workspaces / 来源）各自单选、未选中即「全部」，且任何时刻至少保留一组选中，任务分组只保留最近 / 更早 / 已归档 / 搜索，用量报告仅由页脚图标进入，来源行只显示必要状态与当前视图匹配数，不显示触发方式 / 最近扫描时间或说明文字；应用内不显示扫描状态栏、扫描耗时或用量新鲜度提示。提供正文的来源可在任务行打开会话对话正文视图（见 [session-transcript.md](session-transcript.md)）。历史“补齐四个来源”的变更过程见 CHG-0004，本文只陈述当前产品行为与验收。

## 2. 当前事实与边界

- 当前 `ALL_SOURCES` 共 11 个：qoder、claude、codex、opencode、kilo、trae-cn、cline、antigravity、minimax、codearts、codebuddy。
- 每个来源有采集器、默认路径与环境变量覆盖、probe、continue 决策、前端名称 / 颜色 / 图标，以及 fixture 测试。
- 用量报告由 usage-ledger 的 `usage html` 生成并在 Perch 应用内 iframe 展示；报告仅由侧边栏页脚图标进入，任务分组不显示报告导航项。
- 任务导航为「最近 / 更早 / 已归档 / 搜索」；任务组可取消选中，表示「全部」= 全部未归档任务（最近 + 更早，按最近活动倒序），归档仍只经「已归档」进入；workspace / 来源筛选叠加在任务视图或全部态上。报告视图由页脚图标触发、不计入三组选中。侧边栏不显示 workspace 搜索框、来源触发方式 / 更新时间、来源说明和只读采集脚注。应用内不显示扫描状态栏、扫描耗时、「用量可能已过期 / 用量快照 X」或「匹配 N 个会话」；实时刷新没有暂停入口；选中来源时列表头只保留 token / credits / est 三个数值 pill，副标题只显示「N 个任务 · 快照 X」。
- 来源侧栏数字、smart list 数字与 workspace 数字均按当前视图和其它筛选联动；任务组未选中时按全部未归档口径计数。第二栏不显示筛选 chip，侧栏选中态表达当前筛选，列表副标题展示当前视图数量。Workspaces / 来源列表按全局近 7 天会话数倒序，排序键不显示为可见文字；workspace 名称只显示源数据名称，不追加路径父目录、临时目录、未关联项目或分组后缀。

## 3. 成功指标

- 主指标：`ALL_SOURCES` 与 usage-ledger `SOURCES` 一致（当前 11 个）；文档中当前状态描述与代码一致。
- 护栏指标：不写源数据；单源失败不影响其他来源；失败保留旧快照；`continue_task` 仍只接受任务 key；四轨用量分轨展示。
- 验证方式：临时目录 fixture 单元测试 + demo UI 验收 + 当前文档 / 代码核对。

## 4. 当前需求与验收（EARS）

- R1：When 某来源的本地数据目录存在, the system shall 扫描其会话并在任务列表展示，字段语义与既有来源一致（标题、项目、时间、状态）。Claude 标题按 JSONL `custom-title` → `custom-title.json` sidecar → JSONL `ai-title` → 首条候选用户消息派生标题 → `新会话` 重新计算；派生标题为单行并截断到 60 个字符，首条候选不可用或清洗后为空时回退 `新会话`。
- R2：If 某来源的数据目录不存在或采集失败, then the system shall 标记来源状态并保留旧快照，不影响其他来源与其他功能；状态需在侧栏可读暴露，不只依赖颜色。
- R3：When 用户对某来源的会话触发“继续工作”, the system shall 按该 Agent 的固定方式恢复会话；Qoder 会话在合法 session id 下使用 `qoder://chat/open?sessionId=<id>`、Codex 桌面 / 自动化会话在 UUID 形态的 session id 下使用 `codex://threads/<id>` 精确定位，其余来源（含 session id 不合法的 Qoder / 非 UUID 的 Codex）只激活对应 App，不启动终端或 CLI 恢复；详情面板只呈现一个继续工作按钮，不另设通道标签或说明段：按钮文案为「动词 + 来源名称」，能定位到具体会话的来源用 `打开 X`，只激活 App 的来源用 `激活 X`，动词必须与 Rust `plan_continue` 的决策分支一致（App 通道不得承诺定位具体会话）；若某来源的继续工作通道未知（`continue_hint` 缺失），按钮仍在，但动词取不承诺定位也不承诺激活的 `继续 X`，点击后失败原因由错误通道返回；按钮以该来源的品牌色填充，前景色按品牌色对比度自动选择，保证在浅色与深色面板上都可读；点击成功后不显示提示，失败显示危险色错误文本；URL 不得来自前端。
- R4：When usage-ledger 报告包含某来源的用量, the system shall 将该来源计入 `PERCH_SOURCES` 用量子集。
- R5：While 窗口可见, the system shall 每 3 秒检查 SQLite `data_version`，按不超过 15 秒的节流周期计算 JSONL / 日志目录签名；When 变化源经去抖 / 冷却 / 最长等待判定需要更新, the system shall 只重扫该来源；手动刷新期间后台 live scan 不争用 engine 锁。
- R11：While 应用运行, the system shall 保持实时刷新常开，不提供暂停 / 恢复入口，也不显示扫描状态栏；根命令 `set_live_refresh` 保留为后端能力但前端不调用；来源行不显示触发方式或更新时间。
- R12：When 扫描开始 / 结束, the system shall 只通过刷新按钮的「扫描中」文案与 spinner 表达手动扫描进度，不在列表头或状态栏显示来源、触发方式或耗时。When 没有到期 live scan 且 usage 未变化, the system shall 不发射 `sources-updated` 或触发全量 `bootstrap`；When 完成 live scan 或检测到 usage 变化, the system shall 发射 `sources-updated`。When 扫描产生来源错误, the system shall 保留错误 banner 与「N 条数据源提示」折叠条。
- R13：When 任意任务视图超过 60 条, the system shall 分页渲染并提供“显示更多”；「待关注」不作为用户可见导航项，`Counts.attention` 仅保留在 payload 供后续诊断。
- R14：When 搜索有 N 个命中, the system shall 返回全部 N 条，并在弹层按 50 条分页展开；When 打开命中结果, the system shall 自动展开到该任务所在页并定位。
- R15：When 视图 / workspace / 来源筛选组合变化, the system shall 让侧栏计数联动当前口径；第二栏不显示可清除筛选 chip；When workspace 名称重复或来自临时 / 无项目路径, the system shall 用文字区分并归组；Workspaces 分组不显示搜索框。
- R16：While 侧栏 Workspaces / 来源列表渲染, the system shall 按近 7 天非归档、非缺失、主会话数倒序；同数时按最近活动时间倒序，最后按名称 / 特殊目录稳定排序；排序不受当前视图 / 筛选影响，且不显示排序计数文字。
- R17：When Workspaces 或来源列表超过 8 项, the system shall 折叠排序后的后续项并提供「显示更多 / 收起」；When 当前选中项排在折叠区, the system shall 仍显示该选中项。
- R18：When Workspaces 列表渲染, the system shall 不显示 `[临时目录]`、`[未关联项目]` 或任何 workspace 分组后缀；When 排序时, the system shall 不按路径类型把 workspace 后置。
- R19：When Workspaces 列表渲染, the system shall 对每个 workspace 只显示源数据名称，不追加路径父目录或点号后缀；When 名称重复, the system shall 保持两个同名按钮，不做可见消歧。
- R20：When workspace / 来源筛选生效, the system shall 通过侧栏选中态表达筛选，不在第二栏顶部显示 `Workspace：… ×` / `来源：… ×` 或任何筛选 chip；搜索弹层内部作用域提示不受影响。
- R6：When 来源数量增加使侧栏导航内容超过可用高度, the system shall 在导航内容区内部滚动并保持侧栏页脚固定，不得压缩、覆盖或裁切来源项。
- R7：When 窗口缩小到最小尺寸 `900×560`, the system shall 允许用户通过滚动与键盘到达全部来源、来源说明和固定页脚。
- R8：When 用户切换任务视图、workspace 或来源筛选, the system shall 不保留不属于当前列表的 Inspector 选中任务，并关闭已打开的对话正文视图；When 搜索结果打开对应任务, the system shall 仍定位并选中该任务；When 刷新后选中任务不在当前筛选列表, the system shall 清除选中并关闭对话正文视图。
- R9：When 用量报告打开, the system shall 标识当前报告视图为全局报告、提供返回任务路径，且不启动外部浏览器；任务分组不显示报告导航项。
- R10：While 当前 workspace / 来源筛选生效, the system shall 在搜索弹层明示搜索作用域；When 无筛选时, the system shall 明示忽略任务视图并搜索全部会话（含归档）。
- R21：When Codex 的子代理会话或侧聊会话被采集, the system shall 将其识别为产生它的主会话的子会话：默认不进入主列表，不计入来源 / workspace / 最近 / 更早计数，但仍保留在快照中、可被搜索，并在详情与搜索结果中标注「子会话」；子代理会话缺少正式标题时以代理昵称派生「子代理 · <昵称>」标题；用户自行 fork / 续接的普通 Codex 会话不受影响，仍按主任务展示与计数。
- R22：When 用户点击侧边栏的任务视图、workspace 或来源项, the system shall 在组内单选：点击其它项切换，点击组内已选中项则取消该组选中（该组回到「全部」）；When 取消后将导致三组全部未选中, the system shall 保持该组原选中项不变；When 任意操作完成, the system shall 保证三组中至少一组、至多三组有选中项。任务组未选中 = 全部未归档任务，按最近活动倒序；smart list / workspace / 来源计数按全部未归档口径联动；报告视图不计入三组，进入时清空任务选中，返回时恢复原任务视图与筛选。选中项样式须为柔色底、加粗标签、强调色文字与计数，对比度 ≥ 4.5:1，并保留 `aria-pressed`。
- R23：When 列表头渲染, the system shall 不显示扫描状态栏、扫描耗时、「用量可能已过期 / 用量快照 X」或「匹配 N 个会话」；选中某来源时只显示 token / credits / est 三个数值 pill；副标题只显示「N 个任务 · 快照 X」。

验证位置：R1–R5 见来源采集器 / probe / continue 测试与当前代码核对；R6–R7 见侧栏布局实机验证；R8–R23 见对应变更文档的 core / Tauri 测试、demo UI 验收与 `npm run check`，其中 R22 / R23 见 [三组选中收敛与会话对话视图](../exec-plans/completed/2026-11-26-ui-convergence-and-transcript-view.md)；R3 / R12 的事件与提示一致性见 [代码审核修复](../exec-plans/completed/2026-11-23-code-review-fixes.md) 单测与实机验证。

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
| 2026-11-22 | 删除 workspace 名称的路径父目录后缀 | 已验证 | [../exec-plans/completed/2026-11-22-remove-workspace-name-suffix.md](../exec-plans/completed/2026-11-22-remove-workspace-name-suffix.md) |
| 2026-11-22 | 删除第二栏筛选 chip | 已验证 | [../exec-plans/completed/2026-11-22-remove-filter-chips.md](../exec-plans/completed/2026-11-22-remove-filter-chips.md) |
| 2026-11-23 | PR #9 代码审核修复 | 已验证 | [../exec-plans/completed/2026-11-23-code-review-fixes.md](../exec-plans/completed/2026-11-23-code-review-fixes.md) |
| 2026-11-23 | Qoder 会话深链与 App-only 继续工作 | 已验证 | [../exec-plans/completed/2026-11-23-qoder-session-deeplink.md](../exec-plans/completed/2026-11-23-qoder-session-deeplink.md) |
| 2026-11-25 | Claude 会话标题派生与人工标题 sidecar | 已验证 | [../exec-plans/completed/2026-11-25-claude-session-titles.md](../exec-plans/completed/2026-11-25-claude-session-titles.md) |
| 2026-11-25 | Codex 子代理 / 侧聊会话归为子会话 | 开发中 | [../exec-plans/active/2026-11-25-codex-subagent-sessions.md](../exec-plans/active/2026-11-25-codex-subagent-sessions.md) |
| 2026-11-24 | 继续工作入口收敛为单按钮（打开 / 激活）与来源品牌色 | 已验证 | [../exec-plans/completed/2026-11-24-continue-work-affordance.md](../exec-plans/completed/2026-11-24-continue-work-affordance.md) |
| 2026-11-26 | 三组选中收敛与回退柔色底、扫描状态栏 / 提示语删除、app icon 对齐与会话对话视图 | 已验证 | [../exec-plans/completed/2026-11-26-ui-convergence-and-transcript-view.md](../exec-plans/completed/2026-11-26-ui-convergence-and-transcript-view.md) |

## 6. 链接

- 来源定义（对齐基准）：usage-ledger `src/usage_ledger/collectors.py`、`docs/exec-plans/completed/CHG-0016..0019`
- Perch 扩展点：`ARCHITECTURE.md` §"扩展点：新增一个来源"
- 逐源设计：[docs/design-docs/source-designs/SOURCES.md](../design-docs/source-designs/SOURCES.md)
