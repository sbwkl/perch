# 数据来源规格

本文记录 Perch 当前版本实际读取的本地数据、字段映射、状态推断与已知限制。
设计阶段的完整论证见 [DESIGN.md](DESIGN.md) 第 6 节。

## 通用原则

- **只读**：SQLite 用 `SQLITE_OPEN_READ_ONLY` + `PRAGMA query_only=ON`；JSONL 只读打开。
- **只取元数据**：标题、项目路径、时间、模型、分支、计数、todo 的 subject、状态、置顶/未读标志。
  不读消息正文、prompt、tool 输出、附件、思考过程。
- **故障隔离**：单个来源或文件失败只影响该来源，其他来源照常；失败时保留上一次快照。
- **任务身份**：`source + source_id`，例如 `qoder:<session_id>`、`claude:<session_id>`。
- **缺失判定**：只有一次完整成功扫描后才把未出现的任务标记为 `source_missing`；
  `partial / error / unavailable` 不会误删快照。
- **防误删**：来源上次任务数 > 20 且本次不足一半时，本次结果降级为 `partial`，保留旧快照。

默认路径均可通过环境变量覆盖，见文末表格。

## 总表

| 来源 | 存储 | 主要文件 | 待办 | 继续工作 |
| --- | --- | --- | --- | --- |
| Qoder | SQLite | `main.sqlite` | 无 | 激活 App |
| Claude Code | JSONL + JSON | `projects/**/*.jsonl`、`sessions/`、`tasks/` | 有 | 终端 `claude --resume` |
| Codex | JSONL + SQLite | `sessions/`、`archived_sessions/`、`session_index.jsonl`、`goals_1.sqlite` | 无（goal 状态） | `codex://threads/<id>` 或终端 |
| OpenCode | SQLite | `opencode.db` | 旧表有，v2 暂无 | 激活 App 或终端 |
| Kilo | SQLite | `kilo.db` | 旧表有 | 激活 App 或终端 |
| Trae CN | usage-ledger hooks JSONL | `trae-cn/events-*.jsonl` | 无 | 激活 App |
| ChatGPT 桌面 | — | 普通聊天无本地数据；Codex 会话经 Codex 采集 | — | 未支持 |

---

## Qoder

**路径**

```
~/Library/Application Support/com.qoder.app.stable/main.sqlite
```

覆盖变量：`PERCH_QODER_DB`。

**读取内容**

- 表 `chat_sessions`：`session_id`、`session_kind`、`owner_session_id`、`title`、`workspace_id`、
  `cwd`、`git_branch`、`model`、`created_at`、`updated_at`、`archived`、`unread`、`extra_json`。
  过滤 `deleted_at IS NULL`。
- `chat_session_messages`：按 session 统计消息数；取每个 session 最后一条带 role 的消息。
- `chat_session_turn_states`：最近一次 turn 状态。
- `chat_session_sidebar_placements`：读取 `pinned`，只读展示，不改动。

**标题优先级**

1. `title` 非空；若 `extra_json.titleSource` 存在则照抄（如 `custom` / `ai`）。
2. `extra_json.provisionalTitle`，标记 `provisional`。
3. `新会话`，标记 `fallback`。

**状态推断**

1. `archived` → `archived`。
2. turn state 最新值为 `running` → `running`。
3. turn_states 为空时兜底：最后一条带 role 的消息是 `user`（尚无回复），且会话在 24 小时内更新 →
   `running`（`status_raw = waiting_reply`）。
4. 其他 → `idle`；未知 turn 状态保留 `status_raw`。

**项目与子会话**

- `workspace_id` 为空时，`cwd` 常是 `~/Documents/Qoder/<日期>/<会话前缀>` 自动沙箱，不作为项目路径。
- `session_kind = side_chat` 的会话存 `parent_key`（`qoder:<owner_session_id>`），默认不进入主列表。

**继续工作**

Qoder App 已注册 `qoder://`，但不支持会话级 URL（`main.log` 报 `DEEP_LINK_ROUTE_UNSUPPORTED`），
因此只能 `open -b com.qoder.app` 激活窗口，界面上会提示“无法定位到具体会话”。

**已知限制**

- 没有 todo 数据。
- 运行状态依赖 turn_states 或“最后一条是 user”的启发式；它表示“可能正在等待回复”，不是进程状态。

**变化探测**：`PRAGMA data_version`。

---

## Claude Code

**路径**

```
~/.claude/projects/<project>/<sessionId>.jsonl     # 顶层会话
~/.claude/projects/**/subagents/...                 # 子会话，仅计数，不解析
~/.claude/sessions/<pid>.json                       # 运行中进程的实时状态
~/.claude/tasks/<sessionId>/*.json                  # 待办
```

覆盖变量：`PERCH_CLAUDE_DIR`。

**读取内容**

- 只解析 `projects/` 下深度为 `<project>/<sessionId>.jsonl` 的顶层文件；路径中包含 `subagents`
  的文件跳过。
- 从 JSONL 中读取：`cwd`、`gitBranch`、`timestamp`、`type`、`message.model`、`isSidechain`。
- `message_count` = 非 sidechain 的 user + assistant 记录数。
- 标题：最后一条 `custom-title.customTitle` → 最后一条 `ai-title.aiTitle` → `新会话`。
- 时间：首/末记录 timestamp，缺失用文件 mtime。

**状态**

Claude Code 在 `~/.claude/sessions/<pid>.json` 写入运行中进程的实时状态（只读）：
`busy` → 运行中，`waiting` → 等待中，`idle` → 空闲；进程已退出或没有状态文件时记为 `idle`。
不提供会话完成/归档状态，不臆造 `done`。

**待办**

`~/.claude/tasks/<sessionId>/*.json`，只读 `subject` / `status`（只读 subject，不读 description）。
状态映射：`pending`、`in_progress`、`completed`、`cancelled`、`unknown`。

**继续工作**

终端执行 `cd '<cwd>' && claude --resume <sessionId>`。Claude 桌面 App 只能新建会话，不能恢复指定会话。

**已知限制**

- 实时状态依赖 Claude Code 的 `~/.claude/sessions/<pid>.json`；旧版本没有该文件时统一显示空闲。
- 子会话（`subagents/`）暂不进入主列表，也不读取其待办。
- 没有运行/完成状态。

**变化探测**：`projects/` 下 JSONL 的文件数 + 最新 mtime 签名。

---

## Codex

**路径**

```
~/.codex/sessions/**/*.jsonl
~/.codex/archived_sessions/**/*.jsonl
~/.codex/session_index.jsonl
~/.codex/goals_1.sqlite
```

覆盖变量：`PERCH_CODEX_DIR`。

**读取内容**

- 每个 rollout 读第一行 `session_meta`：`id`、`timestamp`、`cwd`、`originator`、`thread_source`、`model`；
  尾部 256 KB 用于最新事件时间与运行状态。不扫完整 rollout。
- `session_index.jsonl` 提供 `id → thread_name` 与 `updated_at`。
- `goals_1.sqlite` 提供 `thread_goals` 的 `objective`、`status`。
- `.jsonl.zst` 不解析。

**更新时间**

`updated_at` 取 `session_index.updated_at`、rollout 尾部最后一条事件时间、`session_meta.timestamp`
三者的最大值。`session_index.jsonl` 可能长期不更新，不能单独作为续用旧会话的时间依据。

**标题优先级**

1. `session_index.thread_name`。
2. `thread_goals.objective`（截断 240 字符）。
3. `新会话`。

**状态推断**

1. 文件位于 `archived_sessions` → `archived`。
2. 文件在 24 小时内修改，且尾部 256 KB 中最后一条 `event_msg` 为 `task_started` → `running`
   （`task_complete` / `turn_aborted` 结束运行）。
3. 有 goal 时按 goal status 映射：`active`、`paused`、`blocked`、`limited`、`complete`、`archived`。
4. 否则 `idle`。

**子会话与自动化**

- `thread_source = automation` → `agent_kind = automation`，默认不进入主列表。
- `originator` 含 `Desktop` → `desktop`；含 `tui` / `exec` → `cli`。
- 同一 id 同时存在于 `sessions/` 与 `archived_sessions/` 时只保留一条，活跃副本优先。

**项目**

`cwd` 若形如 `~/Documents/Codex/<日期>/<名称>`（Codex Desktop 自动沙箱），不作为项目路径。

**继续工作**

- `desktop` / `automation`：`codex://threads/<id>`。
- `cli`：终端 `cd '<cwd>' && codex resume <id>`。
- 未知 `agent_kind`：根据 Codex App 是否运行、CLI 进程是否存在决定。

**已知限制**

- 标题依赖 `session_index` 与 goal，覆盖不完整时显示 fallback；属正常情况。
- goal 数据当前可能为空，此时状态只有运行/空闲。

**变化探测**：`sessions/` + `archived_sessions/` 下 JSONL 的文件数 + 最新 mtime 签名。

---

## OpenCode

**路径**

```
~/.local/share/opencode/opencode.db
~/.local/share/opencode/opencode-stable.db   # 回退
```

覆盖变量：`PERCH_OPENCODE_DB`、`PERCH_OPENCODE_STABLE_DB`。

**读取内容**

- 优先 `session_v2` 表；旧 `session` 表自 2026-10-21 起不再更新，仅在缺少 `session_v2` 时回退。
- 字段：`id`、`parent_id`、`title`、`directory`、`model`、`time_created`、`time_updated`、
  `time_archived`、`time_busy`、`time_idle`。
- `model` 可能是 JSON，读取 `id` 或 `name`。
- 默认占位目录 `~/Documents/Default Project` 不作为项目。

**标题 / 状态**

- 标题：`title` 非空 → 原样；否则 fallback。
- `time_archived > 0` → `archived`。
- 否则 `time_busy > time_idle` → `running`（`status_raw = busy`），其他 `idle`。

**子会话 / 待办**

- `parent_id` 非空 → 存 `parent_key`，默认不进入主列表。
- `todo` 表存在时读取 `content` / `status` / `position`。新版 OpenCode 已把待办内嵌到
  `session_message`，v2 会话可能没有 todo 行，此时不显示进度。

**继续工作**

- App 运行中：激活 `ai.opencode.desktop`。
- 否则：终端 `cd '<directory>' && opencode --session <id>`。

**变化探测**：`PRAGMA data_version`。

---

## Kilo

**路径**

```
~/.local/share/kilo/kilo.db
```

覆盖变量：`PERCH_KILO_DB`。

**读取内容**

Kilo 的 schema 与旧版 OpenCode `session` 表同构（没有 `session_v2`、`time_busy/time_idle`），
采集器自动回退到 `session` 表。

- 标题、项目、父会话处理同 OpenCode。
- 状态：`time_archived > 0` → `archived`，否则 `idle`。
- `todo` 表存在时读取待办。

**继续工作**

- App 运行中：激活 `ai.kilo.desktop`。
- 否则：终端 `cd '<directory>' && kilo`。Kilo CLI 没有已确认的会话恢复参数，因此不拼接 session id；
  详情面板中的恢复命令也只展示可确认的部分。

**变化探测**：`PRAGMA data_version`。

---

## Trae CN

**路径**

```
~/.local/share/usage-ledger/trae-cn/events-<date>.jsonl
~/.trae-cn/memory/projects/<encoded-project>/<date>/session_memory_<session>.jsonl
```

覆盖变量：`PERCH_TRAE_CN_EVENTS_DIR`、`PERCH_TRAE_CN_MEMORY_DIR`；设置 `USAGE_LEDGER_HOME`
时事件目录跟随其目录。

**来源说明**

Trae CN 没有可读的本地会话索引：`state.vscdb` 只有模型配置与当前窗口状态，
agent 数据库 `ModularData/ai-agent/database.db` 是加密 SQLite，会话列表在服务端。
Perch 不接触加密库，只读取 usage-ledger 的 Trae hook 事件——它由 Trae 的
`UserPromptSubmit` / `Stop` hook 追加，写入纯文本元数据，不包含消息正文。
hook 安装晚于部分历史会话，因此再枚举 `memory/projects` 下的
`session_memory_<session>.jsonl` 文件名来覆盖安装前的会话；读取文件名、日期目录、
文件 mtime，以及每个会话首条记录的 `intent` 摘要作为标题，不读消息正文。

安装 hook 后才会出现会话；未安装时该来源显示 `unavailable`，提示执行：

```bash
usage hooks install trae-cn --scope global
```

**读取内容**

- 枚举 `events-*.jsonl`，逐行只读 `session`、`ts`、`project`、`model`。
- 枚举 `memory/projects/**/session_memory_*.jsonl` 的文件名作为历史会话 id；
  `created_at` 取日期目录的本地零点，`updated_at` 取文件 mtime。
- 标题读取每条会话首个记录的 `intent` 字段（AI 生成的短摘要，截断到 60 字）；
  `actions`、`outcome`、`learned`、`message_id` 与消息正文一律不读。
- 历史会话与 hook 会话按 session id 合并，hook 事件的时间与状态优先；
  历史会话状态固定 `idle`。
- 项目名从编码目录名启发式还原（去掉 `--p2-<hash>` 与常见父目录），不伪造 `project_path`。
- `tool`、`tool_use_id`、`loop`、`chars_in`、`chars_out` 等字段不读取、不存储。
- `session = "unknown"` 的行跳过；无法解析的行计入 warning，来源降级为 `partial`。
- 按 `session` 聚合：`created_at` 取最早 `ts`，`updated_at` 取最晚 `ts`，
  `project` / `model` 取时间最晚的非空值。

**标题 / 状态**

- 标题优先使用 memory 首条 `intent` 摘要（`title_source = ai`，超过 60 字截断）；
  没有 memory 记录时显示 `新会话`（`title_source = fallback`）。
- 状态由会话中时间最晚的已知事件推导：`UserPromptSubmit` / `PreToolUse` / `PostToolUse`
  视为 `running`，`SessionStart` / `Stop` 视为 `idle`；未知事件不改变状态。
- 状态没有时间衰减：若 Trae CN 在回合中退出且未产生 `Stop`，会话会停留在 `running`，
  直到下一次事件或手动刷新。
- `project` 只是 hook 记录的 basename，因此写入 `project_name`，没有 `project_path`。
- 没有 hook 事件的历史会话只能按 memory 文件名估计项目与时间，项目名可能与真实路径不完全一致。

**继续工作**

Trae CN 没有确认的会话级深链，只能 `open -b cn.trae.app` 激活窗口。

**已知限制**

- 历史会话只覆盖有 `session_memory_*.jsonl` 的会话，不是完整历史列表。
- 没有待办、消息数和归档状态；标题固定为 fallback `新会话`，`agent_kind` 固定为 `desktop`。
- 事件文件缺失或未安装 hook 时来源不可用；单个坏行会导致 `partial`。

**变化探测**：事件与 memory 目录下 JSONL 的文件数 + 最新 mtime 签名。

---

## 未支持的来源

| 来源 | 原因 |
| --- | --- |
| ChatGPT 桌面 | 普通聊天无本地任务数据；Codex 会话写入 `~/.codex`，经 Codex 采集 |
| WorkBuddy | 未安装，schema 未知；不猜测 |

WorkBuddy 不会出现在侧边栏；ChatGPT 桌面的 Codex 会话计入 Codex，普通聊天暂无本地数据。
如果未来出现稳定的只读数据源，会以独立采集器加入。

---

## 只读验证

自动测试覆盖了采集器的只读行为与快照语义：

```bash
cargo test -p perch-core
```

手工验证时可以：

1. 记录源数据库主文件（以及 Qoder `chat_session_sidebar_placements.pinned`）的 mtime/内容和
   JSONL 的 mtime。
2. 在 Perch 中点击刷新、浏览任务。
3. 再次比较，源文件应保持不变。

> 只读连接可能触碰 SQLite 的 `-wal` / `-shm` 辅助文件，比较时应以主数据库和业务字段为准；
> Perch 自身不执行任何源写 SQL。

## 环境变量覆盖

| 变量 | 作用 |
| --- | --- |
| `PERCH_SOURCE_HOME` | 替换所有默认路径的 home 基准 |
| `PERCH_QODER_DB` | Qoder 数据库 |
| `PERCH_CLAUDE_DIR` | Claude 数据目录 |
| `PERCH_CODEX_DIR` | Codex 数据目录 |
| `PERCH_OPENCODE_DB` | OpenCode 主数据库 |
| `PERCH_OPENCODE_STABLE_DB` | OpenCode 回退数据库 |
| `PERCH_KILO_DB` | Kilo 数据库 |
| `PERCH_TRAE_CN_EVENTS_DIR` | Trae CN hook 事件目录 |
| `PERCH_CACHE_DB` | Perch 快照缓存 |
