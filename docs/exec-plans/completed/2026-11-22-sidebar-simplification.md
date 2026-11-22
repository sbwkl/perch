# 2026-11-22 侧边栏与来源区界面收敛

| 项 | 内容 |
|---|---|
| 状态 | 已验证 |
| Owner | wayne + agent |
| 所属模块 / 入口 | `src/App.svelte`、`src/app.css`、产品规格与当前状态文档 |
| 关联 | [agent-sources.md](../../product-specs/agent-sources.md) R9 / R13 / R15；[usage-report.md](../../product-specs/usage-report.md) R1；[P0/P1 全量修复](../completed/2026-11-22-real-ux-p0-p1.md) |

## 1. 摘要

用户要求界面保持简洁：去掉「任务」下的「待关注」和「用量报告」、Workspaces 下的搜索框、来源行的手动 / 更新时间文字、来源区 Trae CN / ChatGPT 说明，以及页脚「只读采集 · 不改动源数据」描述。本变更只收敛可见 UI，不删除底层实时扫描控制、只读不变量、用量报告生成、待关注计数、分页 / 搜索 / 筛选联动等能力。

## 2. 问题与证据

- 2026-11-22 用户反馈：当前侧边栏新增的导航项、输入、来源新鲜度和说明文字过多，界面不够简洁。
- 指定要移除的文字 / 控件：`待关注`、`用量报告`（任务分组内）、workspace 搜索框、来源下的「手动」和更新时间、`Trae CN 需安装 usage-ledger hook 后显示会话；ChatGPT 桌面仅 Codex 会话可见。`、`只读采集 · 不改动源数据`。
- 不做的后果：常驻总览窗口被解释性文字和重复入口占据，任务导航层级变重。

## 3. 目标与非目标

### 目标

- 「任务」分组仅保留最近 / 更早 / 已归档 / 搜索；用量报告仍可由页脚图标进入。
- Workspaces 分组仅保留名称、当前视图联动计数与「显示更多 / 收起」，不显示搜索框或解释性副标题。
- 来源分组仅保留图标、名称、必要状态徽标与当前视图匹配数；不显示触发方式、最近扫描时间或 tooltip 中的更新时间；不显示 Trae CN / ChatGPT 说明。
- 页脚只保留用量报告图标 / 不可用图标，不显示只读描述。
- 当前产品规格、README、ARCHITECTURE、CHANGELOG 同步描述；历史变更文档保留证据。

### 非目标

- 不删除 `set_live_refresh` / `scan-status`、probe 节流 / 去抖 / 冷却、手动刷新优先等 P0 能力。
- 不删除 `counts.attention`、搜索全量命中 / 分页、任务分页、筛选联动计数、workspace 重名消歧等 P1 能力。
- 不删除 `open_usage_report`、应用内 iframe 报告及页脚入口；不改变只读硬约束、命令边界或数据路径。
- 不改动 Inspector 的任务详情 / 继续工作提示 / 只读脚注；不改动报告视图的返回按钮与 iframe。
- 不引入新依赖、远程资源或新 IPC 命令；不读取真实用户数据。

## 4. 行为 / AC delta

- R1：When 用户查看「任务」分组, the system shall 只显示最近 / 更早 / 已归档 / 搜索四个导航项，不显示「待关注」与「用量报告」。
- R2：When 用户点击页脚用量报告图标, the system shall 仍能进入应用内报告视图；报告视图保留全局说明与返回任务路径。
- R3：When 用户查看 Workspaces 分组, the system shall 不显示 workspace 搜索框和分组说明；When workspace 数量超过 8, the system shall 仍提供「显示更多 / 收起」。
- R4：When 用户查看来源分组, the system shall 不显示「手动」、实时 / 更新时间、较旧标记或更新时间 tooltip；When 来源非正常, the system shall 仍用状态文字 / 颜色提示。
- R5：When 用户查看侧栏页脚, the system shall 不显示「只读采集 · 不改动源数据」描述；只读约束仍由代码与文档保证。
- R6：When 用户查看来源分组底部, the system shall 不显示 Trae CN / ChatGPT 说明。
- 边界：报告视图、无 usage 数据、来源错误 / 未扫描、workspace 重名 / 临时目录、900×560 小窗口、搜索 / 筛选叠加均保持既有行为。

## 5. 方案

1. 前端移除任务导航中的「待关注」「用量报告」项；`ViewMode` 去掉 `attention`，保留 `recent / older / archived / report`；底层 `Counts.attention` 与 Rust 计数不变。
2. 移除 `workspaceQuery`、workspace 搜索派生与输入框；分组标题恢复为纯 `Workspaces`，保留重名后缀和特殊目录标记。
3. 来源项移除 `source-freshness` 文本、新鲜度辅助函数、`last_scan_at` / `last_trigger` tooltip 文案与 stale 类；保留 `SourceView.last_trigger` IPC 字段供 Rust / 后续诊断使用。
4. 移除 `.nav-note` 与页脚只读 `<span>`；页脚布局改为仅右对齐报告按钮，并清理无用 CSS。
5. 更新 `docs/product-specs/agent-sources.md`、`docs/product-specs/usage-report.md`、README、ARCHITECTURE、CHANGELOG；PR #9 描述同步。

## 6. 验证计划

- 自动化：`npm run check`、`npm run build`、`cargo fmt --all --check`、`cargo clippy -p perch-core --all-targets -- -D warnings`、`cargo test -p perch-core`、`cargo test -p perch --lib`。
- demo / CDP 合成数据验收：任务分组四个导航项；页脚报告图标仍能进入报告；无 workspace 搜索框；来源行不含「手动 / 实时 / 小时前 / 较旧」；无 Trae CN / ChatGPT 说明；无「只读采集」页脚文字；900×560 无横向溢出、页脚可点击。
- 边界：报告视图、无 usage 数据、来源错误 / 未扫描、workspace 超过 8、搜索 / 筛选叠加、空列表；只使用合成数据。

## 7. 验证结果

- 自动化：`npm run check` 0 errors / 0 warnings；`npm run build` 成功；`cargo fmt --all --check` 通过；`cargo clippy -p perch-core --all-targets -- -D warnings` 零告警；`cargo test -p perch-core`、`cargo test -p perch --lib` 全部通过；`tmp/integration-sidebar-simplification` 从最新 `origin/main` 合入无冲突、`cargo test --workspace` 与全量门禁通过并删除；PR https://github.com/sbwkl/perch/pull/9 的 Frontend / Rust core CI 均通过。
- demo / CDP 合成数据验收：任务分组仅「最近 / 更早 / 已归档 / 搜索」；无「待关注」「用量报告」导航项；无 workspace 搜索框，分组标题仅 `Workspaces`；来源行仅名称 / 状态 / 当前视图计数，title 不含触发方式或更新时间；无 `source-freshness`、来源说明及「只读采集」页脚文字；页脚报告图标仍能进入 iframe 报告视图并返回；900×560 无横向溢出、侧栏可滚动、页脚可点击。
- 边界：无 usage 数据时的页脚不可用图标、报告视图返回、workspace 超过 8、来源错误 / 未扫描、搜索 / 筛选叠加保持既有路径；未读取真实 SQLite / JSONL。

## 8. 回滚

单分支 revert；无 schema 迁移、无源数据写入。删除的控件 / 文案为纯前端展示，底层命令与数据字段不变。

## 9. 风险与未决边界

- 移除「待关注」导航后，`counts.attention` 与 `task_needs_attention` 仅保留在 core / payload，不作为用户可见入口；若后续需要恢复视图，应另开变更文档。
- 用量报告仍由页脚图标进入；若用户后续希望完全隐藏报告入口，应另开变更文档并同步产品规格。
- 来源行不再展示最近扫描时间；`scan-status` 事件与扫描状态条仍保留，用于 P0 可观察性。

## 10. 变更记录

| 状态 | 操作人 | 证据 / 链接 |
|---|---|---|
| 提议 → 就绪 | wayne + agent | 2026-11-22 用户明确要求移除指定 UI 文案 / 入口；本变更文档先于代码 |
| 开发中 → 已验证 | agent | 2026-11-22 本文件 §7：全量门禁与 demo / CDP 合成数据验收通过 |
| 已验证（集成 / PR） | agent | 临时集成分支从最新 main 全量验证通过并删除；PR #9 CI 两项检查通过 |
