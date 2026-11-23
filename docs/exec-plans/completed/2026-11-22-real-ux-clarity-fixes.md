# 2026-11-22 真实体验反馈中的确定性缺陷修复

| 项 | 内容 |
|---|---|
| 状态 | 已验证 |
| Owner | wayne + agent |
| 所属模块 / 入口 | `src/App.svelte`、`src/app.css`、`crates/perch-core/src/continue_work.rs`、`crates/perch-core/src/query.rs`、`src/lib/types.ts`、`demo/`、`docs/` |
| 关联 | [agent-sources.md](../../product-specs/agent-sources.md) R1–R10；[usage-report.md](../../product-specs/usage-report.md) R1–R7；2026-11-22 真实 Tauri 体验反馈（脱敏聚合结论） |

## 1. 摘要

真实数据下可明确复现的体验缺陷包括：切换视图后 Inspector 还显示视图外的任务；搜索提示声称“所有会话”但当前 workspace / 来源筛选仍在生效；侧栏导航与来源项没有可访问的状态语义；“打开 X”按钮不预告实际恢复通道；报告视图没有独立导航态与明确返回路径；来源健康 / 新鲜度只藏在 tooltip；文档仍描述 7 个来源、外浏览器报告与旧视图模型。本轮只修这些有确定正确值且不需要产品方向决策的问题，并把性能 / 计数口径 / 长列表 / 结果上限等取舍项留给用户审核。

## 2. 问题与证据

- 反馈来自真实 Tauri 应用：约 790 条任务快照、11 个来源、应用内 iframe 报告；证据仅使用聚合数字和界面行为，不包含真实标题、路径、session id、用量明细。
- 代码证据：`src/App.svelte` 切视图函数不清 `selectedKey`；`selected` 从全 payload 查找；搜索 `$effect` 传入当前 `sourceFilter` / `workspaceFilter` 但空状态仍写“搜索所有会话（含归档）”；nav / source 按钮只有 `class:active`；`continue_task` 点击后才反馈通道；报告入口仅依赖页脚图标。
- 文档证据：`README.md`、`ARCHITECTURE.md`、`docs/product-specs/agent-sources.md`、`DEVELOPMENT.md`、`AGENTS.md` 的部分段落仍写 7 个来源、外浏览器报告、`recent / all / archived`，与 `ALL_SOURCES`、`open_usage_report`、四视图不一致。
- 不做的后果：用户会在当前视图与详情不一致时误执行恢复操作；按文档错误判断支持范围；辅助功能无法确认当前视图和筛选；点击核心动作前无法预判会启动终端、打开深链还是仅激活 App。

## 3. 目标与非目标

### 目标

- 上下文：切换视图 / workspace / 来源筛选或打开报告时，只显示属于当前列表的选中任务；搜索命中定位流程保持可用。
- 可预期：搜索弹层明示当前作用域；“继续工作”按钮在点击前显示 Rust 固定决策模板推导的通道与是否可定位会话；报告视图有独立导航语义、返回路径与“全局报告”说明。
- 可访问：nav / workspace / 来源筛选按钮带 `aria-pressed` 或 `aria-current`；来源状态、最近扫描与触发方式以文字或可读 title 暴露，不依赖颜色。
- 文档：README / ARCHITECTURE / DEVELOPMENT / 产品事实源与工作区地图与当前 11 个来源、应用内嵌报告、四视图一致。
- 只读、最小 IPC、四轨分示、源 id 校验等硬约束不变。

### 非目标

- 不改变 `ALL_SOURCES`、采集器、probe 频率、缓存 schema 或 `continue_task` 的固定命令边界。
- 不改变搜索的 P0 10 条结果上限、workspace / 来源筛选对搜索的叠加方式；只把当前作用域说清楚。
- 不实现虚拟滚动 / 分页、attention 视图、overlay.db、workspace 重名消歧、用量报告视觉统一或主题持久化。
- 不把 sidebar 计数改为“点进去正好看到 N 条”；该口径涉及产品定义，留待审核。
- 不引入新依赖、远程资源、新 IPC 命令或真实用户数据测试。

## 4. 行为 / AC delta

- 新增 R1：When 用户切换任务视图, the system shall 清除不属于新视图的选中任务，避免 Inspector 显示当前列表外的任务并允许继续工作。
- 新增 R2：When workspace 或来源筛选使当前选中任务离开列表, the system shall 清除该选中任务；When 搜索结果打开对应任务, the system shall 仍定位并选中该任务。
- 新增 R3：When 搜索弹层打开且存在 workspace / 来源筛选, the system shall 明示“作用域：当前 workspace / 来源”并提供清除作用域入口；When 无筛选时, the system shall 明示搜索全部会话（含归档）。
- 新增 R4：When 选中一个任务, the system shall 在“继续工作”按钮前显示固定模板推导出的动作：终端恢复 / 深链打开 / 仅激活 App；If 动作无法定位会话, then the system shall 在点击前提示该限制。
- 新增 R5：When 报告视图打开, the system shall 用独立导航状态和 `aria-current` 标识当前位置、显示“全局报告 · 不随任务筛选变化”，并提供“返回任务”按钮；When 返回任务, the system shall 回到进入报告前的最近任务视图且不遗留视图外选中项。
- 新增 R6：When 来源状态为 partial / unavailable / error / unknown, the system shall 在来源项显示可读状态；When 来源有 `last_scan_at`, the system shall 在 title 中可读地暴露最近扫描时间与触发方式，而不是把状态只藏在颜色或 `!`。
- 新增 R7：When 用户或 agent 阅读当前文档, the system shall 让 README / ARCHITECTURE / DEVELOPMENT / 产品事实源 / 工作区地图与当前 11 个来源、应用内嵌报告、四视图一致；历史版本说明不得冒充当前状态。
- 边界：demo stub 与 fixture 必须继续只使用合成数据，并覆盖新增 IPC 字段；搜索筛选、刷新、只读脚注、用量四轨与 `⌘F` / `⌘R` / `Esc` 行为回归不变。

## 5. 方案

1. 前端用 `selected` 从当前 `flatTasks` 派生，配合视图 / 筛选切换清理；`revealTask` 先设置视图与 workspace，再设置选中，避免自清理把搜索定位吞掉。
2. 搜索弹层新增 `searchScopeLabel` 派生值；作用域 chip 只清除 `workspaceFilter` / `sourceFilter`，不关闭弹层、不重置查询。
3. 在 `perch-core::continue_work` 增加纯函数 `describe_continue(task)`，输出由已有 `TaskSnapshot` 字段推导的通道文案和会话定位能力；`bootstrap` 的 `TaskView` 带可选 `continue_hint`，前端只读展示。Tauri 的 `continue_task` 仍由 Rust 侧 `plan_continue` 决定并执行固定动作，前端不传命令或 URL。
4. 在页脚报告按钮外，报告视图头部提供“返回任务”按钮；报告导航项带 `aria-current`，并保留页脚入口供全局到达。
5. `SourceView` 增加 `last_trigger` 透传；来源按钮组合 status / last scan / trigger / error 生成 title 与状态文字，保持最小 IPC 命令不变。
6. 修正当前状态文档；保留 CHANGELOG 与历史设计稿中的历史事实，不篡改历史记录。

## 6. 验证计划

- 正常：demo / CDP 脚本验证切视图与筛选清选中、搜索打开结果仍选中、搜索作用域提示、继续动作提示、报告返回、导航 `aria-*`、来源状态可读。
- 异常：来源 partial / unavailable / unknown 时状态文字与 title 可见；usage 不可用时报告入口给出说明，且不启动浏览器；搜索失败与刷新失败仍显示错误 banner。
- 边界：workspace 恰好 8 / 超过 8、900×560 小窗口、报告视图、无选中任务、demo 合成数据；不读取真实用户 SQLite / JSONL。
- 自动化门禁：`npm run check`、`npm run build`、`cargo fmt --all --check`、`cargo clippy -p perch-core --all-targets -- -D warnings`、`cargo test -p perch-core`；新增 core 单测覆盖 `describe_continue` 与 `SourceView.last_trigger`；临时集成分支全量验证后删除。
- 手工：仅在 demo 合成数据下验证 UI；如启动真实 Tauri 窗口，只观察聚合状态，不把真实标题 / 路径写回仓库。

## 7. 验证结果

- 自动化门禁：`npm run check` 0 errors / 0 warnings；`npm run build` 成功；`cargo fmt --all --check` 通过；`cargo clippy -p perch-core --all-targets -- -D warnings` 零告警；`cargo test -p perch-core` 35 + 5 + 25 + 5 + 9 + 5 个测试全部通过；GitHub Actions PR #9 的 Frontend 与 Rust core 两项检查均通过。
- 新增 core 测试：`continue_hints_expose_action_before_click` 覆盖终端 / 深链 / 仅激活 App / 动态通道与未知来源；`bootstrap_exposes_continue_hint_and_source_trigger` 覆盖 `TaskView.continue_hint` 与 `SourceView.last_trigger`。
- demo / CDP 合成数据验收（1200×780 与 900×560）：选中任务后切到「已归档」Inspector 清空且继续按钮消失；报告视图显示全局说明 / iframe / 返回任务；返回后无视图外选中；来源筛选下的搜索显示作用域且命中结果可定位并选中；继续工作按钮前置显示通道与无法定位提示；小窗口无横向溢出、页脚可见、侧栏可滚动。任务导航中的 `aria-current` 后续按界面收敛要求移除。
- 边界 / 异常：demo 来源 title 暴露状态、全部快照任务数、最近扫描与触发方式；`usage` 不可用时的报告入口说明已由 UI fallback 覆盖；搜索与刷新错误 banner 保持原路径。未读取真实 SQLite / JSONL，未写回真实标题 / 路径 / 用量。

## 8. 回滚

单分支 revert；无 schema 迁移、无源数据写入。`TaskView` 新增字段为可选，旧前端可忽略；删除字段时同步回退前端与 demo fixture。

## 9. 风险与审核边界

- 已生成供人工审核的取舍清单：`/private/tmp/perch-real-ux-review-checklist.md`（临时文件，不入库、不含真实数据）。
- 以下反馈项需要产品决策，不在本轮实现：live probe 降频 / 暂停 / 手动优先；503 条「更早」分页 / 虚拟滚动 / attention 视图；搜索 10 条上限或全局结果视图；sidebar 计数联动口径；workspace 重名与临时目录分组；来源健康完整看板；报告主题统一；overlay.db 与覆盖层。
- “继续工作”提示只描述已有固定决策，不根据真实应用运行状态做二次探测，避免 UI 与点击瞬间实际通道不一致；动态按进程探测来源保留既有通道决策文案。

## 10. 变更记录

| 状态 | 操作人 | 证据 / 链接 |
|---|---|---|
| 提议 → 就绪 | wayne + agent | 2026-11-22 真实 Tauri 体验反馈（脱敏聚合结论）与本变更文档范围分层 |
| 就绪 → 开发中 | agent | 分支 `feat/real-ux-clarity-fixes`，基于最新 main；产品规格与变更文档先行 |
| 开发中 → 已验证 | agent | 2026-11-22 本文件 §7：全量门禁通过、demo / CDP 合成数据验收通过 |
| 已验证（集成 / PR） | agent | `tmp/integration-real-ux-clarity-fixes` 从 main 合入无冲突、全量门禁通过并删除；PR https://github.com/sbwkl/perch/pull/9 ，CI 两项检查通过 |
