# 本机数据探测记录

> 探测时间：2026-11-02 ｜ 环境：macOS 27.0.0 (arm64) ｜ Python 3.12 / sqlite3
>
> 目的：确认 Perch 的每个 source 是否真的存在“任务列表 + 标题 + 项目 + 状态 + 子项”元数据。
> 所有查询均为只读（SQLite `mode=ro` / 文件只读），不包含 message/prompt 正文。
>
> 注意：本文件只记录 schema、字段可用性、计数与耗时；**不记录任何任务标题或 prompt 内容**。

---

## 1. Qoder（`qoder`）

### 1.1 路径与容器

| 项 | 值 |
|---|---|
| 数据库 | `~/Library/Application Support/com.qoder.app.stable/main.sqlite` |
| 大小 | 约 259 MB（探测时 259,256,320 bytes） |
| 相关表 | `chat_sessions`、`chat_session_messages`、`chat_session_turn_states`、`chat_session_sidebar_placements`、`attention_completions`、`scheduled_tasks` |
| URL scheme | `qoder` / `qoder-app`（Info.plist 已确认） |
| 会话级深链 | **未验证**；本设计不猜测 URL 格式 |

### 1.2 会话元数据

`chat_sessions` 可用字段（探测时）：

```
session_id, origin_session_id, session_kind, owner_session_id,
conversation_mode, product_mode, title, cwd, git_branch, model,
created_at, updated_at, archived, unread, archived_at, deleted_at,
extra_json, workspace_id, execution_target_json
```

`chat_session_sidebar_placements` 另有：

```
session_id, work_directory, sort_order, pinned, updated_at
```

**重要**：源自己有 `pinned` 字段。Perch 只读展示“源置顶”，全局置顶写入自己的 `overlay.db`。

### 1.3 计数（探测时）

| 指标 | 值 |
|---|---:|
| `chat_sessions` 总行数 | 183 |
| `deleted_at IS NOT NULL` | 0 |
| `archived=1` | 6 |
| `unread=1` | 1 |
| `model` 非空 | 183 |
| `git_branch` 非空 | 131 |
| `workspace_id` 非空 | 175 |
| `chat_session_messages` 总行数 | 4,111 |
| `session_kind` 分布 | `standard=183` |
| `extra_json.source` 分布 | `workspace=183` |
| `extra_json.titleSource` 分布 | 空=130、`ai`=43、`custom`=4、`fallback`=6 |
| `chat_session_turn_states` 行数 | 0（表存在，当前无状态行） |
| `attention_completions` 行数 | 0 |

### 1.4 性能

```sql
SELECT session_id, COUNT(*), MAX(created_at)
FROM chat_session_messages GROUP BY session_id;
```

实测约 **0.08s**（4,111 行）。按会话取消息计数不影响首屏。

### 1.5 结论

- 可作为 P0 任务源：会话、标题、项目、时间、归档/未读、模型、分支齐全。
- 状态较薄：`chat_session_turn_states` 当前为空；默认显示 `idle`，不臆造完成状态。
- 标题优先级：`title` → `extra_json.provisionalTitle` → `新会话`；`titleSource` 仅内部保留，不展示。
- `scheduled_tasks` 是 Qoder 的定时任务，不是用户会话任务，**不纳入**本项目。

### 1.6 hooks（P2 相关，既有探测 2026-10-30）

- 用户级配置：`~/.qoder/settings.json` 的 `hooks` 字段；Claude Code 风格，支持 command/http、matcher。
- 事件：12 个，含 `SessionStart` / `UserPromptSubmit` / `Stop` / `PostToolUse` 等；payload 不含 token。
- 用途：P2 可安装 opt-in command hook，把 `{event, session_id, cwd, ts}` 追加到 Perch 事件文件，
  补足数据库里没有的“正在运行/已停止”语义；payload 是否含 `session_id` 需在实现时验证。
- 该方案会修改 Qoder 配置，默认不启用；必须可一键卸载。

---

## 2. Claude Code（`claude`）

### 2.1 路径与结构

| 项 | 值 |
|---|---|
| 顶层会话 | `~/.claude/projects/<encoded-project>/<sessionId>.jsonl` |
| 子代理 | `~/.claude/projects/.../subagents/agent-*.jsonl` |
| 会话内 todo | `~/.claude/tasks/<sessionId>/<n>.json` |
| `~/.claude` 大小 | 约 110 MB |
| 顶层 JSONL 大小 | 约 25 MB（探测时） |

### 2.2 可用元数据

顶层 JSONL 中出现的记录类型（实测一种样本）：

```
assistant, user, mode, permission-mode, last-prompt, attachment,
file-history-snapshot, system, ai-title, custom-title
```

- 标题：
  - `{"type":"custom-title","customTitle":"...","sessionId":"..."}`
  - `{"type":"ai-title","aiTitle":"...","sessionId":"..."}`
- 项目：记录的 `cwd` 字段；文件目录名是编码后的路径，不直接使用。
- 时间：记录 `timestamp`；文件 mtime 兜底。
- 模型：`message.model`。
- 分支：`gitBranch`（存在时）。
- 子代理：独立 JSONL，位于 `subagents/`；顶层任务不混入子代理记录。
- Todo JSON 字段：

```json
{"id":"1","subject":"...","description":"<不读取>","activeForm":"...",
 "status":"pending","blocks":"[]","blockedBy":"[]"}
```

只读取 `subject/status/blocks/blockedBy`，**不读 `description`**。

### 2.3 计数（探测时）

| 指标 | 值 |
|---|---:|
| 顶层 session JSONL | 14 |
| 有 `custom-title` 或 `ai-title` 的顶层会话 | 14 / 14 |
| 顶层 JSONL 总量 | 约 25 MB |
| `~/.claude/tasks` | 存在，多个 session 目录与 `*.json` |

### 2.4 结论

- 可作为 P0：标题覆盖率高，项目/时间/模型齐全。
- **无会话完成/归档状态**；Claude Code 没有类似 Qoder 的 archived 字段。进程级实时状态见
  `~/.claude/sessions/<pid>.json`（`busy` / `idle` / `waiting`），Perch 只读接入；人工状态由 overlay 补。
- todo 可作为子项展示，进度可用于筛选“未完成 todo”。
- resume 命令已确认：`claude --resume <session-id>`。

---

## 3. Codex（`codex`）

### 3.1 路径

| 项 | 值 |
|---|---|
| 活跃 rollout | `~/.codex/sessions/**/*.jsonl` |
| 归档 rollout | `~/.codex/archived_sessions/**/*.jsonl` |
| 会话索引 | `~/.codex/session_index.jsonl` |
| goal 状态 | `~/.codex/goals_1.sqlite` → `thread_goals` |
| 历史 prompt | `~/.codex/history.jsonl`（**不读文本**） |
| `~/.codex` 大小 | 约 862 MB（含日志/缓存，不是全部需要读） |

### 3.2 session_meta（rollout 第一行）

字段样本（值已抽象）：

```
session_id, id, timestamp, cwd, originator, cli_version,
source, thread_source, model_provider, base_instructions,
history_mode, context_window
```

- `originator` 分布（69 个 rollout）：
  - `Codex Desktop` = 42
  - `codex-tui` = 25
  - `codex_exec` = 2
- `thread_source` 分布：`user=67`、`automation=2`
- 首行 `session_meta` 即可拿到 id/cwd/originator/thread_source/created_at；另读尾部 256KB 取最新活动时间，
  实测 69 个文件约 **0.09s**。
- `.jsonl.zst` 归档探测时 0 个；实现仍应跳过并计数，避免未来漏读。

### 3.3 标题与状态

- `session_index.jsonl`：`id`、`thread_name`、`updated_at`；探测时 **38 条**（少于 69 个 rollout）。
- `goals_1.sqlite` 的 `thread_goals`：

```sql
thread_id TEXT PRIMARY KEY,
goal_id TEXT NOT NULL,
objective TEXT NOT NULL,
status TEXT CHECK(status IN ('active','paused','blocked','usage_limited','budget_limited','complete')),
token_budget INTEGER,
tokens_used INTEGER,
time_used_seconds INTEGER,
created_at_ms INTEGER,
updated_at_ms INTEGER
```

- **探测时 `thread_goals` 为 0 行**（schema 存在，当前没有 goal 数据）。因此 Codex 状态当前以 `idle` 为主；
  未来有 goal 行时再映射 blocked/paused/complete 等。
- 标题缺失的会话用 goal objective 或 fallback；不能从 `history.jsonl` 抄 prompt 正文。

### 3.4 结论

- 可作为 P0：会话 id、项目、originator、更新时间、标题（部分）可读。
- 标题覆盖率不是 100%，必须在 doctor 中报告；fallback 不读正文。
- resume 命令已确认：`codex resume <session-id>`；CLI 还提供 `archive/unarchive`，但 Perch 不调用。
- 归档目录中的 rollout 标记 `source_archived=1`，与活跃目录同 id 去重。

---

## 4. OpenCode（`opencode`）

### 4.1 路径与 schema

| 项 | 值 |
|---|---|
| 数据库 | `~/.local/share/opencode/opencode.db`（约 988 MB） |
| 备选库 | `opencode-stable.db` |
| 会话表 | `session`（另有 `session_v2`、`session_message`） |
| todo 表 | `todo` |

`session` 关键字段：

```
id, project_id, parent_id, slug, directory, path, title,
summary_additions, summary_deletions, summary_files,
cost, tokens_input, tokens_output, tokens_reasoning,
tokens_cache_read, tokens_cache_write,
agent, model, time_created, time_updated, time_archived
```

`todo` 字段：`session_id, content, status, priority, position, time_created, time_updated`。

### 4.2 计数（探测时）

| 指标 | 值 |
|---|---:|
| `session` 总行数 | 107 |
| 顶层（`parent_id IS NULL`） | 68 |
| `time_archived` 非空 | 29 |
| `todo` 行数 / 涉及会话 | 72 / 13 |

### 4.3 结论

- 可作为 P0：标题、项目目录、时间、归档、todo 齐全；子会话用 `parent_id` 折叠。
- 会话表自带 token 字段，但本设计 MVP **不把它当作跨源 token 口径**；token 展示留给 usage-ledger 关联（P2），避免各源口径不一致。
- resume 已确认：`opencode --session <id>`（帮助中 `--session, -s`）。
- 只读打开 DB；注意 WAL 文件可能包含最新写入。

---

## 5. Kilo（`kilo`）

### 5.1 路径与 schema

| 项 | 值 |
|---|---|
| 数据库 | `~/.local/share/kilo/kilo.db`（约 104 MB） |
| 会话表 | `session`（与 OpenCode 同构） |
| todo 表 | `todo` |
| board 表 | `kilo_board`、`kilo_board_message` |

### 5.2 计数（探测时）

| 指标 | 值 |
|---|---:|
| `session` 总行数 | 9 |
| 顶层（`parent_id IS NULL`） | 6 |
| `time_archived` 非空 | 2 |
| `todo` 行数 / 涉及会话 | 10 / 2 |
| `kilo_board` 行数 | 6 |

### 5.3 结论

- 可作为 P0：字段与 OpenCode 基本一致。
- `kilo_board` 的 `objective` 可作为详情里的长目标，不优先作为标题。
- Kilo CLI 帮助只显示 `kilo session list/delete`，**未确认** `--session` 恢复参数；MVP 只复制 `kilo` 命令 + session id + 项目路径，不猜测参数。

---

## 6. Trae CN（`trae-cn`）

### 6.1 探测结果

| 项 | 值 |
|---|---|
| `~/Library/Application Support/Trae CN/User/globalStorage/state.vscdb` | 约 970 KB |
| `ItemTable` 中的任务相关键 | `chat.ChatSessionStore.index` = `{"version":1,"entries":{}}`（空） |
| 模型配置键 | `AI.agent.model.model_list_map` 等（非任务） |
| `ModularData/ai-agent/database.db` | SQLCipher 加密 SQLite（页头不可读，未尝试解密） |
| 其他明文痕迹 | `snapshot/<session>`、`memory/projects/**/session_memory_*.jsonl` 仅覆盖部分会话，没有标题与完整索引 |
| `~/.trae-cn/hooks.json` | 不存在 |
| `~/.local/share/usage-ledger/trae-cn` | 不存在（未安装 usage-ledger hooks） |
| URL scheme | `trae-cn`（Info.plist 已确认） |

### 6.2 结论

- **没有可直接读取的本地任务列表**；session 列表在服务端，本地 agent 库加密。
- **已实现**：安装 usage-ledger 的 Trae hooks 后，从 `~/.local/share/usage-ledger/trae-cn/events-*.jsonl`
  聚合会话；状态按最新事件推导 `running` / `idle`，标题 fallback `新会话`。
- **已实现（历史会话）**：枚举 `~/.trae-cn/memory/projects/**/session_memory_*.jsonl` 的文件名，
  补充 hook 安装前的会话；只读日期目录与文件 mtime，不打开文件内容，项目名启发式还原。
- `snapshot` 目录不足以构成稳定索引，不作为数据源。
- 未安装 hook 时 source 状态为 `unavailable`，doctor/UI 明确提示安装命令。

---

## 7. ChatGPT 桌面 / WorkBuddy

| 源 | 探测 | 结论 |
|---|---|---|
| ChatGPT 桌面 | `~/Library/Application Support/com.openai.chat` 不存在；桌面端 bundle 为 `com.openai.codex`，Codex 会话写入 `~/.codex` | 普通聊天云端存储、无本地任务数据；Codex 会话经 Codex 采集 |
| WorkBuddy | `~/.workbuddy` 不存在 | 本机未安装，后续 adapter |

---

## 8. 只读性与性能汇总

| 源 | 扫描量 | 实测/估算 | 备注 |
|---|---|---:|---|
| Qoder | 183 会话 + 4,111 消息 | 0.08s（group by） | main.sqlite 约 259MB |
| Claude | 14 个顶层 JSONL，约 25MB | 秒级以内 | 只解析所需字段 |
| Codex | 69 个 rollout，只读首行 + 尾部 256KB | 0.09s（首行 + 尾部） | 不扫完整 rollout |
| OpenCode | 107 会话；72 todo | 亚秒级 | WAL 只读 |
| Kilo | 9 会话；10 todo；6 board | 亚秒级 | WAL 只读 |
| Trae CN | 事件 JSONL + memory 会话文件名（探测时 13 个会话） | 亚秒级（逐行只取 `session / ts / project / model`，memory 只读路径与 mtime） | hook 后会话 + 部分历史会话 |

首版全量扫描目标 **<1.5s**；`serve` 请求按 5s 节流，CLI 用 mtime 签名判断是否刷新。

---

## 9. 探测命令（可复现）

以下命令均为只读；不要在源目录上执行任何写操作：

```bash
# Qoder 会话与消息计数
sqlite3 "file:$HOME/Library/Application Support/com.qoder.app.stable/main.sqlite?mode=ro" \
  "SELECT COUNT(*), SUM(archived), SUM(unread) FROM chat_sessions;"

# Qoder 标题来源分布（不取标题文本）
sqlite3 "file:$HOME/Library/Application Support/com.qoder.app.stable/main.sqlite?mode=ro" \
  "SELECT COALESCE(json_extract(extra_json,'$.titleSource'),'?'), COUNT(*)
   FROM chat_sessions GROUP BY 1;"

# Claude 顶层会话数
find ~/.claude/projects -maxdepth 2 -name '*.jsonl' -not -path '*/subagents/*' | wc -l

# Codex 只读第一行 session_meta（Python）
python3 - <<'PY'
import glob, json, os
for p in glob.glob(os.path.expanduser('~/.codex/sessions/**/*.jsonl'), recursive=True):
    with open(p, errors='ignore') as f:
        o = json.loads(f.readline())
        if o.get('type') == 'session_meta':
            print(o['payload'].get('id'), o['payload'].get('cwd'))
PY

# OpenCode / Kilo 会话计数
sqlite3 "file:$HOME/.local/share/opencode/opencode.db?mode=ro" \
  "SELECT COUNT(*), SUM(parent_id IS NULL), SUM(time_archived IS NOT NULL) FROM session;"
sqlite3 "file:$HOME/.local/share/kilo/kilo.db?mode=ro" \
  "SELECT COUNT(*), SUM(parent_id IS NULL), SUM(time_archived IS NOT NULL) FROM session;"

# Trae CN 任务索引（探测时为空）
sqlite3 "file:$HOME/Library/Application Support/Trae CN/User/globalStorage/state.vscdb?mode=ro" \
  "SELECT key, value FROM ItemTable WHERE key='chat.ChatSessionStore.index';"
```

---

## 10. 探测未覆盖 / 需在实现时验证

1. Qoder 会话级深链 URL 格式（`qoder://` 已确认存在，但 session 路径未确认）。
2. Qoder `chat_session_turn_states.status` 的真实取值集合（当前无行）。
3. Kilo 是否有可用的会话恢复参数（当前只确认 `kilo session list/delete`）。
4. Codex `thread_goals` 在实际使用后的状态分布（当前 0 行）。
5. ~~Claude Code 是否有未落盘的“正在运行”状态（当前只能 `idle`）~~：已确认
   `~/.claude/sessions/<pid>.json` 提供 `busy` / `idle` / `waiting`，Perch 已只读接入。
6. Trae hooks 安装后的实际事件字段与覆盖范围。
7. OpenCode/Kilo 的 `session` 表在桌面版/CLI 双写时是否会出现副本（按 id 覆盖即可，但需 fixture 验证）。
8. Qoder 新会话的落库时机：打开 tab 即写 `chat_sessions`，还是第一条消息后才写；`main.sqlite-wal`
   的写入/checkpoint 行为决定 G 的实际可见延迟。
9. Qoder hooks 的 payload 是否包含 `session_id`；决定 P2 能否在 `SessionStart` 时建占位任务。
10. Claude Code 顶层 JSONL 是否严格 append-only；标题行/最后一行半行、`tasks/*.json` 是否原子替换；
   决定 `file_cursor` 的重置条件。
11. Codex `session_index.jsonl` 是否只追加；若整体重写，offset 失效时的全量重读成本。
12. OpenCode/Kilo/Qoder 的 WAL 提交是否让 `data_version` 及时变化；不稳定时以 `-wal` mtime/size 回退。
13. Qoder `chat_sessions` 与 `chat_session_sidebar_placements` 是否在同一事务更新；成员指纹是否覆盖
    所有会影响列表显示的字段。
14. Qoder `chat_sessions.updated_at` 是否随每条消息更新；若不更新，活动指纹改用
    `chat_session_messages` 的 `max(rowid)` 或计数。
