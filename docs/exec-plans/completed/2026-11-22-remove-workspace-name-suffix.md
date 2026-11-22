# 2026-11-22 删除 workspace 名称的路径父目录后缀

| 项 | 内容 |
|---|---|
| 状态 | 已验证 |
| Owner | wayne + agent |
| 所属模块 / 入口 | `src/App.svelte`、产品规格与当前状态文档 |
| 关联 | [agent-sources.md](../../product-specs/agent-sources.md) R18–R19；[删除临时目录后缀](../completed/2026-11-22-remove-temporary-workspace-suffix.md)；[近 7 天侧栏排序](../completed/2026-11-22-sidebar-recent-activity-order.md) |

## 1. 摘要

用户反馈 workspace 名称中出现的 ` · 路径父目录` 拼接不需要。该后缀来自早期“同名 workspace 消歧”处理：同名 workspace 会追加 ` · <parent>`，所以两个 `docs-portal` 会显示成 `docs-portal · code` / `docs-portal · archive`。本变更删除该后缀，Workspace 按钮只显示源数据里的 workspace 名称；排序仍按近 7 天会话数与最近活动，界面继续简洁。

## 2. 问题与证据

- 用户反馈：workspace 有一些以 `.` 号拼接，不需要这种拼接。
- 代码证据：`src/App.svelte` 中 `nameCounts` 统计同名项，`parent = item.path.split("/").at(-2)`，同名时拼接 ` · ${parent}` 到 `label`。
- 来源证据：最初 P1 反馈提到两个 `playground` 按钮同名，建议做可区分副标题；但后续用户明确要求界面简洁，并先删除 `[临时目录]` 后缀，现在进一步确认不需要路径父目录后缀。
- 不做的后果：可见 workspace 与源元数据名称不一致；名称里混入路径段，视觉噪音大，尤其同名项目会出现点号拼接。

## 3. 目标与非目标

### 目标

- Workspace 按钮只显示 `group.name`，不追加 ` · <parent>`、`[临时目录]`、`[未关联项目]` 或任何后缀。
- 删除 `nameCounts`、`parent`、`suffix` 相关派生逻辑；标题、筛选 chip、搜索作用域仍使用同一 workspace 名称。
- 保持近 7 天会话数倒序、最近活动次级排序、名称稳定排序与超过 8 项折叠不变。
- 当前产品规格、README、ARCHITECTURE、CHANGELOG 与变更索引同步；历史完成文档保留过程证据。
- 保持只读、最小 IPC、命令边界、四轨分示、分页、搜索、报告、筛选联动不变。

### 非目标

- 不改 Rust 的 `NO_PROJECT_NAME = "未关联项目"`；无项目路径仍可显示为源数据名称，但不追加后缀。
- 不做新的 workspace 重命名 / 别名 / overlay 能力；不新增真实数据测试、IPC、cache schema 或源数据读取。
- 不恢复「待关注」导航、用量报告导航、workspace 搜索框、来源更新时间或只读采集脚注。
- 不改 usage-ledger 报告、continue 决策、任务分页 / 搜索分页 / 筛选 chip / live 扫描控制。

## 4. 行为 / AC delta

- R1：When Workspaces 列表渲染, the system shall 对每个 workspace 只显示 `group.name`，不显示路径父目录、点号后缀、括号分组或其它可见后缀。
- R2：When workspace 名称重复, the system shall 保持两个同名按钮，不做可见消歧；详情 toolbar / tooltip 仍可显示项目路径作为已有补充。
- R3：When Workspaces 列表排序, the system shall 继续按近 7 天会话数、最近活动、名称排序；删除路径父目录后缀不得改变排序键。
- R4：When 用户或 agent 阅读当前事实源, the system shall 不再把同名路径后缀描述为当前行为；历史变更文档可保留历史证据。
- 边界：无项目路径、同名 workspace、临时路径、超过 8 项折叠、900×560 小窗口、报告 / 搜索 / 筛选组合保持其它既有行为。

## 5. 方案

1. 在 `src/App.svelte` 删除 `nameCounts`、`parent`、`suffix` 派生；`workspaceItems.label` 直接取 `item.name`。
2. 排序仍使用 `compareActivity(left.activity, right.activity) || left.name.localeCompare(right.name)`，不使用 label 后缀。
3. 更新 `docs/product-specs/agent-sources.md`：当前事实改为“workspace 名称不追加任何后缀”，R19 记录验收；变更索引登记本变更。
4. 更新 README / ARCHITECTURE / CHANGELOG 当前行为措辞，去掉“重名路径后缀”为当前能力的描述；历史完成文档不改写成当前事实。
5. 新建本变更文档，验证后移入 `completed/`；PR #9 描述同步。

## 6. 验证计划

- 自动化：`npm run check`、`npm run build`、`cargo fmt --all --check`、`cargo clippy -p perch-core --all-targets -- -D warnings`、`cargo test -p perch-core`、`cargo test -p perch --lib`；临时集成分支跑 `cargo test --workspace`。
- demo / CDP 合成数据验收：`?stress=520` 下 Workspaces 所有 `nav-label` 不含 ` · `、`[临时目录]`、`[未关联项目]`；demo 中两个 `docs-portal` 均显示为 `docs-portal`；`data-recent-count` 仍非递增、同数按活动时间；折叠 / 展开、选中项可见、900×560 无横向溢出。
- 边界：默认 demo 数据、无项目路径、同名 workspace、近 7 天排序、报告 / 搜索 / 筛选 / 分页组合；只使用合成数据。

## 7. 验证结果

- 自动化：`npm run check` 0 errors / 0 warnings；`npm run build` 成功；`cargo fmt --all --check` 通过；`cargo clippy -p perch-core --all-targets -- -D warnings` 零告警；`cargo test -p perch-core` 与 `cargo test -p perch --lib` 全部通过；`tmp/integration-remove-workspace-name-suffix` 从最新 `origin/main` 合入无冲突、`cargo test --workspace` 与全量门禁通过并删除；PR https://github.com/sbwkl/perch/pull/9 的 Frontend / Rust core CI 均通过。
- demo / CDP 合成数据验收（`?stress=520`）：折叠与展开状态下所有 workspace 标签均不含 ` · `、`[临时目录]`、`[未关联项目]` 或 `&#x20;`；两个 `docs-portal` 均显示原名，`title` 仍保留各自完整路径；`data-recent-count` 非递增且同数按活动时间倒排；折叠 / 展开、900×560 无横向溢出与页脚报告图标均保持。
- 边界：默认 demo 数据、无项目路径、同名 workspace、近 7 天排序、报告 / 搜索 / 筛选 / 分页组合保持既有路径；未读取真实 SQLite / JSONL。

## 8. 回滚

单分支 revert；纯前端展示 / 文档变更，无 IPC、schema 或源数据改动。恢复 `nameCounts` / `suffix` 即恢复历史后缀，但不建议作为当前行为。

## 9. 风险与未决边界

- 删除后缀后，同名 workspace 会显示相同名称；用户需要靠排序位置、计数或 tooltip / 点击后的详情路径区分。这是当前「简洁 + 最近活跃优先」的既定方向。
- path tooltip 仍作为既有可达信息存在，本次不新增也不删除 tooltip。
- overlay、手动命名 / 隐藏 workspace 仍属于 P2，不在本轮范围。

## 10. 变更记录

| 状态 | 操作人 | 证据 / 链接 |
|---|---|---|
| 提议 → 就绪 | wayne + agent | 2026-11-22 用户明确不需要 workspace 名称的点号拼接；本变更文档先于代码 |
| 开发中 → 已验证 | agent | 2026-11-22 本文件 §7：全量门禁与 demo / CDP 合成数据验收通过 |
| 已验证（集成 / PR） | agent | 临时集成分支从最新 main 全量验证通过并删除；PR #9 CI 两项检查通过 |
