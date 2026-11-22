# 2026-11-22 删除第二栏筛选 chip

| 项 | 内容 |
|---|---|
| 状态 | 已验证 |
| Owner | wayne + agent |
| 所属模块 / 入口 | `src/App.svelte`、`src/app.css`、产品规格与当前状态文档 |
| 关联 | [agent-sources.md](../../product-specs/agent-sources.md) R15 / R20；[P0/P1 全量修复](../completed/2026-11-22-real-ux-p0-p1.md)；[侧边栏界面收敛](../completed/2026-11-22-sidebar-simplification.md) |

## 1. 摘要

用户选中 workspace / 来源后，不希望第二栏顶部出现 `Workspace：any2api × 来源：Codex ×` 这样的筛选 chip。本变更删除第二栏顶部筛选 chip 及对应 CSS，但保留侧栏选中态、筛选行为与计数联动；清除筛选仍可通过再次点击侧栏对应项完成。

## 2. 问题与证据

- 用户反馈：选中 workspace 和来源后，第二栏顶部不需要出现 `Workspace：any2api ×来源：Codex ×` tag 选中样式。
- 代码证据：`src/App.svelte` 中 `filterChips` 派生 `Workspace：…` / `来源：…` 列表，并在 `.list-head` 内渲染 `.filter-chips`；CSS 定义在 `src/app.css`。
- 不做的后果：第二栏顶部被筛选状态占用，和简洁侧栏方向冲突；用户可通过侧栏按钮高亮感知筛选，不需要重复 chip。

## 3. 目标与非目标

### 目标

- 删除 `.filter-chips` / `.filter-chip` 渲染与样式；选中 workspace / 来源后第二栏顶部不再出现筛选 tag。
- 保留 workspace / 来源筛选逻辑、侧栏按钮 `active` / `aria-pressed` 状态、列表计数联动、视图保留筛选、搜索作用域提示。
- 保留搜索弹层内部的“作用域 / 清除作用域”入口；本变更只删第二栏顶部 tag。
- 当前产品规格、README、ARCHITECTURE、CHANGELOG 与变更索引同步；历史变更文档保留过程证据。

### 非目标

- 不改筛选语义、排序、分页、搜索、报告、live 扫描控制、只读约束或命令边界。
- 不新增其它筛选状态入口；不恢复 workspace 搜索框、更新时间、说明文字或只读采集脚注。
- 不处理任务行内 `workspace · 来源 · 时间` 元信息分隔符；那不是筛选 chip。

## 4. 行为 / AC delta

- R1：When 用户选中 workspace / 来源, the system shall 在侧栏保持选中态并过滤任务，但第二栏顶部不显示 `Workspace：… ×` / `来源：… ×` 或任何筛选 chip。
- R2：When 用户再次点击已选中的侧栏 workspace / 来源, the system shall 清除该项筛选；该清除方式保持可用。
- R3：When 搜索弹层打开且当前存在 workspace / 来源筛选, the system shall 继续显示搜索作用域与清除入口；本变更不影响该行为。
- R4：When 用户或 agent 阅读当前事实源, the system shall 不再把第二栏筛选 chip 描述为当前行为；历史变更文档可保留历史证据。
- 边界：无筛选、单筛选、双筛选、报告视图、空列表、900×560 小窗口、搜索 / 分页 / 排序组合均保持其它既有行为。

## 5. 方案

1. 在 `src/App.svelte` 删除 `filterChips` 派生与 `.filter-chips` 渲染块；`clearWorkspaceFilter` / `clearSourceFilter` 若无其它引用则一并删除。
2. 在 `src/app.css` 删除 `.filter-chips` / `.filter-chip` / `.filter-chip:hover` 样式。
3. 更新 `docs/product-specs/agent-sources.md`：当前事实改为“不显示第二栏筛选 chip”，新增 R20；变更索引登记本变更。
4. 更新 README / ARCHITECTURE / CHANGELOG 当前行为措辞；历史完成文档不改写成当前事实。
5. 新建本变更文档，验证后移入 `completed/`；PR #9 描述同步。

## 6. 验证计划

- 自动化：`npm run check`、`npm run build`、`cargo fmt --all --check`、`cargo clippy -p perch-core --all-targets -- -D warnings`、`cargo test -p perch-core`、`cargo test -p perch --lib`；临时集成分支跑 `cargo test --workspace`。
- demo / CDP 合成数据验收：点击 workspace + 来源筛选后，第二栏 `.filter-chips` / `.filter-chip` 数量为 0，页面文本不含 `Workspace：`、`来源：` tag；侧栏对应两按钮仍 `aria-pressed=true`，列表数量与 smart list 联动；再次点击侧栏项可清除；搜索弹层作用域与清除入口仍可用；900×560 无横向溢出。
- 边界：无筛选、单筛选、双筛选、报告视图、空列表、任务分页 / 搜索分页；只使用合成数据。

## 7. 验证结果

- 自动化：`npm run check` 0 errors / 0 warnings；`npm run build` 成功；`cargo fmt --all --check` 通过；`cargo clippy -p perch-core --all-targets -- -D warnings` 零告警；`cargo test -p perch-core` 与 `cargo test -p perch --lib` 全部通过。
- demo / CDP 合成数据验收：初始与同时选中 workspace / 来源后，`.filter-chips` / `.filter-chip` 数量均为 0；第二栏文本不含 `Workspace：` / `来源：` tag；侧栏 workspace / 来源各有 1 个 `aria-pressed=true`，任务数缩小到 2、smart list 联动为最近 2 / 更早 0 / 已归档 0；搜索弹层仍显示自己的作用域说明；900×560 无横向溢出、页脚报告图标保留且无页脚文字。
- 边界：无筛选、单筛选、双筛选、报告视图、空列表、任务分页 / 搜索分页保持既有行为；未读取真实 SQLite / JSONL。

## 8. 回滚

单分支 revert；纯前端展示 / 文档变更，无 IPC、schema 或源数据改动。恢复 `filterChips` 派生与渲染块即恢复历史 chip。

## 9. 风险与未决边界

- 删除 chip 后，当前筛选仅在侧栏选中态体现；这是用户明确要求。
- 搜索作用域提示仍会显示 `Workspace：…` / `来源：…`，因为那是搜索弹层内部说明，不是第二栏筛选 tag。
- overlay、手动命名 / 隐藏 workspace 仍属于 P2，不在本轮范围。

## 10. 变更记录

| 状态 | 操作人 | 证据 / 链接 |
|---|---|---|
| 提议 → 就绪 | wayne + agent | 2026-11-22 用户明确要求删除第二栏筛选 tag；本变更文档先于代码 |
| 开发中 → 已验证 | agent | 2026-11-22 本文件 §7：全量门禁与 demo / CDP 合成数据验收通过 |
