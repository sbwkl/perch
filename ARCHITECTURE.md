# Architecture

Perch 是一个 macOS 桌面应用：Tauri 2 壳 + 纯 Rust core + Svelte 5 前端。
目标是只读聚合多个 coding agent 的本地会话，因此架构的第一约束是**不写源数据**。

产品背景与方案对比见 [v0.3.3-design-review.md](docs/design-docs/history/v0.3.3-design-review.md)（历史稿），当前逐源规格见 [SOURCES.md](docs/design-docs/source-designs/SOURCES.md)；
本文描述当前代码的实际结构。

## 总览

```
┌──────────────────────────── Svelte 5 (WKWebView) ───────────────────────────┐
│  App.svelte：三栏界面 / 视图与筛选 / 搜索 / 对话面板 / Inspector             │
│  invoke("bootstrap" | "refresh_sources" | "search_tasks" |                    │
│         "continue_task" | "read_session_content" | "open_usage_report")       │
└───────────────────────────────────┬─────────────────────────────────────────┘
                                    │ Tauri IPC
┌───────────────────────────────────▼─────────────────────────────────────────┐
│  src-tauri/src/lib.rs                                                       │
│  · bootstrap / refresh_sources / search_tasks / continue_task /             │
│    read_session_content / open_usage_report / set_live_refresh              │
│  · AppState = Arc<Mutex<ScanEngine>>                                        │
│  · 3 秒 SourceProbe + live 去抖 / 冷却、scan-status 事件（前端不再消费）      │
└───────────────────────────────────┬─────────────────────────────────────────┘
                                    │
┌───────────────────────────────────▼─────────────────────────────────────────┐
│  crates/perch-core                                                          │
│  collectors ──▶ sync::ScanEngine ──▶ store::CacheStore ──▶ cache.db         │
│       │                                  │                                  │
│       └── SourceResult                   └── query::build_bootstrap         │
│  usage::UsageSnapshot ──（只读 usage.db）──▶ query::build_bootstrap_with_usage│
│  probe::SourceProbe（SQLite data_version / JSONL 签名）                     │
│  search::search_tasks（只读 cache.db 元数据；拼音/模糊/加权排序）           │
│  continue_work::plan_continue（App 会话深链 / 激活 App）                     │
└──────────────────────────────────────────────────────────────────────────────┘
```

依赖方向单向：`src-tauri → perch-core`，`perch-core` 内部
`collectors / probe → models`、`query / search → store + models`、`sync → collectors + store`。
`perch-core` 不依赖 Tauri，可以独立编译和测试。

## 模块职责

| 模块 | 位置 | 职责 |
| --- | --- | --- |
| 采集器 | `crates/perch-core/src/collectors/` | 每个来源一个模块，只读解析并返回 `SourceResult`；不接触 DB 写路径 |
| 扫描引擎 | `src/sync.rs` | 逐个执行采集器，捕获 panic，写快照，判定缺失，记录同步状态 |
| 快照存储 | `src/store.rs` | `cache.db` schema、upsert、缺失标记、sync_state / scan_run |
| 查询 | `src/query.rs` | 把快照组装为 `BootstrapPayload`（workspaces、sources、counts、可选 usage） |
| 搜索 | `src/search.rs` | 只读 `task` / `task_todo`，NFKC 归一 + 拼音 + 模糊匹配 + 字段加权，按 workspace / 来源筛选后返回全部命中与总数 |
| 用量桥接 | `src/usage.rs` | 只读打开 usage-ledger 的 `usage.db`，按 `session_id` 聚合 token / credits / est |
| 探测 | `src/probe.rs` | 轻量判断源是否变化：SQLite `PRAGMA data_version`、JSONL 文件数与最新 mtime；Claude 还单独监测 `custom-title.json` |
| 继续工作 | `src/continue_work.rs` | 按来源决定会话深链或激活 App，并校验 session id；不启动终端 |
| 对话正文 | `src/transcript.rs` | 按来源分派的按需读取器：单次快照解析可见文本，只读、内存内、不落盘 |
| 路径 | `src/paths.rs` | 默认源路径与环境变量覆盖 |
| Tauri 壳 | `src-tauri/src/lib.rs` | 七个 IPC 命令、状态管理、3 秒探测循环、事件 |
| 前端 | `src/App.svelte` | 三栏布局、任务筛选、元数据搜索、对话正文面板、详情与“继续工作”按钮 |
| 演示夹具 | `demo/` | 仅开发用：假数据 + Tauri API stub，用于截图与 UI 调试 |

## 数据流

### 启动与刷新

1. 前端 `onMount` 调用 `bootstrap`，读取 `cache.db` 中的上次快照，立即渲染。
2. 随后调用 `refresh_sources`：`ScanEngine::scan(None, "manual")` 依次执行 11 个采集器。
3. 每个采集器独立返回 `ok / partial / unavailable / error`；单个来源失败不影响其他来源。
4. `ok` 结果在事务中 upsert，并在扫描记录 `scan_run` 完成后把本次未出现的任务标记
   `source_missing`；`partial / error / unavailable` 保留旧快照。
5. 前端重新 `bootstrap`，更新列表。

防误删保护：若某来源上次有超过 20 个任务，而本次扫描结果不足一半，则降级为 `partial`
并保留旧快照，等待用户手动刷新确认。

### 实时探测

`src-tauri` 启动一个后台线程，每 3 秒执行一次：

- 窗口最小化或不可见时跳过；根命令 `set_live_refresh` 保留为后端能力，前端不提供暂停入口。
- SQLite 来源每轮检查 `PRAGMA data_version`；JSONL / 日志目录签名最多每 15 秒计算一次。Claude 的 `custom-title.json` 也进入文件签名，但只匹配该 basename。
- 变化按源记录，6 秒静默去抖、30 秒冷却、60 秒最长等待；只重扫到期的来源（`trigger = "live"`）。
- 手动刷新期间后台不争用 `ScanEngine` 锁；扫描开始 / 结束仍向事件通道发射 `scan-status`（来源、触发方式与耗时），但前端不再消费展示——列表可见的扫描反馈只有刷新按钮的「扫描中」文案。
- 仅当完成 live scan 或检测到 usage 变化时 emit `sources-updated`；空闲 / 去抖 / 冷却轮次不触发前端全量 `bootstrap`；完成后推迟该 JSONL / 日志源的签名检查。

探测只做轻量读取，不解析内容；不会常驻 daemon，应用退出即停止。

### Claude 标题读取边界

Claude 标题优先级在每次扫描时完整重算：JSONL `custom-title` → sidecar `custom-title.json` → JSONL `ai-title` → 首条候选用户消息派生的 `derived` 标题 → `新会话`。派生只为生成标题读取首条候选用户文本，跳过 sidechain、`isMeta`、`toolUseResult`、系统注入与 slash command 包装；缓存只保存单行标题和既有任务字段，不保存正文。`agentName`、`summary` / compaction summary 与 `history.jsonl` 均不参与。

### 用量附加（可选）

1. `paths::usage_db_path()` 解析 `PERCH_USAGE_DB` → `USAGE_LEDGER_DB` →
   `USAGE_LEDGER_HOME` → `~/.local/share/usage-ledger/usage.db`。
2. `UsageSnapshot::try_load` 以只读连接读取 `usage_turn` / `usage_daily` /
   `usage_credit` / `usage_est`；库不存在、表缺失或 schema 不兼容时返回 `None`。
3. `query::build_bootstrap_with_usage` 按 `(source, session_id)` 把用量挂到
   `TaskView.usage`，并在 `BootstrapPayload.usage` 写入**按来源分组**的汇总：来源之间单位
   不同（token / credits），因此只做来源内的 7 天聚合，不做跨来源合计。
4. 三块用量分开展示：token 真值、credits（服务端计费）、est（估算）。activity 只有
   `source + day + project` 粒度，不挂到单个任务。
5. Tauri 的 3 秒探测同时比较 `usage.db` 主文件与 `-wal` 的 mtime；usage-ledger
   单独同步后也会触发前端重新 `bootstrap`，但 Perch 不会主动写该库。
6. 侧边栏页脚的用量报告图标走 usage-ledger 自己的 CLI：
   `usage html --output <Perch 缓存目录>`，完成后通过 asset 协议在应用内 iframe 展示。
   报告是全局的，不按来源过滤；Perch 只读该库，HTML 与缓存目录属于 Perch 自身。

### 继续工作

前端只传 `task_key`，Rust 侧从快照找到任务并调用 `plan_continue`：

| 来源 | 决策 |
| --- | --- |
| Qoder | `qoder://chat/open?sessionId=<id>`（`^[A-Za-z0-9_-]{1,128}$`）；不合法时 `open -b com.qoder.app` |
| Claude Code | `open -b com.anthropic.claudefordesktop` |
| Codex | `desktop/automation` 且 id 为 UUID → `codex://threads/<id>`；其他 `agent_kind` 或非 UUID id → `open -b com.openai.codex` |
| OpenCode | `open -b ai.opencode.desktop` |
| Kilo | `open -b ai.kilo.desktop` |
| Trae CN | `open -b cn.trae.app`（hook/memory 无项目路径，也没有确认的会话级深链） |
| Cline | `open -b bot.cline.app`（App 无 URL scheme） |
| Antigravity | `open -b com.google.antigravity`（无确认的会话级恢复方式） |
| MiniMax Code | `open -b com.minimax.agent.cn`（无确认的会话级恢复方式） |
| CodeArts | `open -b com.huawei.codearts.agent`（无确认的会话级恢复方式） |
| CodeBuddy | `open -b com.tencent.codebuddycn`（无确认的会话级恢复方式） |

`TaskView.continue_hint` 由 `continue_work::describe_continue` 按快照元数据只给出 `channel`（`deeplink` / `app`），
措辞由前端拥有：`deeplink` → `打开 X`，`app` → `激活 X`。实际动作仍由 `plan_continue` 与 Tauri 固定模板执行，
前端不传命令或 URL；`continue_task` 成功后不返回文案，失败通过 Tauri 错误通道回到详情面板。

session id 先经过字符集校验；URL 与 bundle id 都由固定模板生成，前端无法传入任意命令。
“继续工作”只使用 `open` 的 URL scheme 或 App 激活，不启动终端，也不执行 CLI 恢复命令。

### 对话正文读取（按需）

用户在任务行点击对话图标时，前端 `invoke("read_session_content", { taskKey })`，Rust 侧
`transcript::read_transcript(&TaskSnapshot)` 按 `task.source` 分派到 9 个读取器
（SQLite：Qoder / OpenCode / Kilo / CodeArts；JSONL：Claude / Codex / Antigravity / MiniMax；JSON：Cline）：

- **按需、单次快照**：只在用户打开时读取一次，不随实时刷新自动重读；重新解析只能由用户
  点击「重新读取」触发。不做全量预读、不做后台索引、不做正文搜索。
- **只读**：源 SQLite 复用 `open_read_only`（`SQLITE_OPEN_READ_ONLY` + `PRAGMA query_only=ON`），
  JSONL / JSON 只读打开；读取失败只影响该次对话视图，不写入 `sync_state`、不改变来源状态。
- **内存内**：解析结果只存在内存中渲染；不写入 `cache.db`、不写日志、不联网，关闭面板即丢弃。
  `cache.db` 的 `task.source_path` 只保存可重建的文件定位指针（SQLite 来源留空，
  读取时用配置的 DB 路径 + `source_id` 定位）。
- **内容口径**：只产出 user / assistant 可见纯文本；工具调用折叠为「工具调用 N 次」计数行，
  不渲染 tool 输入/输出、思考过程、附件、系统注入包装；坏行 / 非 UTF-8 行跳过继续。
- **有界读取**：单条消息 8,000 字符截断标注，单会话累计 200,000 字符只保留最近部分。
  JSONL 读取为流式逐行 + 有界驻留（`VecDeque` 保最近），大文件无需整体驻留；
  SQLite 读取按序查询消息表；Cline 整文件单次 JSON 解析。
- **能力集合**：`models.rs` 的 `SUPPORTED_TRANSCRIPT_SOURCES`（9 个）是事实源，
  前端 `src/lib/transcript.ts` 为手工同步的镜像（models.rs 是事实源，改动需双侧同步）；
  Trae CN 与 CodeBuddy 不提供正文，任务行不渲染对话图标。
- 命令层只接受任务 key（与 `continue_task` 同规校验）；来源不支持返回 `Err`，
  读取期失败（缺指针、文件缺失、空会话等）返回 `Ok(TranscriptOutcome::Error{reason})`
  由前端在面板内显示。

## 数据模型

`cache.db` 是派生快照，schema 版本为 `user_version = 2`：

| 表 | 内容 |
| --- | --- |
| `task` | 任务快照：`key`（`source:source_id`）、标题、项目、时间、状态、模型、分支、todo 计数、resume 命令、指纹、`source_missing`、`source_path`（文件型来源的会话文件定位指针，可重建、不含正文；v1 → v2 幂等迁移，删库重建即 v2）等 |
| `task_todo` | 待办项：`task_key`、`item_id`、subject、status、position |
| `sync_state` | 每个来源最近一次扫描的状态、时间、错误、文件数 |
| `scan_run` | 每次扫描的审计记录：触发方式、耗时、任务数、缺失数、错误 |

任务身份统一为 `source + source_id`；子会话通过 `parent_key` 关联，自动化任务带
`agent_kind = "automation"`，两者默认不进入主列表。

`TaskView` / `WorkspaceGroup` / `BootstrapPayload` 等 IPC 类型定义在
`src/lib/types.ts`，由 Rust 的 serde 序列化生成对应结构。任务可选带
`usage { token?, credits?, est? }`，payload 可选带 `usage` 汇总；两者在
usage-ledger 不可用时完全不出现。

当前**没有** `overlay.db`：置顶、标签、备注等用户覆盖层尚未实现（见 [ROADMAP.md](ROADMAP.md)）。

## 只读不变量

这些约束是代码审查的重点，修改采集路径时必须保持：

1. 源 SQLite 只用 `SQLITE_OPEN_READ_ONLY` 打开，并设置 `PRAGMA query_only=ON`。
2. 源 JSONL 只读打开；不写入、不重命名、不删除源文件。
3. 扫描路径只写 `cache.db`，不持有任何源写连接。
4. 默认不读取消息正文、prompt、tool 输出、附件或思考过程；唯一例外是用户打开对话视图时
   `read_session_content` 按「按需、只读、内存内、单次快照」读取该会话的可见文本
   （见「对话正文读取（按需）」），以及 Claude 标题派生读取的首条候选用户文本。
   Claude todo 只读 `subject/status`。
5. `continue_task` 只能使用固定模板 URL / App 激活；session id 必须通过字符集校验。
6. 删除 `cache.db` 必须只导致数据重建，不影响任何 agent。
7. usage-ledger 的 `usage.db` 只读打开，不执行迁移、不触发 sync、不写入任何表；
   其 `usage_turn` / `usage_daily` / `usage_credit` / `usage_est` 必须分轨展示，永不相加。

## 前端结构

`src/App.svelte` 使用 Svelte 5 runes：

- `payload` 保存最近一次 `bootstrap` 结果。
- `flatTasks` 派生当前任务视图，按状态 / 置顶 / `updated_at` 排序，过滤子会话、自动化与归档；`renderedTasks` 每页最多 60 条。
- 视图：`recent`（近 7 天）、`older`、`archived`、`report`；任务分组只显示三个视图与搜索，报告由页脚图标进入；workspace / 来源筛选可组合，侧栏计数联动当前口径，第二栏不显示筛选 chip。Workspaces / 来源列表按全局近 7 天主会话数倒序，超过 8 项折叠；workspace 名称只显示源数据名称，不追加路径父目录、临时目录或分组标记。
- 三组选中：任务 / Workspaces / 来源各自单选，未选中的组表示「全部」，任何操作后至少一组保持选中（组内互斥不变量集中在 setter）；导航项用 `aria-pressed` 表达单选态。报告视图不计入三组，进入时清空任务选中，返回时恢复。
- 对话面板：任务行带对话图标按钮（`aria-label="查看对话"`，仅正文来源渲染）；点击后
  `read_session_content` 拉取单次快照，`.transcript-pane` 以 `grid-column: 2 / -1` 占满后两列，
  列表与详情从 DOM 移除、侧栏保持可交互。默认展示最近 50 条；用户消息以靠右气泡呈现
  （约 76% 宽、内文左对齐），助手消息与工具调用行保持全宽 / 左侧。「加载更早」位于消息列表
  第一行，滚到顶部才可见，每次追加 50 条并锚定阅读位置，首开滚到最新；返回 / `Esc` / 切换视图 /
  workspace / 来源筛选 / 刷新后任务不在当前列表均关闭面板；失败与空态在面板内显示一行原因，
  可「重新读取」，不自动退出。
- `selected` 从当前 `flatTasks` 派生，切换视图 / 筛选后不会显示列表外的任务。
- Inspector 显示元数据、todo、resume 命令与单个“继续工作”按钮；按钮以来源品牌色填充，前景色按品牌色对比度自动选深色或白色，不显示独立通道标签或说明段。
- 来源图标内联在 `src/lib/source-icons.ts`，不加载远程资源或图标库。

## 扩展点：新增一个来源

1. 在 `crates/perch-core/src/collectors/` 新建采集器，实现 `Collector`，返回 `SourceResult`。
2. 在 `models.rs` 增加 `SOURCE_*` 常量与 label。
3. 在 `paths.rs` 增加默认路径与环境变量覆盖。
4. 在 `collectors/mod.rs` 的 `all_collectors` 中注册。
5. 在 `probe.rs` 中增加该来源的变化探测（SQLite 或文件签名）。
6. 在 `continue_work.rs` 中定义“继续工作”的深链或 App 激活决策。
7. 决定该来源是否提供对话正文：提供则实现 `transcript.rs` 读取器并加入
   `SUPPORTED_TRANSCRIPT_SOURCES`（同时手工同步前端 `src/lib/transcript.ts`）；不提供则不加入，
   任务行不渲染对话图标。
8. 前端补充来源图标/标签（`source-icons.ts` 与 `SOURCE_COLORS`）。
9. 增加使用临时 fixture 的集成测试；更新 [docs/design-docs/source-designs/SOURCES.md](docs/design-docs/source-designs/SOURCES.md)。

新来源必须遵守只读不变量，且不能在无证据时臆造状态。

## 测试策略

- `cargo test -p perch-core`：采集器 fixture、存储幂等、缺失判定、probe、继续工作决策。
- `npm run check`：Svelte/TypeScript 类型检查。
- `npm run demo`：无 Tauri 的 UI 调试与文档截图（假数据）。
- Tauri 壳与真实 agent 数据以 macOS 手工冒烟为主；CI 不跑 GUI E2E。

## 已知边界

- 未启用 CSP（`tauri.conf.json` 中 `csp: null`）；当前前端不加载任何远程资源，
  加固计划见 ROADMAP。
- 未签名/未公证，分发前需要 codesign + notarize。
- 无 Windows/Linux 版本；采集路径与“打开”逻辑都按 macOS 实现。
