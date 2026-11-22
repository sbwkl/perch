# 2026-11-22 删除 workspace 的「[临时目录]」后缀

| 项 | 内容 |
|---|---|
| 状态 | 开发中 |
| Owner | wayne + agent |
| 所属模块 / 入口 | `src/App.svelte`、产品规格与当前状态文档 |
| 关联 | [agent-sources.md](../../product-specs/agent-sources.md) R15–R18；[P0/P1 全量修复](../completed/2026-11-22-real-ux-p0-p1.md)；[近 7 天侧栏排序](../completed/2026-11-22-sidebar-recent-activity-order.md) |

## 1. 摘要

用户确认删除 workspace 名称后的 `[临时目录]` 可见后缀。该后缀由 P0/P1 修复中的路径启发式生成，会把路径中包含 `tmp` / `Temp` / `/private/tmp/` / `/var/folders/` 的 workspace 都标成临时目录；真实 macOS / agent 会话路径下误判较多，且不符合后续「界面保持简洁」要求。本变更只移除可见后缀与对应的特殊目录分组 / 排序优先级；近 7 天会话数倒排与折叠继续生效，低活跃项自然排到后面。

## 2. 问题与证据

- 用户反馈：很多 workspace 显示 `&#x20; [临时目录]`，询问来源并要求删除后缀。
- 代码证据：`src/App.svelte` 中 `special(item)` 用 `/(^|\/)(tmp|Temp|private\/tmp|var\/folders)(\/|$)/i.test(item.path)` 判定临时目录，并在 `workspaceItems` 的 `label` 后拼接 ` [${group}]`；排序又用 `special` 把这类项后置。
- 来源证据：最初 P1 反馈只要求「对临时目录和无项目路径单独归组，别与真实项目混排」；P0/P1 修复把「归组」具体化成可见后缀。后续用户要求界面简洁、按近 7 天会话数倒排并可折叠，不需要再给路径类型加可见标签。
- 不做的后果：侧栏出现大量误判标签和额外空格实体，干扰 workspace 识别；特殊项在排序中被强行后置，削弱「最近活跃优先」。

## 3. 目标与非目标

### 目标

- Workspace 按钮文字只保留名称；重名时保留既有的「 · 路径父目录」后缀，不追加 `[临时目录]` / `[未关联项目]` / 分组标记。
- 删除 `special()` 路径启发式、`group` / `special` 字段以及按特殊目录后置的排序条件。
- Workspaces 排序只按近 7 天会话数倒序 → 最近活动时间 → 名称稳定排序；来源排序不变。
- 当前产品规格、README、ARCHITECTURE、CHANGELOG 与变更索引同步；历史完成文档保留作为过程证据，不冒充当前行为。
- 保持只读、最小 IPC、命令边界、四轨分示、分页、搜索、报告、筛选联动与折叠行为不变。

### 非目标

- 不改 Rust 的 `NO_PROJECT_NAME = "未关联项目"`；没有项目路径的 workspace 仍可显示该名称，但不追加方括号分组标签。
- 不新增临时目录识别规则、真实数据测试、IPC 字段或 cache schema。
- 不恢复「待关注」导航、用量报告导航、workspace 搜索框、来源更新时间或只读采集脚注。
- 不改 usage-ledger 报告、continue 决策、任务分页 / 搜索分页 / 筛选 chip。

## 4. 行为 / AC delta

- R1：When Workspaces 列表渲染, the system shall 不显示 `[临时目录]`、`[未关联项目]` 或任何 workspace 分组后缀；When 名称重复, the system shall 仅显示可区分的路径父目录后缀。
- R2：When Workspaces 列表排序, the system shall 不把路径中含 `tmp` / `Temp` / `/private/tmp/` / `/var/folders/` 的项作为特殊目录后置；排序仅按近 7 天会话数、最近活动时间与名称。
- R3：When Workspaces 列表渲染, the system shall 继续按近 7 天会话数倒序、超过 8 项折叠、选中项保持可见；侧栏数字仍按当前视图 / 筛选联动。
- R4：When 用户或 agent 阅读当前事实源, the system shall 让文档不把 `[临时目录]` 后缀描述为当前行为；历史变更文档可保留历史证据。
- 边界：无项目路径、路径中含 `tmp`、重名 workspace、超过 8 项折叠、900×560 小窗口、报告 / 搜索 / 筛选组合均保持其它既有行为。

## 5. 方案

1. 删除 `src/App.svelte` 中 `special()`、`group`、`special` 字段、`[临时目录]` / `[未关联项目]` 拼接及排序中的 `Number(left.special) - Number(right.special)`。
2. `workspaceItems.label` 改为 `${item.name}${suffix}`，只保留重名路径父目录后缀。
3. 更新 `docs/product-specs/agent-sources.md`：当前事实增加「不显示临时目录 / 分组后缀」，R18 记录验收；变更索引登记本变更。
4. 更新 README / ARCHITECTURE / CHANGELOG 中当前行为措辞，避免把临时目录标记写成当前能力；历史 `2026-11-22-real-ux-p0-p1.md` 等完成文档不改写成当前事实。
5. 新建本变更文档，验证后移入 `completed/`；PR #9 描述同步。

## 6. 验证计划

- 自动化：`npm run check`、`npm run build`、`cargo fmt --all --check`、`cargo clippy -p perch-core --all-targets -- -D warnings`、`cargo test -p perch-core`、`cargo test -p perch --lib`；临时集成分支跑 `cargo test --workspace`。
- demo / CDP 合成数据验收：`?stress=520` 下 Workspaces 所有 `nav-label` 不含 `[临时目录]` / `[未关联项目]` / `&#x20;`；路径含 `tmp` 的合成 workspace 不因路径被后置，排序仍满足近 7 天会话数 / 活动时间非递增；折叠 / 展开、选中项可见、900×560 无横向溢出。
- 边界：默认 demo 数据、无近 7 天活动、无项目路径、重名 workspace、报告视图、搜索 / 筛选 / 分页组合；只使用合成数据。

## 7. 回滚

单分支 revert；纯前端展示 / 文档变更，无 IPC、schema 或源数据改动。恢复 `special()` 即恢复历史标签，但不建议作为当前行为。

## 8. 风险与未决边界

- 删除标签后，用户仍需靠名称、重名路径后缀与近 7 天排序区分 workspace；这是当前「简洁 + 最近活跃优先」的既定方向。
- 近 7 天排序键仍只用 `data-*` 暴露给验收，不显示可见文字；侧栏数字仍代表当前视图 / 筛选可点击任务数。
- P2（overlay、workspace 手动命名 / 隐藏等）仍不在本轮范围。

## 9. 变更记录

| 状态 | 操作人 | 证据 / 链接 |
|---|---|---|
| 提议 → 就绪 | wayne + agent | 2026-11-22 用户明确要求删除 `[临时目录]` 后缀；本变更文档先于代码 |
