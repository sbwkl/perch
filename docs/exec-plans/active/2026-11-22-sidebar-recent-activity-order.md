# 2026-11-22 侧栏 Workspaces / 来源按近 7 天会话数排序

| 项 | 内容 |
|---|---|
| 状态 | 开发中 |
| Owner | wayne + agent |
| 所属模块 / 入口 | `src/App.svelte`、`demo/fixture.ts`、产品规格与当前状态文档 |
| 关联 | [agent-sources.md](../../product-specs/agent-sources.md) R15–R17；[侧边栏界面收敛](../completed/2026-11-22-sidebar-simplification.md)；[P0/P1 全量修复](../completed/2026-11-22-real-ux-p0-p1.md) |

## 1. 摘要

用户要求 Workspaces 与来源侧栏列表都按「最近 7 天的会话数」倒序排列，让最近活跃的 workspace / 来源排在前面，后续低活跃项可以折叠。显示数字仍保持当前视图 / 筛选联动口径；排序使用全局近 7 天活动口径，不作为额外可见文字，保持界面简洁。

## 2. 问题与证据

- 2026-11-22 用户反馈：侧栏 workspace / 来源列表需要按近 7 天会话数倒排，目的是最近活跃项优先，后面的项可能被折叠。
- 当前 `workspaceItems` 先按「特殊目录后置 + 名称」排序，`sourceItems` 沿用 `ALL_SOURCES` 固定顺序；两者都不区分近 7 天活跃度。
- 当前 Workspaces 超过 8 项会折叠，来源 11 项全部展开；折叠时没有保证低活跃来源排到后面。
- 不做的后果：用户常看到的顶部项可能是长期不活跃 workspace / 来源，折叠后反而把近期活跃入口藏起来。

## 3. 目标与非目标

### 目标

- 定义「近 7 天会话数」：以 `bootstrap.generated_at` 为基准，统计近 7 天内 `updated_at`（缺失时 `created_at`）有活动的主任务；排除源归档、源已删除、子会话与自动化任务。
- Workspaces 列表按近 7 天会话数倒序；同数按最近活动时间倒序；再按特殊目录 / 名称稳定排序。
- 来源列表按近 7 天会话数倒序；同数按最近活动时间倒序；再按名称稳定排序。
- 来源列表超过 8 项时提供「显示更多 / 收起」，与 Workspaces 一致；当前选中的 workspace / 来源即使排在折叠区也保持可见。
- 侧栏显示的数字仍为当前视图 / 其它筛选下的可点击任务数，不新增可见的「近 7 天 N 个」文字；排序键只通过 `data-*` 暴露用于验收。
- 当前视图 / 筛选组合、分页、搜索、报告、只读、命令边界不变。

### 非目标

- 不改变任务列表排序、smart list 计数、搜索排序或 Rust `Counts`。
- 不新增 IPC 命令、字段、缓存 schema、源数据读取或真实数据测试；不读取消息正文。
- 不恢复「待关注」导航、用量报告导航、workspace 搜索框、来源更新时间或只读采集脚注。
- 不改 usage-ledger 报告、continue 决策、四轨用量展示。

## 4. 行为 / AC delta

- R1：While 侧栏 Workspaces / 来源列表渲染, the system shall 按近 7 天会话数倒序排列；同数时按最近活动时间倒序，最后按名称 / 特殊目录稳定排序。
- R2：While 近 7 天会话数排序键计算, the system shall 使用全局任务快照，不受当前任务视图、workspace 筛选或来源筛选影响；显示数字仍按当前视图 / 筛选联动。
- R3：When Workspaces 或来源列表超过 8 项, the system shall 折叠排序后的后续项并提供「显示更多 / 收起」；When 当前选中项排在折叠区, the system shall 仍显示该选中项。
- R4：When 用户切换视图 / 筛选, the system shall 保持简洁侧栏：不新增近 7 天计数文字、更新时间、触发方式或说明文字。
- 边界：无近 7 天活动时按最近活动时间倒序；来源 / workspace 为 0 个当前视图任务时保持既有显示规则；900×560 小窗口、页脚报告图标、报告视图返回、搜索 / 分页不变。

## 5. 方案

1. 在 `src/App.svelte` 增加只读 `activityIndex`：从 `allPrimaryTasks` 聚合 workspace / source 的 `recentCount` 与 `lastActivity`；基准时间用 `payload.generated_at`，窗口 7 天。
2. `workspaceItems` 与 `sourceItems` 带上排序键，按 `recentCount desc → lastActivity desc → special / label` 排序；保留现有可见数字字段 `count`。
3. 来源侧栏复用 `nav-more` 样式与 8 项阈值，新增 `showAllSources` / `visibleSources` / `hiddenSourceCount`；workspace 仍用既有折叠逻辑。
4. nav 按钮增加 `data-recent-count` / `data-last-activity` 供合成数据 CDP 验收，不展示为可见文案。
5. demo fixture 在 `?stress=N` 下增加合成 workspace 与近 7 天会话，验证 >8 项折叠与排序；默认截图数据保持不变。
6. 同步产品规格、README、ARCHITECTURE、CHANGELOG；PR #9 描述补充排序与折叠。

## 6. 验证计划

- 自动化：`npm run check`、`npm run build`、`cargo fmt --all --check`、`cargo clippy -p perch-core --all-targets -- -D warnings`、`cargo test -p perch-core`、`cargo test -p perch --lib`；临时集成分支 `cargo test --workspace`。
- demo / CDP 合成数据验收：使用 `?stress=520`，断言 Workspaces / 来源 `data-recent-count` 非递增、同数时 `data-last-activity` 非递增；两组列表都有「显示更多」且展开后全部可见；折叠区选中项仍可见；900×560 无横向溢出、页脚报告图标可点击。
- 边界：默认无 stress 数据、无近 7 天活动、来源 / workspace 当前视图计数为 0、报告视图、搜索 / 筛选组合；只使用合成数据。

## 7. 回滚

单分支 revert；无 IPC / schema / 源数据改动。排序键为前端派生值，删除后侧栏恢复声明顺序；`data-*` 属性对 UI 无影响。

## 8. 风险与未决边界

- 排序键不显示为可见数字：侧栏数字仍代表当前视图 / 筛选可点击任务数，可能与排序键不同；用 `title` / `aria-label` 不新增近 7 天描述，保持界面简洁。
- 近 7 天窗口以 `bootstrap.generated_at` 为基准，避免长时间打开窗口后前端时钟漂移；bootstrap 更新时排序同步更新。
- 源归档 / 源已删除 / 子会话 / 自动化任务不计入活动排序，避免噪声项挤占折叠区。

## 9. 变更记录

| 状态 | 操作人 | 证据 / 链接 |
|---|---|---|
| 提议 → 就绪 | wayne + agent | 2026-11-22 用户明确要求按近 7 天会话数倒排；本变更文档与产品规格先于代码 |
