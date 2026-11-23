# 2026-11-23 PR #9 代码审核修复

| 项 | 内容 |
|---|---|
| 状态 | 已验证 |
| Owner | wayne + agent |
| 所属模块 / 入口 | `src-tauri/src/lib.rs`、`crates/perch-core/src/continue_work.rs`、`docs/product-specs/agent-sources.md` |
| 关联 | [agent-sources.md](../../product-specs/agent-sources.md) R3 / R5 / R12；[P0/P1 全量修复](../completed/2026-11-22-real-ux-p0-p1.md)；PR #9 代码审核结果 |

## 1. 摘要

PR #9 代码审核发现两处需在合并前修复的问题：

1. 常驻探测线程在空闲轮次仍发射 `sources-updated`，前端每次重新 `bootstrap`，抵消 P0 常驻成本修复。
2. `describe_continue` 对 OpenCode / Kilo / Cline 的点击前提示与 `plan_continue` 实际分支不一致，且动态通道被错误描述为可定位会话。

本变更只修这两处审核问题，不改动其它 UI 收敛、排序、分页、搜索、只读边界或数据模型。

## 2. 问题与证据

### 2.1 空闲 `sources-updated` 回归

- `src-tauri/src/lib.rs` 后台循环条件为 `if !due.is_empty() || usage_changed || changed.is_empty()`。
- 当 `due.is_empty()`、`usage_changed == false`、`changed.is_empty() == true` 时，每 3 秒仍发射一次事件。
- `src/App.svelte` 的 `sources-updated` 监听器每次调用 `loadBootstrap()`，后者读取全量任务 / todo / sync state 并附加 usage 查询。
- 这与 `main` 的“无变化且 usage 未变化则 continue”语义相反，会在窗口可见但不操作时重新引入常驻 CPU / 电量成本。

### 2.2 `continue_hint` 与实际决策不一致

- `plan_continue` 对 OpenCode / Kilo 总是调用 `probed_plan`，忽略 `agent_kind`；`describe_continue` 却按 `agent_kind == "cli" / "desktop"` 推导终端 / App 提示。
- Cline `agent_kind` 为 `desktop` / `cli` 时 `plan_continue` 有明确分支，`unknown` 时走 `probed_plan`；当前提示与这一分支结构不完全对应。
- Kilo CLI 命令是裸 `kilo`，不能保证定位具体会话；动态通道可能被 App 激活或 CLI 探测到不同路径，不能统一标记 `locates_session = true`。

## 3. 目标与非目标

### 目标

- 仅在实际完成 live scan（`!due.is_empty()`）或检测到 usage 变化时发射 `sources-updated`；空闲 / 去抖 / 冷却 / 暂停轮次不触发全量 bootstrap。
- 为“是否发射事件”增加可单测的纯函数，并在 Tauri 测试中覆盖空闲、有扫描、有 usage 变化三类条件。
- `describe_continue` 与 `plan_continue` 保持同一分支语义：OpenCode / Kilo 永远动态；Cline desktop / cli 固定，unknown 动态；动态提示不承诺定位具体会话。
- 更新产品规格、ARCHITECTURE、变更索引；验证后把本变更文档移入 `completed/`。
- 保持命令边界、只读不变量、四轨分示、最小 IPC、所有 UI 收敛行为不变。

### 非目标

- 不改 probe 节流 / 去抖 / 冷却常量，不改 live scan 决策逻辑。
- 不改 `scan-status` 事件，它只在扫描开始 / 结束时发射，用于状态展示。
- 不改 `continue_task` 实际执行模板、session id 校验、命令边界和 Tauri 命令集合。
- 不恢复已移除的 UI 文案 / 控件；不处理 P2 项；不合并 `main`。

## 4. 行为 / AC delta

- R1：While 没有到期 live scan 且 usage 未变化, the system shall 不发射 `sources-updated`，前端不因空闲探测重新 `bootstrap`。
- R2：When 至少一个来源完成 live scan 或 usage 文件变化被检测到, the system shall 发射 `sources-updated`，前端重新读取快照并更新报告视图。
- R3：While live 刷新暂停或手动扫描进行中, the system shall 继续消费 / 更新探测基线但不触发 `sources-updated`。
- R4：When 用户选中 OpenCode / Kilo 任务, the system shall 显示“按运行状态决定”提示，不承诺终端或 App 固定通道，也不承诺定位具体会话。
- R5：When 用户选中 Cline `desktop` / `cli` 任务, the system shall 显示与 `plan_continue` 一致的固定通道；`unknown` 时显示动态提示且不承诺定位具体会话。
- R6：When 点击继续工作, the system shall 仍由 Rust 侧 `plan_continue` 决定实际动作；前端只展示只读提示，不传命令 / URL。
- 边界：无 usage 数据、手动刷新期间、暂停期间、报告视图、Kilo 裸 CLI、OpenCode App 激活、Cline unknown 均保持既有执行路径与错误提示。

## 5. 方案

1. 在 `src-tauri/src/lib.rs` 增加纯函数 `should_emit_sources_updated(due_count, usage_changed)`，后台循环仅在返回 true 时 emit；空闲轮次跳过。
2. 在 Tauri 单测中覆盖 `due_count == 0 && !usage_changed` 不发射、`due_count > 0` 或 `usage_changed` 发射。
3. 在 `crates/perch-core/src/continue_work.rs` 调整 `describe_continue`：
   - OpenCode / Kilo 始终动态；
   - Cline `desktop` → App，`cli` → Terminal，`unknown` → 动态；
   - 动态提示 `locates_session = false`，文案说明可能激活 App 或使用 CLI，激活 App 无法定位会话；
   - 保留 Codex 的 desktop / cli / unknown 决策与 Qoder / Claude / 其它 App-only 来源不变。
4. 更新 core 单元测试，断言上述分支与 `locates_session` 语义。
5. 同步 `docs/product-specs/agent-sources.md`、`ARCHITECTURE.md`、本变更文档证据。

## 6. 验证计划

- 自动化：`npm run check`、`npm run build`、`cargo fmt --all --check`、`cargo clippy -p perch-core --all-targets -- -D warnings`、`cargo test -p perch-core`、`cargo test -p perch --lib`；临时集成分支跑 `cargo test --workspace`。
- 单测：后台事件判定函数；continue hint 的 OpenCode / Kilo / Cline desktop / Cline cli / Cline unknown / Codex desktop / Qoder 分支。
- demo / CDP 合成数据验收：继续工作提示徽标与按钮文案仍可见；筛选 / 分页 / 报告 / 折叠行为未回归。
- 实机：`npm run tauri dev` 启动窗口，观察启动日志无错误；只做短时 UI 交互，不把真实会话标题 / 路径 / 用量写入仓库。
- 边界：无 usage 数据、手动刷新、暂停、报告视图、空列表、900×560 小窗口。

## 7. 验证结果

- 自动化：`npm run check` 0 errors / 0 warnings；`npm run build` 成功；`cargo fmt --all --check` 通过；`cargo clippy -p perch-core --all-targets -- -D warnings` 零告警；`cargo test -p perch-core` 全部通过；`cargo test -p perch --lib` 4 个测试通过，包括新增 `sources_updated_is_emitted_only_with_real_changes`。
- core 单测：`continue_hints_expose_action_before_click` 覆盖 OpenCode / Kilo 永远动态且 `locates_session=false`，Cline desktop / cli / unknown 分别对齐 App / Terminal / Dynamic，Codex desktop 与 Qoder 固定提示不变。
- 真机 `npm run tauri dev`：Vite 与 Rust 编译启动成功，无错误日志；首次扫描完成后，连续两次相隔 9 秒的 AX 头部快照（任务数 / 快照时间 / 扫描摘要）保持稳定，验证空闲轮次不再每 3 秒触发全量 bootstrap；点击任务检查继续提示，Codex 深链、Claude 终端保持固定通道，OpenCode 动态提示明确“激活 App 无法定位具体会话”。
- demo / CDP 合成数据：继续提示徽标与按钮文案仍可见，筛选 / 分页 / 报告 / 折叠行为未回归。
- 边界：无 usage 数据、手动刷新、暂停、报告视图、空列表、900×560 小窗口保持既有路径；未读取 / 写回真实任务正文或路径。

## 8. 回滚

单分支 revert；无 schema / IPC 新增 / 源数据改动。恢复旧 emit 条件与旧 `describe_continue` 分支即可，但不建议作为当前行为。

## 9. 风险与未决边界

- 事件减少后，前端只在 live scan 或 usage 变化时刷新；手动刷新路径仍由 `refresh_sources` 返回后显式 `loadBootstrap()`，不依赖事件。
- 动态提示不承诺定位会话，可能让用户更谨慎点击；实际执行仍按 Rust 侧进程探测决定，命令边界不变。
- 本轮不做长时间 CPU / 电量采样；代码路径与单测覆盖空闲事件消除，实机只做短时启动验证。

## 10. 变更记录

| 状态 | 操作人 | 证据 / 链接 |
|---|---|---|
| 提议 → 就绪 | wayne + agent | 2026-11-23 PR #9 代码审核结果；本变更文档先于代码 |
| 开发中 → 已验证 | agent | 2026-11-23 本文件 §7：全量门禁、单测、demo / 真机短验通过 |
