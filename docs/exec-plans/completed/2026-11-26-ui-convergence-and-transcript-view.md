# 2026-11-26 界面收敛（三组选中 / 状态栏 / 提示语 / 图标）与会话对话视图

| 项 | 内容 |
| --- | --- |
| 状态 | 已验证 |
| 类型 | 变更文档 |
| Owner | wayne + agent |
| 所属模块 / 入口 | `src/App.svelte`、`src/app.css`、`src/lib/types.ts`、`src-tauri/src/lib.rs`、`crates/perch-core`、`demo/`、文档 |
| 关联产品规格 | [agent-sources.md](../../product-specs/agent-sources.md) R5 / R8 / R11 / R12 / R22 / R23；[session-transcript.md](../../product-specs/session-transcript.md) T1–T12 |
| 关联 ticket | [#24](https://github.com/sbwkl/perch/issues/24)（规格，`ready-for-agent`） |
| 关联设计 | [ADR：按需读取会话正文](../../design-docs/adr/session-transcript-on-demand-read.md)、[SOURCES.md](../../design-docs/source-designs/SOURCES.md)、[SEARCH.md](../../design-docs/source-designs/SEARCH.md)、[GLOSSARY.md](../../../GLOSSARY.md) |

## 1. 背景

用户对主窗口提出四项收敛诉求与一项新能力：

1. 侧边栏「任务 / Workspaces / 来源」三组的选中效果与语义要统一；
2. 需要在不离开 Perch 的情况下查看某个会话的完整对话；
3. 「后台更新 · CodeBuddy 扫描完成 · 1934 ms」整条状态栏（含「暂停实时」）多余；
4. 列表头的「用量可能已过期 / 匹配 N 个会话 / 扫描 N 个来源 · N ms」是噪音；
5. 侧栏品牌位的线描鸟与 app icon 不一致。

其中第 2 项与 Perch 既有「不读取消息正文」的只读口径冲突，需要显式改口径并留下 ADR；其余为纯前端收敛。设计与取舍已通过逐轮问答定稿，本文件记录目标行为、验收与实现路径。

## 2. 目标行为

### 2.1 三组选中

- 任务 / Workspaces / 来源三组各自**单选**（组内互斥），未选中的组表示「全部」。
- 任一操作后三组中被选中的组数 ∈ `[1, 3]`：只剩一组被选中时再次点击该项保持选中（no-op）；搜索弹层「清除作用域」若会导致三组全空，则任务组回落「最近」。
- 任务组「全部」= 最近 + 更早（不含归档），按最近活动倒序；归档仍只经「已归档」进入。Workspaces / 来源行计数按「全部未归档」口径联动。
- 报告视图不计入三组；进入时清空任务选中，返回时恢复原任务视图与筛选。
- 选中样式强化：实底 + 标签加粗 + 计数反白 + 左侧 3–4px 竖条（来源组取该来源品牌色，任务 / Workspaces 取中性强调色）；保留 `aria-pressed`；分组标题不变。

### 2.2 状态栏与头部提示

- 删除整条 `.scan-status`（状态文案、进度点、「暂停实时 / 恢复实时」）；前端移除 `scan-status` 监听、`activeScans`、`scanStatus`、`formatScanStatus`、`handleScanStatus`、`toggleLiveRefresh`。实时刷新常开；根命令 `set_live_refresh` 保留但前端不再调用。
- 列表可见的扫描反馈只剩刷新按钮（`扫描中` 文案 + spinner）；错误 banner 与「N 条数据源提示」折叠条保留。
- 副标题保留「N 个任务 · 快照 X」，删除 `lastScanSummary`（「扫描 N 个来源 · N ms」「手动扫描中」）。
- usage-strip 只保留 token / credits / est 三个数值 pill；删除「用量可能已过期 / 用量快照 X」与「匹配 N 个会话」。`UsageOverview.stale` / `as_of` / `matched_tasks` 字段保留在 payload，不再展示。

### 2.3 图标

- 侧栏品牌位、详情空态、`demo/social.html` 统一改用 `src-tauri/icons/icon.svg`（app icon 的矢量源）内联渲染；删除 `ICONS.bird` 与 `.brand-mark` 的渐变覆盖，圆角与底色由图标本身提供。

### 2.4 对话视图

- 任务行结构改为「行容器 + 行按钮 + 对话图标按钮」；图标点击 = 选中该任务 + 打开对话。Trae CN / CodeBuddy 不渲染图标，也不在详情面板解释缺失。
- 对话占满第 2、3 列（侧边栏保留可交互）；头部：来源图标、标题、状态徽标、「继续工作」、「重新读取」、「返回」。
- 内容：user / assistant 纯文本，按时间正序；纯工具轮折叠为「工具调用 N 次」；不渲染 tool 输入输出、思考过程、附件、系统注入包装。纯文本节点渲染，不使用 `{@html}`。
- 读取：打开时单次快照；最近 50 条，向上每次再加载 50 条；单条 > 8,000 字符截断并标注；单会话 > 200,000 字符只保留最近部分；首开滚到最新，加载更早保持阅读位置。
- 退出：返回 / `Esc` 回到原列表与详情；切换任务视图、workspace 或来源自动退出；刷新后任务不在当前列表时退出并清除选中。
- 失败与空态：面板内一行原因，复用头部「重新读取」，不弹提示、不自动退出。

## 3. 验收

### 正常路径

1. 默认打开：任务组「最近」高亮；点 workspace → 两组高亮且列表按 workspace 过滤；再点来源 → 三组高亮，计数三组联动。
2. 任务组已取消（全部）时选中 workspace：列表为「该 workspace 的全部未归档任务」，按最近活动倒序，归档任务不出现；「已归档」视图仍只显示归档。
3. 选中样式：三组任一选中项都有实底、加粗标签、反白计数、左侧竖条；来源组竖条颜色等于该来源品牌色；深浅色主题下文字对比度 ≥ 4.5:1。
4. 主窗口不再出现 `.scan-status`、「暂停实时 / 恢复实时」、扫描耗时文本、「用量可能已过期」「用量快照」「匹配 N 个会话」。
5. 选中某来源时列表头只显示 token / credits / est 三个数值 pill；无用量数据时不显示该条。
6. 品牌位、详情空态、demo 社交图显示同一个 app icon 图形（棕色渐变圆角方块 + 插画鸟），尺寸 30px / 24px 下清晰。
7. 点任务行对话图标：任务被选中，第 2、3 列换成对话，头部有来源图标、标题、状态、继续工作、重新读取、返回；首屏停在最新消息。
8. 9 个正文来源各打开一次对话：消息按时间正序、角色正确；纯工具轮显示「工具调用 N 次」；超 8,000 字符的单条显示截断标注。
9. 超过 50 条消息的会话：默认 50 条，点「加载更早」追加 50 条且阅读位置不跳；超过 200,000 字符的会话只保留最近部分且顺序正确。
10. 「重新读取」成功替换内容并保持视图；「继续工作」沿用既有通道与文案；返回 / `Esc` 回到原列表并保留该任务详情。
11. Trae CN / CodeBuddy 任务行没有对话图标，详情面板没有任何正文说明。

### 异常与边界

1. 会话只有工具轮：面板显示「工具调用 N 次」行，不出现空白文档或报错。
2. 会话为空 / 源文件缺失 / 读取失败：面板显示一行原因 + 可用的「重新读取」；不自动退出、不弹窗。
3. JSONL 含损坏行或非 UTF-8：跳过该行继续渲染其余消息，不整屏失败。
4. 源文件在读取过程中被外部删除：返回错误态，源文件字节不变，不产生写操作。
5. 只剩一组被选中时再次点击：保持选中，不出现三组全空。
6. 任务组为「全部」时在搜索弹层点「清除作用域」：任务组回落「最近」，三组至少一组高亮。
7. 报告视图打开 / 返回：报告不计入三组，返回后恢复原任务视图与筛选；对话视图在进入报告前已关闭。
8. 对话打开时切换视图 / workspace / 来源：对话自动关闭，列表按新筛选渲染。
9. 刷新后任务不在当前筛选列表：对话关闭，详情清空。
10. 归档任务、子会话、`is_automation` 任务：同样按来源能力显示 / 隐藏对话图标，打开后行为一致。
11. 900×560 最小窗口：侧栏可滚动、页脚可点、对话面板内部滚动且无横向溢出；对话头部按钮不重叠。
12. 只读断言：打开对话前后所有源文件字节一致；`cache.db` 无正文字段；进程无新增网络调用。
13. fixture 与截图只用合成数据。

## 4. 方案

### 4.1 前端（`src/App.svelte` / `src/app.css` / `src/lib/types.ts`）

- 状态拆分：`view: "recent" | "older" | "archived" | null`（`null` = 全部），报告视图改为独立 `reportView: boolean`；`taskReturnView` 同步可空。选择辅助函数统一维护「选中组数 ∈ [1,3]」不变量，`clearSearchScope` / `revealTask` / `returnToTasks` 走同一入口。
- 计数与匹配：`tasksMatching(view, workspace, source)` 支持 `view = null`（全部未归档）；侧栏三组计数沿用同一函数。
- 选中样式：`.nav-item.active` 改为实底 + 竖条 + 加粗；来源组用 `--source` 变量着色。
- 删除：`.scan-status` 标记 / CSS / 状态 / 监听 / `toggleLiveRefresh`；`lastScanSummary`；usage-strip 的 stale / matched pill 及 `usageSnapshotLabel`。
- 图标：将 `src-tauri/icons/icon.svg` 内容内联到 `App.svelte`（品牌位与详情空态复用同一 snippet），删除 `ICONS.bird`；`demo/social.html` 内联同款图形。
- 任务行：`.task-row` 容器 + 主按钮 + `.task-transcript` 图标按钮（`aria-label="查看对话"`，仅 `transcriptSupported(source)` 时渲染）。
- 对话面板：`transcriptTaskKey` + `transcriptState`（`idle | loading | ready | error`）；`.transcript-pane` 以 `grid-column: 2 / -1` 占满后两列；返回 / `Esc` / 筛选变化统一关闭；消息按纯文本渲染，`white-space: pre-wrap`。
- IPC：`invoke<TranscriptOutcome>("read_session_content", { taskKey })`；只提交任务 key。

### 4.2 核心（`crates/perch-core`）

- 新模块 `transcript.rs`：`read_transcript(task: &TaskSnapshot) -> TranscriptOutcome`，按来源分派 9 个读取器（SQLite：Qoder / OpenCode / Kilo / CodeArts；JSONL：Claude / Codex / Antigravity / MiniMax；JSON：Cline）。统一产出 `TranscriptMessage { role, text, truncated }`。
- 解析口径：只提取 user / assistant 可见文本；工具块只计数；跳过 sidechain / meta / 系统注入包装；单条 8,000 字符、单会话 200,000 字符截断（只保留最近部分）；坏行跳过。
- 大文件策略：先读尾部并按上限解码（按行对齐块向前收集，可见文本超单会话预算即停），预算未满且未到文件头时继续向前补齐，避免整文件驻留。
- `models.rs`：新增 `TranscriptOutcome` / `TranscriptMessage` / `TranscriptRole`；`SUPPORTED_TRANSCRIPT_SOURCES` 常量集合（9 个），`transcript_supported(source)`。
- `store.rs`：`task` 表新增可重建的 `source_path TEXT`（`PRAGMA user_version` 升 1 → 2，`ALTER TABLE ADD COLUMN`）；文件型来源在扫描时写入会话文件路径，SQLite 来源留空并在读取时用配置的 DB 路径 + `source_id` 定位。正文不落库。
- 读取一律 `SQLITE_OPEN_READ_ONLY` + `PRAGMA query_only=ON`、JSONL 只读打开；打开失败返回错误字符串，不改来源状态。

### 4.3 Tauri（`src-tauri/src/lib.rs`）

- 新窄命令 `read_session_content(task_key: String)`：任务 key 字符集校验与 `continue_task` 同规；只用 `store.load_task` 取快照，路径由 Rust 侧解析；前端不传路径 / URL。注册进 `invoke_handler`。
- 不新增插件，`capabilities/default.json` 保持只有 `core:default`。

### 4.4 Demo

- `demo/fixture.ts` 增加合成对话（含 tool-only 轮、超长消息、>50 条），`demo/stub-core.ts` 增加 `read_session_content` stub，`demo/stub-event.ts` 移除 `scan-status` 事件（或保留但前端不再消费）。

## 5. 非目标

- 不实现 P1 内容搜索、正文索引、正文导出或正文落盘；`search_tasks` 数据通道不变。
- 不做全局开关、首次披露弹窗、常驻「只读」提示（用户明确不要）。
- 不删除 `set_live_refresh` 根命令与 probe 的暂停分支；不改变实时刷新节流 / 去抖 / 冷却。
- 不引入 overlay.db、置顶 / 标签 / 备注等覆盖层能力。
- 不引入远程页面 / 字体 / 图片 / 脚本；不启用 `fs` / `shell` / `http` 插件；`{@html}` 仍只用于固定来源图标。
- 不改来源扫描口径、排序、分页与搜索行为。

## 6. 回滚

单分支 revert。前端收敛无持久化副作用；`cache.db` 的 `source_path` 列可丢弃——删库重建即恢复（`user_version` 迁移需可重复执行）。正文读取路径是新增能力，回滚后回到「不读取正文」，无需清理任何源数据。旧 ADR 的读取范围表述在回滚时一并恢复。

## 7. 文档同步清单（实现阶段完成）

> 以下同步已随票据 #31（文档收口）完成：产品规格与「当前事实与边界」、状态行、变更索引、
> ADR 状态（proposed → accepted）、根文档口径（按需读取、七窄命令、对话面板布局）均已写回。

- 产品规格：[agent-sources.md](../../product-specs/agent-sources.md)（R5 / R8 / R11 / R12 / R22 / R23、摘要与「当前事实与边界」、变更索引）、[session-transcript.md](../../product-specs/session-transcript.md)、[index.md](../../product-specs/index.md)。
- 设计：[ADR：按需读取会话正文](../../design-docs/adr/session-transcript-on-demand-read.md)、[claude-session-title-read-scope.md](../../design-docs/adr/claude-session-title-read-scope.md)（读取范围表述）、[design-docs/index.md](../../design-docs/index.md)、[SEARCH.md](../../design-docs/source-designs/SEARCH.md)（隐私表述）、[SOURCES.md](../../design-docs/source-designs/SOURCES.md)（逐源正文位置）。
- 根文档：`ARCHITECTURE.md`（只读不变量、命令清单与数量、布局与视图）、`SECURITY.md`（读取范围、命令清单）、`README.md`（覆盖能力、详情内容、不包含列表）、`FAQ.md`（「会读取聊天正文吗」）、`ROADMAP.md`（「当前默认不做」去掉读取正文）、`CHANGELOG.md`、`GLOSSARY.md`。

## 8. 验证计划

- 自动化：`cargo fmt --all --check`、`cargo clippy -p perch-core --all-targets -- -D warnings`、`cargo test -p perch-core`、`cargo test -p perch --lib`、`npm run check`、`npm run build`。
- core 测试（临时目录 + 合成 fixture）：9 个来源各解析一次正常会话；tool-only 轮、超长截断、>50 条分页、坏行跳过、空会话、文件缺失、只读字节断言、`source_path` 迁移幂等。
- 命令测试：非法任务 key 拒绝；Trae CN / CodeBuddy 返回不支持；不存在任务返回错误；不产生任何写操作。
- demo / CDP 合成数据验收：三组选中与不变量、选中样式、状态栏与提示语缺席、图标一致、对话入口与布局、900×560 无溢出。

## 9. 变更记录

| 状态 | 操作人 | 证据 / 链接 |
| --- | --- | --- |
| 提议 → 就绪 | wayne + agent | 2026-11-26 逐轮问答定稿 Q1–Q20；本文件先于代码；产品规格、ADR 与词表已同步 |
| 规格发布 | agent | 2026-11-26 规格发布为 [#24](https://github.com/sbwkl/perch/issues/24)，标签 `ready-for-agent`；测试 seam 已与用户确认（core 读取入口 / 命令层 / demo+CDP / 缓存迁移） |
| 实现完成 | agent | 2026-11-26 票据 #25–#30 逐个以 merge commit 并入集成 `integration/ui-convergence-transcript`（tip 0896735）：#25 界面收敛（CDP S1–S5 共 39 断言）、#26 Claude 正文通道与 `source_path` 迁移、#27 对话面板（CDP T1–T3 共 17 断言 + S1–S5 回归）、#28 / #29 / #30 逐源读取器；门禁全绿：`cargo fmt --all --check` / `cargo clippy -p perch-core --all-targets -- -D warnings` / `cargo test -p perch-core`（165 用例，transcript 59）/ `cargo test -p perch --lib`（11 用例）/ `npm run check`（94 文件 0 错 0 警）/ `npm run build`。T11 只读断言为 core 测试源文件字节一致 + `cache.db` 无正文字段 |
| 已验证并归档 | agent | 2026-11-26 文档收口（票据 #31）：事实源（ARCHITECTURE / SECURITY / FAQ / core-beliefs / SOURCES / SEARCH / DATA-PROBE / README / ROADMAP / CHANGELOG / GLOSSARY）与产品规格改为按需读取口径；本文件移入 `completed/`。P1 内容搜索保持未实现口径 |