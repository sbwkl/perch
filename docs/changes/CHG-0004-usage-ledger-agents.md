# CHG-0004 接入 Antigravity / MiniMax / CodeArts / CodeBuddy 四个来源

类型：CHG
状态：开发中
Owner：wayne
所属模块/入口：perch-core（collectors、models、paths、probe、continue_work、usage）、前端（App.svelte、source-icons.ts、demo）
功能卡：[REQ-0002](../requirements/REQ-0002-agent-sources.md)
关联：用户 2026-11-20 批准"单分支单 CHG、四个 Agent 一次接入、全部计入 PERCH_SOURCES"；分支 feat/usage-ledger-agents（基于 main b4c2ee3）

## 1. 摘要

把 Perch 的来源集合从 7 个补齐到与 usage-ledger 一致的 11 个：新增 Antigravity、MiniMax Code、CodeArts（码道）、CodeBuddy CN 四个会话来源，各自实现采集器并完成全部注册点（模型、路径、探测、continue、用量子集、UI、文档）。

## 2. 变更理由与证据

- 对应 REQ-0002 的问题：报告里有用量、任务列表里没有会话。
- 触发原因：用户 2026-11-20 要求把 usage-ledger 已新增的 Agent 同步到 Perch。
- 证据：usage-ledger main 已含全部 11 来源（CHG-0016 Antigravity / CHG-0017 MiniMax / CHG-0018 CodeArts / CHG-0019 CodeBuddy，均"已验证"，PR #10–#12 已合并）；数据路径与解析规则以 usage-ledger `collectors.py` 为准。
- 不做的后果：四个 Agent 的会话在 Perch 不可见，视图割裂。

## 3. 行为 / AC delta

- 新增：REQ-0002 R1–R5 对四个新来源生效。
- 预期行为变化：
  - `antigravity`：扫描 `~/.gemini/antigravity/`（IDE）与 `~/.gemini/antigravity-cli/`（agy CLI）的 `brain/<session>/.system_generated/logs/transcript_full.jsonl`，双目录按会话合并。
  - `minimax`：扫描 `~/.minimax/v2/sessions/YYYY/MM/DD/<session>/messages.jsonl`；项目名取自 `runtime-state.sqlite` 映射，取不到回退目录名。
  - `codearts`：读取 `~/.codeartsdoer/codearts-data/opencode.db`（legacy OpenCode schema，复用 OpenCode 采集逻辑，路径不同）。
  - `codebuddy`：解析 CodeBuddy CN 日志目录中的会话信息。
  - 四个来源均进入 `PERCH_SOURCES`；UI 增加名称/颜色/图标；probe 按数据形态选 SQLite/JSONL 探测。
- 修改：无既有来源行为。
- 删除：无。
- 边界：任一新来源不可用（目录/数据库缺失）时返回 unavailable，不影响其他来源（R2）。

## 4. 影响范围与非目标

- 受影响：`crates/perch-core`（models.rs、paths.rs、collectors/*、probe.rs、continue_work.rs、usage.rs）、`src/App.svelte`、`src/lib/source-icons.ts`、`demo/fixture.ts`、`docs/SOURCES.md`。
- 本次明确不做：token 用量统计复制进 Perch；来源插件化/自动发现；Windows/Linux 路径适配（保持与既有来源相同的 macOS 优先策略）。
- 兼容性：`ALL_SOURCES` 从 7 扩到 11，数组长度变化对内部调用方无影响；无 schema/存储迁移。

## 5. 验证与观测

- 自动化：新增 4 个采集器单元测试（临时目录 fixture）+ continue 计划测试 + antigravity/minimax/codebuddy 解析器单元测试。
- 手工（实机冒烟）：本机存在全部四个新来源的真实数据 → 任务列表应出现对应会话（待实机验证）。
- 结果与证据（2026-11-20，feat/usage-ledger-agents）：
  - `cargo test --workspace` 10 套件全 ok（83 通过，0 失败；新增 collectors 集成测试 24 项、continue 测试、probe/usage/store_query 既有用例保持通过）。
  - `cargo fmt --all --check` 零告警；`cargo clippy --all-targets` 零告警（修复 collapsible_if、useless_concat 后复验为 0）。
  - `npm run check` 0 errors 0 warnings；`npm run build` 成功（113 模块）。
  - fixture 形状对照真实数据校准：minimax `manifest.json`/`messages.jsonl`（含 reminder 包裹块）、antigravity `transcript_full.jsonl`（含游离片段）+ `conversation_summaries.db`、codebuddy 双日志格式（ISO 与 `[MM/DD/YYYY, h:MM:SS AM.mmm]` 毫秒小数）+ genie-history 目录名解码、codearts legacy `session` 表。
  - 修复的实现缺陷（测试先行暴露）：codebuddy trace→conversationId 映射需先于模型/指标行建立（改两遍扫描）；antigravity 流式解析重新同步需指向 `{` 本身；summaries 标题/项目覆盖转录推导值；IDE 日志小数部分按毫秒补齐微秒；minimax 项目映射存完整路径而非末段。
  - 待验证：实机冒烟（R1/R2/R4 各来源真实数据）、临时分支集成测试（R5 probe）。

## 6. 回滚方案

- 单分支 revert；无数据迁移，新增来源仅在检测到数据目录时激活。

## 7. 风险与未决问题

- CodeBuddy 日志格式由腾讯侧控制，可能随版本变化 → 采集器容错：解析失败按 unavailable/空结果处理，不 panic。
- MiniMax 项目映射依赖 runtime-state.sqlite，缺失时回退目录名，属预期降级。

## 8. 关联与证据

- 对齐基准：usage-ledger `src/usage_ledger/collectors.py`（scan_antigravity / scan_minimax / scan_sqlite_agent codearts 分支 / scan_codebuddy）及其 CHG-0016..0019
- commit：d7b6a0c（feat/usage-ledger-agents，2026-11-20）
- PR：https://github.com/sbwkl/perch/pull/6（待人工审核；合并 main 需用户明确同意）
- 集成测试：tmp/integration-usage-ledger-agents 基于 main b4c2ee3 合入 d7b6a0c（--no-ff，无冲突），合并树与已验证的特性分支树逐字节一致（`git diff` 为空）；main 自切点未移动，故集成树等价于已跑全量验证的树；测毕分支已删

## 9. 变更记录

| 状态 | 操作人 | 证据/链接 |
|---|---|---|
| 提议 → 就绪 | wayne + agent | 2026-11-20 会话，用户批准范围与流程（单分支单 CHG、四 Agent 一次接入、全部计入 PERCH_SOURCES） |
| 开发中 | agent | 本文档建立，分支 feat/usage-ledger-agents |
| 开发中（自动化验证完成） | agent | 2026-11-20 §5 证据：cargo test 83 通过 / clippy / fmt / npm check / npm build 全绿；§8：d7b6a0c、PR #6、临时分支集成合并无冲突 |
| 待办 | agent | 实机冒烟（R1/R2/R4 真实数据）待人工执行；未标"已验证"前不合并 main |