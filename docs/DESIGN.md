# Perch · 多 agent 任务总览 · 设计方案 v0.3.3

| 项 | 内容 |
|---|---|
| 文档状态 | **产品决策已确认**（2026-11-02）；v0.1.0 已实现只读聚合与实时刷新，覆盖层待实现 |
| 形态 | **macOS 原生 `.app`**，Tauri v2 + Rust 后端 + 系统 WebView 前端 |
| 名称 | **Perch**（2026-11-02 已确认；命名参考见 [NAMING.md](NAMING.md)） |
| 目标 | 多 agent 任务总览 + 跨 agent 全局置顶；覆盖层独立存储，默认不修改源 agent 数据 |
| 一致性 | 源是唯一事实来源；快照可重建；启动全量对账 + 签名跳过 + 周期全量 + 窗口可见时全源 probe（§8） |
| 参考项目 | `/Users/wayne/workspace/playground/usage-ledger`（只读源、SQLite、离线、轻量） |
| 本机依据 | [DATA-PROBE.md](DATA-PROBE.md)（2026-11-02 实测，非推测） |
| 推荐技术栈 | Rust stable；Tauri v2；Svelte 5 + TypeScript + Vite；SQLite（rusqlite bundled） |

> 说明：你提到的 “turai-app” 应为 **Tauri**（[tauri-apps/tauri](https://github.com/tauri-apps/tauri)，
> 官网 [v2.tauri.app](https://v2.tauri.app/)）。Tauri 用 Rust 写应用逻辑、系统 WebView 渲染界面，
> 产物是原生 `.app`/`.dmg`，不是 Electron 那种自带 Chromium 的运行时。本文按 Tauri v2 设计。

> 实现状态：M1（只读聚合）与 M3 的列表/实时刷新已落地；`overlay.db`、置顶及部分命令仍在设计中。
> 当前代码结构见 [ARCHITECTURE.md](../ARCHITECTURE.md)，进度见 [ROADMAP.md](../ROADMAP.md)。

---

## 0. 结论摘要

- **Task = 一个 agent 的顶层会话/线程**（Qoder chat session、Claude Code session、Codex thread、OpenCode/Kilo session）。
  会话里的 TodoWrite / todo 作为**子项**展示，不单独占用全局任务列表。
- **源是唯一事实来源**：Rust 只读 agent 本地库/JSONL，归一成 `TaskSnapshot`；本地 `cache.db` 只是可重建的快照索引。
- **覆盖层独立**：MVP 只做**全局置顶/取消置顶**；人工状态、标签、备注、隐藏、跨 agent 分组等
  后续能力也只写入独立 `overlay.db`，不碰源。快照扫描永不写 overlay，也默认不写 agent 目录。
  Qoder 自带的 `sidebar_placements.pinned` 只读展示，不修改。
- **一致性**：启动先读快照秒开，后台做全量对账；聚焦/手动刷新用签名跳过未变源；窗口可见时每 3 秒
  对全部 source 做 probe（SQLite `PRAGMA data_version`、JSONL 文件数 + 最新 mtime 签名），只重扫发生
  变化的 source，并通过 `sources-updated` 事件刷新前端；任意源的新任务/新信息通常几秒内出现；
  只有完整扫描成功才标记 `source_missing`（§8）。
- **形态**：Tauri v2 macOS app。MVP 只提供置顶操作，通过 Tauri command 只写 `overlay.db`；
  无本地 HTTP 服务、无云、无网络请求、无常驻 daemon。
- **默认视图**：智能列表第一个是 **最近**（近 7 天，按最后更新时间倒序的平铺列表，workspace 作为
  每行属性展示）；**全部任务** 为全部非归档任务的平铺列表，**已归档** 同为平铺列表，均按最后更新
  时间倒序；源归档任务只出现在已归档视图，子会话、自动化默认隐藏。
- **明确的数据缺口**：Trae CN 当前无本地任务列表（仅可选 hook 事件），ChatGPT 桌面普通聊天无本地数据（Codex 会话经 Codex 采集）；
  MVP 不解密、不爬私有 API、不编造“已完成”状态；后续如要覆盖，按 §11.2 的 opt-in 规则单独评估。
- **可选的伴生 CLI**：复用同一 Rust core，P2 提供 `perch-cli list/pin/doctor/export`，主形态仍是 app。

---

## 1. 目标与非目标

### 1.1 目标

| 目标 | 说明 |
|---|---|
| 一处找任务 | 跨 Qoder / Claude Code / Codex / OpenCode / Kilo 的任务统一列表、搜索、项目过滤 |
| 知道任务在哪 | 每行显示 source、项目、最近活动、源状态、来源置顶/未读等原生标记 |
| 快速回到任务 | 展示 resume 命令 / 项目路径；复制和打开动作后续加入，已验证的桌面端才提供打开方式 |
| 全局置顶 | 置顶不依赖当前筛选，永远出现在顶部“Global pinned”区 |
| 人工整理（后续） | MVP 只做全局置顶；人工状态、标签、备注、隐藏、重命名按版本后续加入 |
| 默认不写源 | 采集只读；MVP 只把全局置顶写入 Perch 自己的 `overlay.db`。后续整理能力同样只写 overlay；如增加回写 agent 的动作，必须显式确认、可审计、可撤销（§8.6） |
| macOS 原生感 | 可安装的 `.app`，原生窗口/菜单/快捷键/系统主题，低资源占用 |
| 极轻量 | 无 Electron；Rust core 可单测；快照可删可重建；覆盖层可单独备份 |

### 1.2 非目标

- 不做多设备同步、账号体系、云服务、团队协作。
- 默认不修改/删除 agent 的会话，不在源里写“已完成”；后续回写动作按 §8.6 走显式确认。
- 不做自动语义聚类或自动判断两个 agent 的任务是同一件事（误判成本高）；跨 agent 合并只做**人工分组**。
- **MVP 只做任务元数据、只读采集和全局置顶**；消息正文、加密源、agent 内操作作为后续可选能力，
  默认关闭，启用前单独评审权限、隐私与安全边界（§8.6、§11.2）。
- 不做菜单栏常驻/后台守护；app 关闭即退出（可选菜单栏模式见 §17 决策 10）。

---

## 2. 本机数据可得性（实测摘要）

完整字段、计数、耗时见 [DATA-PROBE.md](DATA-PROBE.md)。

| 源 | 本地任务列表 | 标题 | 项目 | 状态 | 子项 | 结论 |
|---|---|---|---|---|---|---|
| Qoder | ✅ `main.sqlite/chat_sessions` 183 条 | `title`，`extra_json.titleSource` 可区分 ai/custom/fallback | `cwd` | archived / unread / turn state | 无 todo，只有消息计数 | P0 |
| Claude Code | ✅ 14 个顶层 session JSONL | `custom-title` / `ai-title`，14/14 有标题 | `cwd` | 无显式状态 | `~/.claude/tasks/<session>/*.json` | P0 |
| Codex | ✅ 69 个 rollout + 38 条 `session_index` | `thread_name` / goal objective | session_meta `cwd` | `thread_goals.status`（当前 0 行；有 goal 时可用） | goal objective | P0 |
| OpenCode | ✅ `session_v2` 表 117 条（旧 `session` 107 条已冻结） | `title` | `directory` | `time_archived` / `time_busy` 运行中 | `todo` 表（v2 时代停更） | P0 |
| Kilo | ✅ `session` 表 9 条（6 个顶层） | `title` | `directory` | `time_archived` | `todo` 表 | P0 |
| Trae CN | ❌ 本地只有模型配置；`ChatSessionStore.index` 为空 | 无 | hook 事件可给 project | 无 | 无 | P2 可选（仅 hook 安装后） |
| ChatGPT 桌面 | ❌ 普通聊天无本地数据；Codex 会话经 Codex 采集 | — | — | — | — | 不单独支持 |
| WorkBuddy | 本机未安装 | — | — | — | — | 后续 adapter |

**关键结论**：P0 五个源已经能覆盖用户当前的主要任务；Trae 与 ChatGPT 普通聊天的缺口必须在 Diagnostics 与 UI 中显式说明，
不能让“没采到”看起来像“没有任务”。

---

## 3. 核心概念

| 概念 | 定义 | 例子 |
|---|---|---|
| Task（会话任务） | agent 侧边栏里的一个顶层会话/线程 | Qoder `8e4c2d44-...`、Claude session、Codex thread |
| Child task | 子代理/侧聊/分支会话；默认折叠在父任务下 | Claude `subagents/agent-x.jsonl`、OpenCode `parent_id` |
| Todo | 任务内部的待办清单项 | Claude tasks JSON、OpenCode/Kilo `todo` |
| Source | 一个 agent 的本地数据源 | `qoder` / `claude` / `codex` / `opencode` / `kilo` |
| Snapshot | app 对源元数据的可重建索引 | `task` + `task_todo` 表 |
| Overlay | 用户自己的整理数据，永不写回源 | pin（MVP）/ status / tag / note / hidden / group（后续） |
| Global pin | 跨 agent 的置顶，独立于源置顶 | `task_overlay.pinned=1` |
| Source status | 源自己表达的状态 | Qoder archived、Codex goal blocked |
| Manual status | 用户人工标记，不声称源的真实状态 | `todo / doing / done / dropped` |

### 3.1 为什么 Task 取“会话”而不是“todo 项”

- 用户的问题是“某个任务在哪个 agent 工具”，对应的是 agent 侧边栏里的会话，不是某一条 checklist。
- 一个会话里可能有多个 todo；把 todo 抬成全局任务会产生大量碎片，且丢失“回到哪个会话继续”的关键信息。
- Todo 仍有价值：在任务详情里显示进度（`3/7`），并支持按“未完成 todo”过滤。

### 3.2 身份与去重

- 主键：`key = "<source>:<source_id>"`，例如 `qoder:8e4c2d44-...`、`claude:f95dea8e-...`。
- `source_id` 使用 agent 自己的稳定会话 id；标题变化、文件移动、项目改名都不改变 key。
- 同一会话在源里出现多次（Codex `sessions/` 与 `archived_sessions/` 副本、OpenCode 多 DB）按 key 覆盖，不重复。
- 子会话：`parent_key` 指向父任务；默认不进入主列表，但可被置顶或搜索到。
- 不自动把不同源的任务合并。跨 agent 同一件事由用户建 group（P1）。

---

## 4. 总体架构（Tauri v2）

```
┌──────────────────────── macOS App（Tauri v2） ────────────────────────┐
│  Svelte 5 + TS + Vite（系统 WKWebView）                               │
│  ┌────────────┐  ┌──────────────────────┐  ┌──────────────────────┐  │
│  │ 侧边栏      │  │ 任务列表              │  │ Inspector 详情        │  │
│  │ 智能列表    │  │ 全局置顶 / workspace  │  │ todos / 元数据 / 只读  │  │
│  │ Workspaces  │  │ 分组 / 筛选 / 搜索     │  │ resume 命令           │  │
│  └────────────┘  └──────────────────────┘  └──────────────────────┘  │
│         │ Tauri IPC（invoke commands / events）                       │
│         ▼                                                             │
│  src-tauri（Rust）                                                    │
│  · commands.rs：bootstrap / refresh / set_pin / doctor                │
│  · state.rs：AppState（cache/overlay 连接、扫描锁、事件通道）          │
│         │                                                             │
│         ▼                                                             │
│  perch-core（Rust lib，可单测）                                      │
│  · collectors：qoder / claude / codex / opencode+kilo / trae          │
│  · probe：SQLite data_version/指纹、JSONL manifest/cursor、hooks       │
│  · store：快照 upsert / source_missing / sync_state                   │
│  · overlay：pin（MVP）/ status/tag/note/hide/group（后续）             │
│  · query：workspace 分组 / 7 天窗口 / 过滤 / 排序 / bootstrap payload │
│         │                                                             │
│         ▼                                                             │
│  ~/Library/Application Support/<bundle-id>/cache.db（快照，可重建）    │
│  ~/Library/Application Support/<bundle-id>/overlay.db（用户数据）      │
└───────────────────────────────────────────────────────────────────────┘
                               │ 只读
                               ▼
┌──────────────────────── 源（全部只读） ────────────────────────────────┐
│ Qoder main.sqlite   Claude projects/*.jsonl + tasks/*.json             │
│ Codex sessions + session_index + goals_1.sqlite                       │
│ OpenCode/Kilo *.db（session / todo）   Trae hooks（可选）              │
└───────────────────────────────────────────────────────────────────────┘
```

### 4.1 分层职责

| 层 | 模块 | 职责 | 不负责 |
|---|---|---|---|
| 采集 | `perch-core::collectors` | 定位源、只读解析、归一为 TaskSnapshot/Todo、错误隔离 | 不写源、不做 UI |
| 运行时探测 | `perch-core::probe` | SQLite `data_version`/指纹、JSONL manifest/`file_cursor`、hooks 事件；返回变更分类 | 不扫描全源、不写源、不做 UI |
| 存储 | `perch-core::store` | 快照 schema、幂等 upsert、`source_missing`、同步状态 | 不写覆盖层 |
| 覆盖层 | `perch-core::overlay` | MVP 只做 pin 的读写与校验；status/tag/note/hide/group 后续加入 | 不碰快照与源 |
| 查询 | `perch-core::query` | workspace 分组、7 天窗口、筛选、排序、前缀解析、payload | 不访问 UI |
| IPC | `src-tauri::commands` | 暴露 bootstrap/refresh/set_pin/doctor | 不解析源 |
| 动作（P2） | `src-tauri::actions` + `perch-core::actions` | 打开/归档/重命名/发消息等受控回写；preview → 确认 → 执行 → 校验 → 审计 | 不直接写 agent DB |
| UI | `src/`（Svelte） | 三栏总览、筛选、详情、快捷键、主题 | 不直接读文件/DB |
| CLI（P2） | `crates/perch-cli` | 复用 core 的 list/pin/doctor/export | 不是主形态 |

### 4.2 与 usage-ledger 的结构对照

| 维度 | usage-ledger | Perch |
|---|---|---|
| 形态 | Python CLI + 单文件 HTML | Rust + Tauri macOS app |
| 采集方式 | 拉取式、只读 | 相同 |
| 数据库地位 | 归档 + 索引，源是真相 | 相同 |
| 只读保证 | SQLite `mode=ro` + `query_only` | 相同，额外用测试校验源文件 mtime/hash 不变 |
| 常驻 | 无 daemon | app 关闭即退出；启动/聚焦/手动刷新/窗口可见时 probe |
| 用户写入 | 无（只有源归档） | 整理数据写独立 `overlay.db`；回写 agent 默认关闭（§8.6） |

### 4.3 为什么选 Tauri，而不是 Electron / 原生 SwiftUI / egui

| 方案 | 优点 | 代价 | 结论 |
|---|---|---|---|
| **Tauri v2 + Rust + WebView** | Rust 写采集/存储；macOS 原生窗口；体积小；Web 技术做列表 UI 快；开源生态成熟 | 需要同时维护 Rust + 前端；WKWebView 兼容性需测试 | **推荐** |
| Electron | 生态最熟、UI 最稳 | 体积大、内存高、与“极轻量”冲突 | 不选 |
| SwiftUI 原生 | 最 macOS、性能好 | 采集/解析/跨源适配要重写 Swift；迭代慢 | 不选 |
| egui / Dioxus / Slint | 纯 Rust | 列表/详情/表单的 UI 生态与 macOS 原生感弱；样式打磨成本高 | 不选 |

> Tauri 的 WebView 在 macOS 上是系统 WKWebView，不打包 Chromium；产物体积通常远小于 Electron。

---

## 5. 数据模型

数据库拆成两个文件，均在 `app_data_dir()` 下（macOS 通常为
`~/Library/Application Support/<bundle-id>/`）：

| 文件 | 内容 | 可删 |
|---|---|---|
| `cache.db` | 任务快照、todo、同步/probe 状态、扫描记录、JSONL `file_cursor`；可重建 | 是，删除后全量重扫 |
| `overlay.db` | pin（MVP）；状态、标签、备注、隐藏、分组（后续）；用户数据 | 否，需备份 |

测试/调试可用 `PERCH_CACHE_DB` / `PERCH_OVERLAY_DB` / `PERCH_SOURCE_HOME` 覆盖。
两个文件各自 WAL + `busy_timeout=5000`。查询在 Rust 侧按 `key` 合并，不做跨库事务，也不设跨库外键。

### 5.1 快照表（`cache.db`，可重建）

```sql
CREATE TABLE task (
  key             TEXT PRIMARY KEY,          -- <source>:<id>
  source          TEXT NOT NULL,
  source_id       TEXT NOT NULL,
  parent_key      TEXT,                      -- 子会话/侧聊
  title           TEXT NOT NULL,             -- 归一化标题，≤240 字符
  title_source    TEXT NOT NULL,             -- custom|ai|source|provisional|fallback
  project_path    TEXT,                      -- 本地绝对路径，用于 resume/打开
  project_name    TEXT,                      -- basename，用于筛选/显示
  created_at      REAL,                      -- epoch 秒
  updated_at      REAL,                      -- 源侧最近活动
  status_source   TEXT,                      -- running|active|paused|blocked|limited|complete|archived|idle
  status_raw      TEXT,                      -- 源原始状态，便于排查
  source_archived INTEGER NOT NULL DEFAULT 0,
  source_unread   INTEGER NOT NULL DEFAULT 0,
  source_pinned   INTEGER NOT NULL DEFAULT 0,-- 源自己的置顶，只读展示
  source_missing  INTEGER NOT NULL DEFAULT 0,-- 完整扫描后源里已不存在
  model           TEXT,
  git_branch      TEXT,
  agent_kind      TEXT,                      -- desktop|cli|automation|unknown
  message_count   INTEGER,
  todo_total      INTEGER,
  todo_done       INTEGER,
  deeplink        TEXT,                      -- 仅验证过的深链；可为空
  resume_cmd      TEXT,                      -- 仅供展示；复制后续加入；不自动执行
  fingerprint     TEXT,                      -- 元数据指纹，减少无意义写入
  seen_at         REAL NOT NULL,
  last_seen_run   INTEGER,                   -- 最近一次看到它的 scan_run
  UNIQUE(source, source_id)
);
CREATE INDEX idx_task_updated  ON task(updated_at DESC);
CREATE INDEX idx_task_source   ON task(source);
CREATE INDEX idx_task_project  ON task(project_name);
CREATE INDEX idx_task_workspace ON task(project_path, updated_at DESC);
CREATE INDEX idx_task_parent   ON task(parent_key);
CREATE INDEX idx_task_status   ON task(status_source);
CREATE INDEX idx_task_missing  ON task(source_missing);

CREATE TABLE task_todo (
  task_key   TEXT NOT NULL,
  item_id    TEXT NOT NULL,
  subject    TEXT NOT NULL,                  -- Claude: subject；OpenCode/Kilo: content
  status     TEXT NOT NULL,                  -- pending|in_progress|completed|cancelled|unknown
  position   INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (task_key, item_id)
);
CREATE INDEX idx_todo_task ON task_todo(task_key, position);

CREATE TABLE sync_state (
  source            TEXT PRIMARY KEY,
  last_checked_at   REAL,                    -- 最近一次 probe（可能未扫描）
  last_change_at    REAL,                    -- 最近一次检测到变更
  last_scan_at      REAL,                    -- 最近一次实际扫描
  last_ok_at        REAL,
  last_full_scan_at REAL,
  last_mode         TEXT,                    -- full|incremental|skipped
  last_trigger      TEXT,                    -- launch|focus|live|manual|periodic
  probe_kind        TEXT,                    -- sqlite|file_tree|hooks
  probe_state       TEXT,                    -- JSON：data_version/成员指纹/manifest hash 等
  status            TEXT NOT NULL,           -- ok|partial|unavailable|error|stale
  files_seen        INTEGER,
  task_count        INTEGER,
  signature         TEXT,
  schema_version    TEXT,
  error             TEXT
);

CREATE TABLE file_cursor (
  source       TEXT NOT NULL,
  path         TEXT NOT NULL,
  inode        INTEGER,
  size         INTEGER NOT NULL DEFAULT 0,
  mtime        REAL,
  offset       INTEGER NOT NULL DEFAULT 0,   -- JSONL 已读字节
  line_count   INTEGER NOT NULL DEFAULT 0,
  last_read_at REAL,
  PRIMARY KEY (source, path)
);

CREATE TABLE scan_run (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  source      TEXT NOT NULL,
  mode        TEXT NOT NULL,                 -- full|incremental
  scope       TEXT,                          -- source|session
  trigger     TEXT,                          -- launch|focus|live|manual|periodic
  started_at  REAL NOT NULL,
  finished_at REAL,
  status      TEXT NOT NULL,                 -- running|ok|partial|error
  task_count  INTEGER NOT NULL DEFAULT 0,
  missing_count INTEGER NOT NULL DEFAULT 0,
  error       TEXT
);
CREATE INDEX idx_scan_run_source ON scan_run(source, started_at DESC);

CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);
```

**workspace 分组规则**：以 `project_path` 的规范化绝对路径（展开 `~`、去掉末尾 `/`）作为 workspace key；
缺失时退回 `project_name`，两者都缺失归入“未关联项目”（key `__no_project__`）。MVP 不做 git root 推断，按源上报的
workspace/文件夹原样分组；同一路径下不同 agent 的任务进入同一组。自动把子目录合并到 git 仓库根可作为 P1 增强。

Qoder 对未打开项目的会话会把 `workspace_id` 留空、cwd 填成 `~/Documents/Qoder/<日期>/<会话前缀>`
这样的沙箱目录；采集器以 `workspace_id` 判定项目归属，此类会话不产出 `project_path`/`project_name`。
Codex Desktop 同样会生成 `~/Documents/Codex/<日期>/<名称>` 沙箱；OpenCode 使用空的
`~/Documents/Default Project` 作为占位目录。三者均不视为项目，统一进入“未关联项目”分组。

### 5.2 覆盖层表（`overlay.db`，用户数据，扫描永不写）

M2 只创建 pin 所需字段与索引：

```sql
CREATE TABLE task_overlay (
  key            TEXT PRIMARY KEY,
  pinned         INTEGER NOT NULL DEFAULT 0,
  pin_order      INTEGER,                    -- P1 拖拽排序使用
  pinned_at      REAL,
  updated_at     REAL NOT NULL
);
CREATE INDEX idx_overlay_pin ON task_overlay(pinned, pin_order, pinned_at);
```

后续版本（P1/P2）通过迁移加入人工状态、标题覆盖、备注、隐藏和分组；**M2 不创建这些列/表**：

```sql
ALTER TABLE task_overlay ADD COLUMN status         TEXT;
ALTER TABLE task_overlay ADD COLUMN title_override TEXT;
ALTER TABLE task_overlay ADD COLUMN note           TEXT;
ALTER TABLE task_overlay ADD COLUMN hidden         INTEGER NOT NULL DEFAULT 0;

CREATE TABLE task_tag (
  key TEXT NOT NULL,
  tag TEXT NOT NULL,                         -- 小写、≤32 字符
  PRIMARY KEY (key, tag)
);

CREATE TABLE task_group (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  name       TEXT NOT NULL UNIQUE,
  pinned     INTEGER NOT NULL DEFAULT 0,
  created_at REAL NOT NULL,
  updated_at REAL NOT NULL
);

CREATE TABLE task_group_member (
  group_id INTEGER NOT NULL,
  task_key TEXT NOT NULL,
  added_at REAL NOT NULL,
  PRIMARY KEY (group_id, task_key)
);
```

### 5.3 语义约束

除置顶、源删除、子任务、自动化、时间、workspace 分组、7 天窗口和重建快照外，其余语义随对应能力在后续版本启用。

| 约束 | 规则 |
|---|---|
| 置顶（M2） | 全局 pin 与源置顶相互独立；pin 只写 overlay，不改源 |
| 隐藏（后续） | 与置顶互斥：`pin` 时 `hidden=0`，`hide` 时 `pinned=0`；UI 给提示 |
| 人工状态（后续） | `NULL` 表示“不覆盖”；`todo/doing/done/dropped` 只影响展示与筛选 |
| 标题覆盖（后续） | `title_override` 优先于快照标题；源标题更新不覆盖人工标题 |
| 源删除 | `source_missing=1`，不删快照、不删 overlay；UI 显示“源已删除”，pin 仍保留 |
| 子任务 | `parent_key` 非空；默认折叠，父任务详情显示子任务数与列表 |
| 自动化 | Codex `thread_source=automation` 记为 `agent_kind=automation`，默认折叠 |
| 时间 | 全部存 epoch 秒；显示用本地时区；不按 UTC 切日 |
| workspace 分组 | 按规范化 `project_path` 分组；缺失时依次退回 `project_name`、“未关联项目” |
| 7 天窗口 | 以 `updated_at` 为准；为空时用 `seen_at` 并标注“时间未知”；滚动 7×24 小时 |
| 重建快照 | 删除 `cache.db` 后全量重扫；`overlay.db` 不动 |

### 5.4 覆盖层为何不写回源

- Qoder 自己有 `chat_session_sidebar_placements.pinned`，Claude/Codex/OpenCode/Kilo 也有各自的归档/状态；
  写回会与 agent 的 UI 语义冲突，且升级 schema 时容易被覆盖。
- Perch 的 pin 是“跨 agent 的全局置顶”，与源置顶不是同一个概念；UI 同时显示两者：
  - `★ 全局`：Perch overlay
  - `源置顶`：只读徽标（例如 Qoder sidebar pinned）
- 覆盖层是独立文件，可单独备份；即使 `cache.db` 被删除，pin（以及后续的备注等）仍在。导出功能后续加入。

### 5.5 为什么拆两个库

- **快照可重建，用户数据不可重建**：`cache.db` 是从源派生的缓存，坏了/删了都能重扫；`overlay.db` 是用户手工积累的 pin（以及后续的备注、分组），不能因为一次 schema 迁移或缓存损坏而丢。
- **边界可验证**：扫描代码只持有 `cache.db` 连接；overlay 写入只持有 `overlay.db` 连接。测试可以直接断言“扫描进程没有打开 overlay.db”。
- **备份简单**：备份 pin 只需要复制一个 `overlay.db`；不需要从大表里挑数据。
- **代价可控**：任务量约 300 条，Rust 侧按 key 合并两个查询结果即可；不需要跨库 JOIN，也不需要跨库事务。

---

## 6. 采集规格（逐源）

> 实现为 `perch-core/src/collectors/` 下的独立模块；每个模块只返回 `SourceResult`，
> 不接触 app 状态、不写 DB。所有 SQLite 连接用 `rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY`
> + `PRAGMA query_only=ON`；JSONL 只读。

### 6.1 Qoder（`qoder`）

- 路径：`~/Library/Application Support/com.qoder.app.stable/main.sqlite`（`PERCH_QODER_DB` 可覆盖）。
- 任务：`chat_sessions`，过滤 `deleted_at IS NULL`；`session_kind='side_chat'` 归为子任务（`parent_key` = `owner_session_id`）。
- 标题优先级：`title`（非空）→ `extra_json.provisionalTitle` → `新会话`。
- `title_source`：`extra_json.titleSource`（custom/ai/fallback）存在时照抄；使用 provisional 时标 `provisional`。
- 项目：`cwd`；`created_at/updated_at` 是 epoch ms，转为秒。
- 状态：`archived` → `archived`；`unread` → `source_unread=1`；`chat_session_turn_states.status` 最新值
  映射 `running`（未知值保留 `status_raw`）。当前版本 turn_states 为空，改用兜底规则：
  最后一条带 role 的消息是 `user`（尚无回复）且会话 `updated_at` 在 24 小时内 → `running`
  （`status_raw=waiting_reply`）；否则 `idle`。
- 源置顶：`chat_session_sidebar_placements.pinned` 只读；不写。
- 子项：无 todo；`message_count` 用 `chat_session_messages GROUP BY session_id`（本机 4,111 行，0.08s）。
- 深链：`qoder://` / `qoder-app://` 已注册，但**不支持会话级 URL**：`qoder://chat/<id>` 在 Qoder `main.log`
  报 `DEEP_LINK_ROUTE_UNSUPPORTED`，只会激活 App。Perch “打开 Qoder”因此只能切到 Qoder 前台，不能定位会话。
- 性能：183 个会话 + 4,111 条消息；只读连接。

### 6.2 Claude Code（`claude`）

- 路径：`~/.claude/projects/**/*.jsonl`，只取**顶层** `<project>/<sessionId>.jsonl`；`/subagents/` 下的文件作为子会话，
  默认不入主列表（可统计数量）。
- 标题：文件内最后一条 `custom-title.customTitle` → 最后一条 `ai-title.aiTitle` → fallback。
  本机 14/14 顶层会话都有标题。
- 项目：记录里的 `cwd`；`project_name` 取 basename。
- 时间：首/末记录 `timestamp`；文件 mtime 兜底。
- 模型/分支：最后一条 assistant 的 `message.model`；`gitBranch` 若存在。
- 状态：Claude Code 不提供会话完成/归档状态，统一 `idle`（UI 显示“无显式状态”），不臆造 `done`。
- 子项：`~/.claude/tasks/<sessionId>/*.json`，只读 `subject` / `status` / `blocks` / `blockedBy`；
  **不读 `description`**。状态映射 pending/in_progress/completed。
- resume：`cd <cwd> && claude --resume <sessionId>`。
- 性能：顶层 JSONL 约 25MB；逐行只解析所需字段。

### 6.3 Codex（`codex`）

- 路径：
  - `~/.codex/sessions/**/*.jsonl`（活跃）
  - `~/.codex/archived_sessions/**/*.jsonl`（归档，`source_archived=1`）
  - `~/.codex/session_index.jsonl`（id → thread_name、updated_at）
  - `~/.codex/goals_1.sqlite`（`thread_goals`：objective、status、tokens_used）
- 任务：每个 rollout 的**第一行** `session_meta` 提供 `id`、`timestamp`、`cwd`、`originator`、`thread_source`；
  另读尾部 256KB 取最新活动时间，不扫完整 rollout（本机 69 文件约 0.09s）。`.jsonl.zst` 跳过并在 Diagnostics 计数。
- 标题：`session_index.thread_name` → `thread_goals.objective`（截断 240）→ fallback。
  本机 session_index 38 条、rollout 69 个，缺失标题属正常，Diagnostics 显示覆盖率。
- 状态：归档目录 → `archived`；否则读 rollout 尾部 256KB，最后一条 `event_msg` 为 `task_started`
  且文件 24 小时内修改过 → `running`（`status_raw=task_started`），`task_complete`/`turn_aborted`
  结束运行；无运行证据时按 goal status 映射：
  `active→active`、`paused→paused`、`blocked→blocked`、`usage_limited/budget_limited→limited`、`complete→complete`；
  无 goal 时 `idle`。**本机探测时 `thread_goals` 为 0 行**，有 goal 数据后自动生效。
- `agent_kind`：`originator=Codex Desktop` → `desktop`；`codex-tui/codex_exec` → `cli`；`thread_source=automation` → `automation`。
- 子项：goal 只有 objective/status，不生成 todo；详情显示 goal 状态与 `tokens_used`（标注“goal 计数”）。
- resume：`cd <cwd> && codex resume <sessionId>`（已用 `codex resume --help` 验证）。
- 去重：同 id 的 `sessions/` 与 `archived_sessions/` 副本只保留一条；归档副本标记 `source_archived=1`。

### 6.4 OpenCode（`opencode`）

- 路径：`~/.local/share/opencode/opencode.db`（另尝试 `opencode-stable.db`）。
- 任务：优先 `session_v2` 表（新版本 OpenCode 已迁移，旧 `session` 表自 2026-10-21 起不再更新，
  仅在无 `session_v2` 时回退）；顶层 `parent_id IS NULL`，子会话保留 `parent_key`。
  `title`、`directory`、`time_created/time_updated`（ms）、`time_archived`、`model`、`agent`。
- 状态：`time_archived` 非空 → `archived`；否则 `time_busy > time_idle` 且 `time_busy` 非空 → `running`
  （`status_raw=busy`）；其余 `idle`。
- 子项：`todo` 表（session_id、content、status、priority、position）。v2 时代 OpenCode 不再写 `todo`
  （待办内嵌在 `session_message` 数据中），新会话暂无 todo 进度；提取方案后续验证。
- resume：`cd <directory> && opencode --session <sessionId>`（已用 `opencode --help` 验证 `--session/-s`）。
- 本机：`session_v2` 117 个会话（旧 `session` 107 个，已冻结）。

### 6.5 Kilo（`kilo`）

- 路径：`~/.local/share/kilo/kilo.db`；schema 与旧版 OpenCode `session` 表同构（无 `session_v2`、
  无 `time_busy/time_idle`，采集器自动回退到 `session` 表）。
- 任务/子项/状态：同 OpenCode（旧表路径），状态为 `archived` 或 `idle`。
- `kilo_board` 表提供 `objective`，可作为详情里的长目标；不作为标题优先项（标题已有）。
- resume：`kilo` CLI 未提供已确认的 `--session` 恢复参数（`kilo session` 只有 list/delete）。
  MVP 显示 `cd <directory> && kilo` + session id；复制动作后续加入，实现时再确认是否有 headless resume 方式，
  **不猜测命令**。
- 本机：9 个会话 / 6 个顶层 / 2 个归档。

### 6.6 Trae CN（`trae-cn`，P2 可选）

- 本机 `state.vscdb` 只有模型配置；`chat.ChatSessionStore.index` 为 `{"version":1,"entries":{}}`；
  无 hooks 配置。**结论：当前没有可用的本地任务列表。**
- 可选方案：复用 usage-ledger 已安装的 Trae hooks 事件
  （`~/.local/share/usage-ledger/trae-cn/events-*.jsonl`，字段 `session/project/ts/model`），
  聚合成会话任务。只有安装 hook 之后的会话可见，且没有标题/归档/状态；标题用 `Trae CN 会话 · <project> · <date>`，
  后续可用人工重命名能力覆盖。未安装时 source 状态为 `unavailable`，Diagnostics 说明。
- MVP 不解密 Trae 的加密库、不调用私有 API；若后续要覆盖 Trae，先做只读可行性验证（加密方式、schema 稳定性、升级频率、失败隔离），再决定是否实现。
  该能力默认关闭，启用时在 Diagnostics 明确标注数据来源与风险（§11.2）。

### 6.7 ChatGPT 桌面 / WorkBuddy

- ChatGPT 桌面普通聊天无本地任务数据；其 Codex 会话写入 `~/.codex`，由 Codex 采集器覆盖。
- WorkBuddy 本机未安装：保留 adapter 位置，P2 实现；不在本设计中猜测其 schema。

### 6.8 子会话与自动化

| 源 | 子会话识别 | 默认处理 |
|---|---|---|
| Claude | 文件位于 `subagents/` | 统计数量，不列主列表；父任务详情可展开 |
| OpenCode/Kilo | `parent_id` 非空 | 存 `parent_key`，默认折叠 |
| Qoder | `session_kind='side_chat'` | 存 `parent_key`，默认折叠 |
| Codex | `thread_source=automation` | 记为 `automation`，默认折叠；`user` 正常显示 |

置顶或搜索命中的子会话仍可显示（带“子会话”徽标），避免“pin 了却看不到”。

---

## 7. 覆盖层与任务操作

### 7.1 操作清单

| 操作 | 存储 | UI | 版本 |
|---|---|---|---|
| 全局置顶 / 取消 | `task_overlay.pinned` | ★ / 右键 / `⌘⇧P` | **MVP** |
| 置顶排序 | `pin_order` | 拖拽 | 后续 |
| 人工状态 | `task_overlay.status` | 状态下拉 | 后续 |
| 重命名 | `title_override` | Inspector 内编辑 | 后续 |
| 标签 | `task_tag` | 添加/移除 | 后续 |
| 备注 | `task_overlay.note` | Inspector textarea | 后续 |
| 隐藏 | `task_overlay.hidden` | 隐藏/恢复 | 后续 |
| 跨 agent 分组 | `task_group*` | 多选 → 建组 | 后续 |
| 复制 resume 命令 | 无 | 复制按钮 | 后续 |
| 打开项目目录 | 无 | 按钮 | 后续 |
| 导出快照 | 无 | 菜单 File → Export | 后续 |

MVP（M1–M3）：**只做全局置顶/取消置顶**。人工状态、标签、备注、隐藏、重命名、复制 resume、
打开项目目录、置顶排序、跨 agent 分组、静态导出全部后置，不在 M1–M3 实现。
P2：用量关联、Trae hooks、手工任务、伴生 CLI。

### 7.2 状态展示规则

```
MVP：有效状态 = status_source（源状态） > "idle"
后续：有效状态 = overlay.status（若设置） > status_source > "idle"
```

- MVP 只显示源状态；人工状态是后续能力，加入后与源状态并排显示，不把人工 `done` 伪装成源已完成。
- `needs attention` 筛选：`source_unread=1` 或 `status_source in (blocked, limited)`；
  人工 `doing` 且长期未活动的提醒随后续状态能力加入。
- `stale` 角标：`updated_at` 超过 14 天且未归档；只提示，不改状态。
- 源状态未知时显示“无显式状态/空闲”，不显示“已完成”。

### 7.3 全局置顶区

- 位置：任务列表上方独立 “Global pinned” 区；侧边栏另有 “Pinned” 智能列表。
- 不受 source/workspace/源状态筛选和 7 天窗口影响；只受全局搜索影响（可选：搜索时仍显示匹配的 pin）。
- 每张 pin 卡显示 source、标题、workspace/项目、最近活动、源状态/未读；点击进入 Inspector。
- 源已删除的 pin 仍显示（`源已删除` 徽标），提醒用户备份或人工收尾。

---

## 8. 数据一致性与新鲜度

> 目标：Perch 的快照与各 agent 的本地数据保持一致，同时不把刷新做成复杂的状态机。
> 源是唯一事实来源；`cache.db` 是可重建的缓存；`overlay.db` 是用户数据。
>
> 两类数据分开看：**源元数据**（标题/项目/归档/状态/todo）是只读镜像，按 §8 同步；
> **Perch overlay**（MVP 的全局 pin；后续的人工状态/标签/备注）是 Perch 自己的数据，不要求与源一致，也不自动回写。

### 8.1 一致性方案对比

| 方案 | 做法 | 优点 | 代价 | 结论 |
|---|---|---|---|---|
| **A. 全量重扫（stateless pull）** | 启动/手动刷新时重新读取全部源，事务内重建快照；完整成功后才标记缺失 | 最简单、最可靠；天然处理删除/改名/迁移；无游标状态 | 数据量大时慢；不能实时 | **正确性底座** |
| **B. 签名/水位增量** | 每源算签名（mtime/size/inode、逻辑指纹、SQLite `data_version`）；未变跳过，变了只扫该源 | 启动快、I/O 少；可扩展 | mtime 不可靠；会漏变更；删除仍需全量 | **加速层** |
| **C. 文件监听（FSEvents/notify）** | 监听源目录/WAL，debounce 后触发刷新 | 近实时；体验好 | app 必须运行；睡眠/唤醒会丢事件；路径多 | P1 可选 |
| **D. Agent hooks / 官方事件** | 安装 hooks 或订阅官方 API/MCP，按事件刷新 | 语义化、实时；能拿到 start/stop | 需改 agent 配置；覆盖不一；升级易断 | P2 可选 |
| **E. 直接 live query** | 不落快照，每次 UI 加载直接查源 | 永远最新；无重复数据 | 多 schema、慢、锁、源不可用、无法跨源搜索/关联 overlay | 不推荐为主路径 |
| **F. CDC/触发器/写源** | 在源库装触发器或用 session extension | 变更精确 | 需要写源；升级/锁/schema 风险高 | 不采用 |
| **G. 可见窗口全源 probe** | 窗口可见时每 3–5 秒对每个 source 做 probe（SQLite `data_version`/JSONL manifest+hooks offset）；变化才扫描 | 秒级延迟；无侵入；跨源统一 | 需只读长连接/文件句柄；隐藏时暂停；不是严格实时 | **MVP 叠加在 A+B 上** |

### 8.2 推荐：A + B + G 的组合

**快照 + 全量对账 + 签名跳过 + 可见窗口全源 probe**：

> 一致性目标是**有界延迟**：窗口可见时秒级发现新任务；app 关闭/隐藏期间不更新；打开/聚焦/手动刷新时追平；
> 源删除在 full 对账时体现。A 是正确性底座，B 是启动/聚焦的加速层，G 只缩短“Perch 开着”时的感知延迟；
> 三者都保留周期全量兜底。

| 场景 | 行为 |
|---|---|
| app 启动 | 立即读 `cache.db` 显示；后台逐源 probe/算签名：未变 → 标记 `skipped`；变了/无签名 → 全量扫描该源；启动后空闲时再做一次全量对账（兜底，可配置） |
| 窗口重新聚焦 | 距上次检查 >10s 时先 probe；变了才扫描；未变只更新“已检查”时间 |
| 窗口可见但未聚焦（如第二屏） | 每 3–5 秒对全部 source 做 probe；SQLite 用 `data_version`，JSONL 用 manifest/offset，hooks 用事件文件；变化才扫描；**不以 app 是否 frontmost 为条件** |
| 窗口隐藏/最小化 | 暂停 probe；恢复可见或聚焦时立即 probe 全部源 |
| 系统唤醒 | 立即 probe；距上次 full >6h 则后台全量对账 |
| 手动刷新 `⌘R` | 强制全量对账（跳过 probe），用于怀疑不一致时 |
| 连续运行 | 每 6 小时后台全量对账一次（兜底，可配置） |
| 退出/关闭窗口 | 进程退出；无后台 daemon |

**为什么不全用增量**：

- 本机 5 个源、300+ 任务，全量 <1.5s，全量的成本可接受。
- mtime 已实测不可靠（Claude 文件 mtime 可晚 13 天）；WAL 也可能让 DB mtime 不变。
- **只有全量扫描能可靠发现“源里删除了什么”**；增量只能更新“还存在的”，不能安全推断删除。
- 不做 CDC/触发器，避免写源。

**为什么还需要 probe/签名**：日常启动/聚焦不必重复解析 25MB JSONL；probe 只决定“要不要扫”，不决定“删不删”。

**MVP 边界**：先实现到“源级 probe（SQLite 指纹 / JSONL offset 增量）+ 可见窗口全源 probe（G）”；
FSEvents、hooks 留 P1/P2，不阻塞 M1–M3。

**可观测性**：Diagnostics 每源显示 `last_checked_at` / `last_change_at` / `last_scan_at` / `last_full_scan_at` /
`last_mode` / `last_trigger` / `probe_kind` / `probe_state` / `file_cursor` / `task_count` / `missing_count` /
`schema_version` / `error`；“最近检查”和“最近全量”分开显示；提供 “Rebuild cache” 按钮。

#### 8.2.1 G：任意 source 的运行时感知

> 场景：Perch 开在第二块屏、没有焦点；用户在任意 agent（Qoder / Claude Code / Codex / OpenCode / Kilo）
> 新建任务或产生新信息。G 的目标是在不写源、不常驻 daemon 的前提下，把“源变更 → Perch 可见”的延迟
> 压到秒级；所有已接入 source 共用同一套 probe 框架。

**统一模型：Probe → Change → Scan**

1. **Probe（探测）**：每个 source 实现只读 probe，返回 `unchanged | structural | activity | reset | error`。
2. **Change（分类）**：
   - `structural`：新任务、删除、改名、归档、状态、todo、源置顶变化 → 触发扫描；区分
     `scope=source`（成员/删除，全源扫描）与 `scope=session`（单会话标题/todo/状态，只重扫该会话）；
   - `activity`：只有消息追加/更新时间变化 → 只增量更新 `updated_at`/计数，不重扫任务列表；
   - `reset`：DB 被替换、JSONL 被截断/轮转、cursor 失效 → 重置 cursor 并 full 扫该源；
   - `error`：保留快照，标 `stale`，指数退避重试。
3. **Scan（落地）**：仍走 §8.4 的事务、upsert、missing 判定；G 只改变“何时检查”，不改变 A+B 的正确性规则。

**触发方式**：

| 触发 | 延迟 | 说明 |
|---|---|---|
| 可见窗口 probe（MVP） | 3–5s | 统一调度器；窗口 `visible && !minimized` 才运行 |
| FSEvents / `notify`（P1） | <1s | 主要给 JSONL 源提前触发；SQLite 源 `data_version` 已足够。只是提前调用同一个 probe |
| Hooks（P2） | 即时 | 只对支持 hooks 的源；opt-in，写 Perch 事件文件 |
| 聚焦/手动/6h（兜底） | 0–6h | 保留现有 A+B 节奏 |

**各源 probe 矩阵**：

| 源 | probe | 变更信号 | 新任务 | 新信息（MVP 元数据） |
|---|---|---|---|---|
| Qoder | SQLite 只读长连接 | `PRAGMA data_version`；回退 `main.sqlite-wal` mtime/size | 新 `chat_sessions.session_id` | 标题/项目/模型/分支/归档/未读/源置顶/消息计数 |
| Claude Code | JSONL 文件树 | 顶层 `*.jsonl`、`subagents/*.jsonl`、`tasks/**/*.json` 的 manifest hash（path/inode/size/mtime） | 新顶层 JSONL | 追加行（标题/时间/模型/分支）、todo 文件变化 |
| Codex | JSONL + SQLite | rollout/archive 清单、`session_index.jsonl` size/offset、`goals_1.sqlite` `data_version` | 新 rollout | index 新行（标题/更新时间）、goal 状态；归档移动=归档，不是删除 |
| OpenCode | SQLite 只读长连接 | `data_version`；回退 `-wal` mtime/size | 新 `session_v2.id` | 标题/项目/模型/归档/busy 状态/活动 |
| Kilo | SQLite 只读长连接 | 同 OpenCode | 新 `session.id` | 标题/项目/模型/归档/todo/活动 |
| Trae CN | hooks 事件文件 | 事件文件 offset | 仅安装 hook 后的会话 | 事件聚合；无 hook 时 `unavailable` |
| ChatGPT 桌面 / WorkBuddy | 无（ChatGPT 普通聊天）；Codex 会话见 Codex | — | — | 普通聊天不单独支持 / WorkBuddy 未安装 |

路径以 [DATA-PROBE.md](DATA-PROBE.md) 为准：Qoder `main.sqlite`；Claude `~/.claude/projects` +
`~/.claude/tasks`；Codex `~/.codex/sessions` / `archived_sessions` / `session_index.jsonl` /
`goals_1.sqlite`；OpenCode `~/.local/share/opencode/opencode.db`；Kilo `~/.local/share/kilo/kilo.db`；
Trae hooks 事件目录。

**增量读取**：

- SQLite 源：成员指纹变化 → 该源 full scan（`scope=source, mode=full, trigger=live`）；
  todo 指纹变化 → 只重扫受影响会话的 todo（`scope=session`）；仅活动变化 →
  `updated_at`/计数增量更新（`mode=incremental`，不标记 missing）。
- JSONL 源：新文件 → 只扫该会话（`scope=session`）；文件增长 → 按 `file_cursor` 从 offset 读新字节；
  文件删除/归档移动 → `scope=source` full；`tasks/**/*.json` 变化 → 只重扫该会话 todo。
- 新文件先读首行/标题行；文件缩短、inode 变化或 offset 越界 → 丢弃该文件 cursor 并 full 扫该会话。
- 半行容错：最后一行未写完时只读到最后一个换行，剩余字节留给下次；不把半行判为损坏。
- cursor 与 task/todo upsert 在**同一个 `cache.db` 事务**内提交；崩溃后 cursor 不前进，下次重读，避免丢行。

**运行时细节**：

- 窗口 `visible && !minimized` 才 probe；**不以 app 是否聚焦为条件**；隐藏/最小化暂停；恢复可见、聚焦、系统唤醒时立即 probe 全部源。
- 同一 source 同时只跑一个扫描；扫描期间再有变更 → 置 dirty，结束后合并再跑一次；每源最小扫描间隔 2–3 秒，避免消息流打爆。
- 多个 source 可并行 probe；扫描按源串行；探针失败按 5s → 10s → 30s → 60s 退避，成功后恢复。
- 电池模式降频到 10 秒；手动 `⌘R` 仍强制 full。
- probe 目标：SQLite `data_version` <1ms；JSONL manifest（stat）<10ms；没有变化不读文件内容。
- 只读连接/文件句柄；不写源；不常驻 daemon。
- UI 收到 `scan-finished` 的 `changed_keys` 后增量更新，不整表刷新。
- MVP 的“新信息”是元数据（标题/项目/时间/状态/todo/计数），不含正文；正文能力后续 opt-in。

**边界与风险**：

- 所有源都受限于“源何时落库”：空会话/未提交草稿如果没有写入本地数据，Perch 不可见。
  各源的落库时机 M1 逐个实测；要覆盖“打开 tab 就出现”，只能依赖 hooks（P2）。
- JSONL 源假设主要是 append-only；若 agent 重写文件（如 Codex `session_index`），offset 失效时全量重读该文件，
  成本可控（38 行）。
- 窗口被其他窗口完全遮挡或在其他 Space 时，Tauri 未必能可靠判断 occlusion；按“可见”处理。
  probe 成本低，可接受。
- 消息流只影响计数/时间，不触发正文读取；不会因为每条 token 重扫全源。
- 隐藏/最小化暂停 probe；app 关闭即停止更新，下次启动全量追平。若要求隐藏/关闭时也持续感知，
  需要菜单栏常驻/后台模式（§17 决策 10，P2），默认不做。

**为什么 G 放在 MVP 而不是 P1**：它决定“第二屏总览”是否可用；实现是只读 probe + 定时器 + JSONL cursor，
不改变 A+B 的正确性模型。FSEvents 只是把 5 秒压到 1 秒，平台细节多，留 P1。

**P2：agent hooks（更实时的语义信号）**：Qoder 已有 Claude Code 风格的 hooks（12 个事件，含
`SessionStart` / `UserPromptSubmit` / `Stop` / `PostToolUse`），用户级配置在 `~/.qoder/settings.json`；
Claude Code 也有同类 hooks。可选做法是安装 Perch 专用 command hook，把 `{event, session_id, cwd, ts}`
追加到 Perch 事件文件，Perch 监听/轮询该文件后只刷新对应会话。它能提供数据库里没有的
“正在运行/已停止”语义，也能在 `SessionStart` 时先建占位任务；但需要修改 agent 配置、
验证 payload 是否含 `session_id`、支持一键卸载，因此默认关闭，作为 opt-in。其他 agent 若有官方
hooks/events，按同一 adapter 接入，不猜测私有接口。

### 8.3 各源 probe 与指纹

| 源 | probe | 成员/删除信号 | 内容/活动信号 |
|---|---|---|---|
| qoder | SQLite 只读长连接：`data_version` + `-wal` mtime/size 回退 | `chat_sessions` + 源置顶的 `(count, hash(id, title, cwd, model, git_branch, archived, deleted_at, unread, pinned))`；183 行，毫秒级 | `max(updated_at)`（不可靠时用 `chat_session_messages` max(rowid)/count）、消息计数；只做增量更新 |
| claude | 文件树 manifest：顶层/子代理 JSONL + `tasks/**/*.json` 的 path/inode/size/mtime | manifest hash；新/删顶层 JSONL、tasks 目录变化 | `file_cursor.offset` 之后的新行（标题/时间/模型/分支）；subagent 文件计数 |
| codex | 文件树 manifest + `goals_1.sqlite` `data_version` | rollout/archive 清单 hash；`session_index.jsonl` hash | `session_index` offset 新行；goals `count/max(updated_at_ms)` |
| opencode / kilo | SQLite 只读长连接：`data_version` + `-wal` 回退 | `session` 的 `(count, hash(id, parent_id, title, directory, model, time_archived))` | `todo` 的 `count/max(time_updated)`（必要时全量 hash）；活动时间 |
| trae-cn | hooks 事件文件 offset | 新会话/新事件 | 事件行聚合；无 hook 时 `unavailable` |

规则：

- **probe 未变** → 只更新 `last_checked_at`，标记 `skipped`，不扫。
- **probe 变** → 先分类：成员/删除变化触发 `scope=source` 扫描；单会话内容变化（todo/标题）触发
  `scope=session`；纯活动变化只做 `updated_at`/计数增量；reset 触发 full。
- **JSONL**：offset 只在完整行后前进；最后半行留到下次；文件缩短/inode 变化/offset 越界 → reset + full。
- **SQLite**：`data_version` 只对长连接有意义，不作为跨进程/跨重启唯一依据；连接重建后先 full。
- **指纹不可用**（表缺失/查询失败）→ 直接 full 扫该源，并在 Diagnostics 报错。
- **每源扫描串行**；probe 可并行；探测失败退避，不算源缺失。

### 8.4 扫描事务与缺失判定

- 每次扫描创建 `scan_run`（source、mode=full/incremental、scope=source/session、trigger=launch/focus/live/manual/periodic、started_at）。
- 扫描开始时写 `status=running`；崩溃/退出后下次启动把残留的 running 标记为 interrupted，并强制 full 重扫该源。
- SQLite 源读取阶段用只读事务（`BEGIN`/`COMMIT`）包住同一源的多次查询，确保看到同一版本；WAL 下读不阻塞写。
- JSONL 最后一行可能是写到一半的半行：解析失败只跳过该行并记录，不把整个源判为失败；下次扫描补齐。
- JSONL 增量读的 `file_cursor` 与 task/todo upsert 在**同一个 `cache.db` 事务**内提交；cursor 只推进到完整行末尾，
  崩溃回滚后下次重读，避免丢行。
- 解析结果先在内存/临时表完成，再在**一个事务**内 upsert `task` / `task_todo`；每条写入 `last_seen_run`。
- **只有 `scope=source` 的 full 且源完整成功**时，才把该源本轮未出现的 task 标记 `source_missing=1`；
  `scope=session`、incremental 或部分失败时**不标记**，避免一次故障误判全删。live 触发的 full scan 也遵守此规则。
- 同一源扫描期间又有变更 → 置 dirty，当前扫描结束后合并再跑一次；每源最小扫描间隔 2–3 秒；
  探测/扫描失败按 5s → 10s → 30s → 60s 退避，不阻塞其他源。
- **异常缩水保护**：full 扫描任务数较上轮下降 >50%（且上轮 >20）时，先不标记 missing，
  `sync_state.status=partial`，Diagnostics 要求确认后再应用缺失。
- 源恢复后任务重新出现 → `source_missing=0`。
- 单源失败不阻断其他源；错误写 `sync_state` 与 Diagnostics。
- 同 key 多副本（Codex 活跃/归档、OpenCode 多 DB）：active 优先于 archived；同状态取 `updated_at` 最新；冲突细节写 Diagnostics。

### 8.5 对齐语义

| 源侧变化 | Perch 检测 | 快照处理 | overlay 处理 |
|---|---|---|---|
| 新会话 | probe 结构变化/全量扫描 | insert | 无 |
| 标题变化 | probe 结构变化/JSONL 新行 | 更新 `title`（无 `title_override` 时） | 保留人工标题 |
| 消息追加 | 活动指纹/JSONL 新行 | 只更新 `updated_at`/消息计数；不重扫任务列表 | 无 |
| 归档/取消归档 | 状态字段 | 更新 `source_archived` | 无 |
| 会话删除 | 全量扫描缺失 | `source_missing=1` | 保留 |
| 会话恢复 | 全量扫描出现 | `source_missing=0` | 保留 |
| todo 变化 | probe 结构变化/按任务重扫 | 该任务 todo 全量替换 | 无 |
| JSONL 截断/轮转 | cursor 失效 | 重置 cursor + full 扫该会话 | 无 |
| 源 schema 变化 | schema 指纹 | 全量重扫 + Diagnostics 警告 | 无 |
| 源 ID 迁移 | 新 ID 出现 + 旧 ID 缺失 | 新任务 + 旧任务 missing | 旧 overlay 孤立；P1 人工 re-link |
| 源不可用 | probe/扫描错误 | 保留上次快照；标 `stale` | 无 |
| 状态过期 | 上次观测的 `status_source` | 保留但显示 as-of 时间；源不可用标 `stale` | 无 |
| 源时间缺失 | 解析失败 | `updated_at=NULL`，按 `seen_at` 排序并标“时间未知” | 无 |

### 8.6 如果以后要回写 agent

默认仍然只读；但设计上给未来动作留出口，不把路堵死：

| 能力 | 首选实现 | 备注 |
|---|---|---|
| 打开/恢复会话 | 官方 CLI/URL scheme | 已实现 `continue_task`：Codex `codex://threads/<id>`（App 生成格式）；Qoder 无会话深链只能激活 App；OpenCode/Kilo 切 App 或 CLI resume；Claude 走 CLI resume |
| 归档/取消归档 | 官方 CLI/API | 不直接写 agent DB |
| 重命名 | 官方 CLI/API | 与 Perch `title_override` 分开 |
| 发消息/继续对话 | 官方 API/CLI | 必须显式确认目标会话；不做批量静默发送 |
| 跑命令 | 不通过 Perch 直接执行 | 若要做，必须 allowlist + 二次确认 + 审计 |

统一走 **Action Gateway**：`preview → 用户确认 → 执行 → 校验源状态 → 审计日志`。
每个源维护 capability matrix；不支持的动作在 UI 禁用，不猜测参数。
任何回写都不改变“全局 pin 只存 overlay”的产品语义。

### 8.6.1 打开来源（已实现）

详情面板提供“打开 <来源>”按钮，只打开 App 或新终端，不写 agent 数据：

| 来源 | 判定 | 动作 |
|---|---|---|
| Qoder | 恒定 App | 激活 App（`qoder://chat/<id>` 被官方判为不支持，无法定位会话） |
| Codex | `agent_kind=desktop/automation` → App；`cli` → 终端；未知时探测 | `codex://threads/<id>` 或终端 `codex resume <id>` |
| Claude | 恒定终端（URL handler 只能开新会话，不能恢复） | 终端 `claude --resume <id>` |
| OpenCode | App 运行 → App；未运行且有 CLI 进程 → 终端；都没有 → App 优先 | 激活 App，或终端 `opencode --session <id>` |
| Kilo | 同 OpenCode | 激活 App，或终端 `kilo` |

- App 探测：`lsappinfo find bundleID=...`；CLI 探测：`ps` 匹配进程名与 session id，排除 OpenCode App 的 `opencode-cli serve --service`。
- 已知限制：Qoder/OpenCode/Kilo 都没有可用的会话级深链，App 优先时只是切到前台并提示“无法定位到具体会话”；需要精确定位时用 CLI resume（OpenCode/Kilo），Qoder 只能在其 App 内自行选择历史会话。
- 终端动作用 `osascript` 调 Terminal 的 `do script`，命令经 argv 传入（不拼接 AppleScript 源码）；命令前加一个空格，吸收 shell 启动提示（如 oh-my-zsh 更新询问）可能吞掉的第一个按键。不启用 shell 插件，session id 限制为 `[A-Za-z0-9_-]`，cwd 做 shell 引号转义。

### 8.7 快照可重建

- 删除 `cache.db` 后重新启动可恢复全部任务；`overlay.db` 不受影响。
- `file_cursor` / `probe_state` 也在 `cache.db`：重建后 JSONL 会重读一次、SQLite 重新 full 扫描，
  不会漏任务，只是一次性成本；cursor 重建后继续增量。
- 重建 cache 会丢失“源已删除任务”的标题快照；overlay 仍保留 pin，但只能显示 key/源。
  若需要长期保留显示信息，P1 在 pin 时把标题/项目快照进 overlay。
- 快照写入用 `ON CONFLICT(key) DO UPDATE`，不累加、不重复。
- `Export → Overlay JSON` / `Import Overlay`（P1）用于备份 pin/notes。
- Diagnostics 提供 “Rebuild cache” 按钮：只删/重建 cache，不动 overlay。

---

## 9. 伴生 CLI（P2，可选）

主形态是 app；为了脚本/自动化/诊断，可提供一个复用 `perch-core` 的小 CLI（不是必须）：

```bash
perch-cli sync [--source qoder] [--full] [--dry-run]
perch-cli list [--since 30d] [--pinned] [--source ...] [--project ...]
                [--status ...] [--tag ...] [--search ...] [--format table|json|csv]
perch-cli show <key|前缀>
perch-cli pin <key|前缀> / unpin <key|前缀>
perch-cli status <key|前缀> <todo|doing|done|dropped|auto>
perch-cli doctor
perch-cli export --output snapshot.html
```

- 与 app 共享同一 SQLite 路径与 core 逻辑；写入同样只碰 overlay 表。
- `--format json` 的 stdout 只放数据；提示走 stderr。
- 如果不需要脚本化，整个 CLI 可以不做，Diagnostics/Export 已在 app 内。

---

## 10. macOS App 形态与 UI

### 10.1 窗口与原生感

- Tauri v2 窗口：`titleBarStyle: "Overlay"` + `hiddenTitle: true`，保留红黄绿交通灯；侧边栏延伸到标题栏下方。
- 最小尺寸约 1100×700；默认 1280×820；可记忆窗口大小/位置。
- 侧边栏可使用 macOS vibrancy/材质（Tauri window effects；P1），列表与详情用系统字体。
- 浅色/深色跟随系统；允许手动覆盖（存 `localStorage`）。
- 原生菜单：
  - `Perch`：About、Settings…、Quit
  - `File`：Reveal Data Folder（MVP）；Export Snapshot…、Import Overlay…（后续）
  - `View`：Refresh、Toggle Inspector、Search
  - `Help`：Diagnostics、Data Sources
- 快捷键：`⌘R` 刷新、`⌘F` 搜索、`⌘⇧P` 置顶/取消、`⌘⌥I` 打开 Inspector；
  `⌘E` 导出为后续能力。
- 右键菜单（MVP）：Pin / Unpin。Set Status、Copy Resume Command、Reveal Project、Hide 等后续再加。
- 可选菜单栏图标（P2）：显示需关注任务数；不是 MVP，避免常驻。

### 10.2 三栏布局

```
┌───────────────┬────────────────────────────────┬──────────────────────┐
│ 侧边栏         │ 任务列表                        │ Inspector            │
│               │                                │                      │
│ 智能列表       │ ★ Global pinned                │ 标题 / source /       │
│  最近          │ ┌────────────────────────────┐ │ workspace             │
│  全部任务      │ │ ★ Qoder 任务…  2h          │ │ 源状态 / todos        │
│  已归档        │ └────────────────────────────┘ │ 最近活动 / session id │
│               │ playground · Qoder · 2h    ☆   │ resume 命令（只读）   │
│ Workspaces    │ article-… · Claude · 昨天  ☆   │ 项目路径              │
│  playground   │ playground · Codex · 3d    ☆   │                      │
│  article-…    │ playground · Kilo · 5d     ☆   │                      │
│               │                                │                      │
│ 来源           │                                │                      │
│  q Qoder      │                                │                      │
│  ✳ Claude     │                                │                      │
│  ◎ Codex      │                                │                      │
│  ■ OpenCode   │                                │                      │
│  K Kilo       │                                │                      │
└───────────────┴────────────────────────────────┴──────────────────────┘
```

- **最近视图**：智能列表第一项；不按 workspace 分组，按 `updated_at` 倒序平铺，每行 meta 展示
  `workspace · source · 最近活动`，workspace 仅作为任务属性。
- **平铺列表**：全部任务与已归档均按 `updated_at` 倒序平铺，不再按 workspace 分组，也不折叠
  “更早”任务；每行 meta 为 `workspace · source · 最近活动`，按 workspace 过滤时省略重复的 workspace 名。
- **时间窗口**：最近视图只显示最近 7 天；全部任务与已归档显示全部匹配任务。时间以 `updated_at`
  为准，缺失时用 `seen_at`。
- **Global pinned**：任务列表顶部独立区域，始终展开，不受视图、筛选影响。
- **侧边栏**：智能列表（最近 / 全部任务 / 已归档）、Workspaces、来源。
  Workspaces 按组内最近活动倒序（同时间按名称）；来源固定按 Qoder、Claude Code、Codex、OpenCode、Kilo 排列。
  来源项显示各工具的品牌图标、名称与任务数；扫描出错时名称后显示红色 `!`，完整状态/错误通过悬停提示查看。
  标签和人工分组是后续能力，MVP 不显示空入口。
- **Inspector**：只读展示标题、source、workspace、源状态、todos、最近活动、session id、
  resume 命令和项目路径；顶部保留一个来源色点作为界面中唯一的颜色来源标识。
  MVP 唯一写操作是 Pin/Unpin。备注/标签编辑后续再加。
- 顶部工具栏：搜索框、筛选 chips、刷新、扫描状态、排序。
- 行信息：`★ | source 标签 | 标题 | workspace（最近视图） | 源状态 | todo 进度 | 最近活动`。
- 源原生标记：`源置顶`、`未读`、`已归档`、`子会话`、`自动化`、`源已删除`。
- 空状态：显示“没有匹配任务”+ 清除筛选；源不可用时显示上次成功扫描时间与错误，不清空快照。
- 主题：跟随系统 + 手动覆盖；无外部字体/图片；所有图标内联 SVG。

### 10.3 交互

- 单击行选中并在 Inspector 显示；双击只打开 Inspector，MVP 不执行主操作。
- 置顶：点击星标立即写入 overlay；乐观更新，失败回滚并 toast。这是 MVP 唯一的任务写操作。
- 视图与窗口：最近、全部任务、已归档均为按最近活动倒序的平铺列表，行内展示 workspace；
  运行中任务固定最前，其后为源置顶任务；最近视图只取近 7 天，其余视图展示全部匹配任务。
- 搜索：title / project / key 子串；中文直接匹配；输入即筛。MVP 不搜索 tag/note。
- 筛选：来源多选、workspace、源状态、归档/子会话/自动化。
- 排序：运行中任务固定最前，其后为源置顶，再按最近活动排序；M2 的 Global pinned 固定在最上方；可切换创建时间/标题。
- 后续交互：状态、备注、标签、隐藏、重命名、复制 resume、打开项目、拖拽排序和分组。
- 键盘：↑/↓ 选择，Enter 打开 Inspector，空格预览（后续）。

### 10.4 Tauri commands（IPC）

```rust
#[tauri::command] fn bootstrap(state) -> Result<BootstrapPayload, AppError>;
#[tauri::command] async fn refresh_sources(state, source: Option<String>) -> Result<SyncReport, AppError>;
#[tauri::command] fn continue_task(state, task_key: String) -> Result<ContinueOutcome, AppError>;
#[tauri::command] fn set_pin(state, key: String, pinned: bool) -> Result<OverlayView, AppError>;
#[tauri::command] fn doctor(state) -> Result<DoctorReport, AppError>;
#[tauri::command] fn reveal_data_dir(state) -> Result<(), AppError>;

// 后续版本：set_overlay(status/tag/note/hide)、set_pin_order、create_group、
// export_snapshot、reveal_project
```

- 事件：`scan-started`、`scan-finished`（含 trigger、每源结果、changed_keys、full/incremental/skipped）、`scan-error`、`overlay-updated`。
- 运行时 probe 由 `src-tauri` 的后台任务驱动，监听窗口 `visible/minimized/focused` 状态；前端不轮询。
- `bootstrap` 从 `cache.db` 读快照、从 `overlay.db` 读 pin，在 Rust 侧按 key 合并后返回（本机约 300 条，JSON < 1MB）。
- `refresh_sources` 只写 `cache.db`；`set_pin` 只写 `overlay.db`；两条写入路径不共享事务。
- 所有写操作在 Rust 侧校验 key 存在、参数类型和字段长度；前端不能传 SQL/任意路径。
- `resume_cmd` 只读展示；`continue_task` 只执行能力表内的启动动作（URL scheme / 激活 App / 固定模板 Terminal 脚本），不接受前端任意命令。
- 剪贴板、打开项目等插件权限随对应后续能力一起启用，MVP 不授予。

### 10.5 打包与分发

- `cargo tauri build` 产出 `target/release/bundle/macos/Perch.app` 与 `.dmg`。
- Apple Silicon 本机构建即可；可选 universal binary（`aarch64-apple-darwin` + `x86_64-apple-darwin`）。
- 个人使用可不签名；首次打开若被 Gatekeeper 拦截，右键 Open 或 `xattr -dr com.apple.quarantine`。
- 正式分发再做 codesign + notarization；设计不依赖 App Store。
- app 数据目录：`~/Library/Application Support/<bundle-id>/`；菜单可 Reveal Data Folder。
- 日志：`~/Library/Logs/Perch/`（滚动，只记扫描错误与耗时，不记正文）。

---

## 11. 隐私、只读与安全

### 11.1 只读保证

| 约束 | 实现 |
|---|---|
| 源只读 | `rusqlite` `SQLITE_OPEN_READ_ONLY` + `PRAGMA query_only=ON`；JSONL 只 `File::open` |
| 轮询只读 | G 的 SQLite `data_version`/指纹查询复用只读连接；JSONL 只读新字节；查询结束即释放读事务/文件句柄，不新增写路径 |
| 不写源置顶/归档 | 不执行任何源写 SQL；Qoder sidebar pinned 只读 |
| 可验证 | 单测记录源主库/JSONL 的 mtime/hash 与 Qoder `sidebar_placements.pinned`，扫描前后必须一致；不比较 `-wal`/`-shm` 的 mtime（只读 WAL 连接可能触碰 shm） |
| 只读副本 | Codex/OpenCode 同 id 副本按 key 覆盖，不修改源 |
| 用户数据隔离 | 扫描路径只打开 `cache.db`，不持有 `overlay.db` 连接；overlay 写入只打开 `overlay.db` |
| 失败隔离 | 单文件/单源错误进 `errors/notes`；其余源照常；快照保留上次结果 |

### 11.2 数据边界与隐私（默认最小化）

**MVP 默认读取并存储**：
- 会话标题（源已生成的 `title` / `ai-title` / `thread_name`）、项目路径、时间、模型、分支、计数、
  todo 的 subject/content、状态字段、源置顶/未读标志。
- 运行时 probe 元数据：文件路径、inode、size、mtime、读取 offset、指纹 hash；不含正文。
- `resume_cmd` 由模板生成（source + id + 路径），不读 shell 历史。

**MVP 默认不读取/存储**：
- message / prompt / assistant 正文、tool input/output、附件内容、思考过程。
- Claude tasks 的 `description`（只读 `subject/status`）。
- Codex `history.jsonl` 的 prompt 文本（标题只用 `session_index.thread_name` / goal objective）。
- Qoder message `content`（只做 count）。
- 账号凭证、JWT、加密库密钥。

**后续可选能力（默认关闭，opt-in）**：
- 任务摘要/正文预览、Claude `description`、Codex prompt 历史、Qoder message content。
- Trae CN 加密库/私有 API、其他 agent 的非公开接口。
- 启用前必须：独立开关 + 按源说明 + 最小化存储 + 一键清除；默认不进入 M1–M3。
- 任何正文/凭证数据不得进入日志、导出或跨源聚合；导出默认只含元数据。

**说明**：标题本身可能包含用户 prompt 的片段（由 agent 自己生成/截断）；Perch 只在本机 SQLite 与 app 内保存，
不发送到任何地方。若用户要求“标题也不落盘”，可加 `title_mode=fallback`（只显示项目+日期），见 §17 决策 8。

### 11.3 Tauri 安全边界

| 风险 | 措施 |
|---|---|
| 外部网络 | MVP 不发起网络请求；CSP `default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'`。后续接入官方 API 时按源加白名单，不放开任意域名 |
| 前端越权读文件 | 前端不启用 `fs`/`shell` 插件；所有源读取在 Rust core，只暴露窄命令 |
| 远程内容 | 不加载任何远程页面/字体/图片；无 `http://`/`https://` 资源 |
| XSS | Svelte 默认转义；禁止 `{@html}` 渲染任务字段；CSP 兜底 |
| 任意命令执行 | MVP 不启用 shell 插件；`continue_task` 仅按 allowlist 打开 URL / 激活 App / 通过 `do script` 执行固定模板命令，session id 校验字符集；不接收前端任意命令 |
| 回写 agent | 默认关闭；若启用，只走官方 CLI/API/URL scheme，不直接写 agent DB；每个动作 preview → 确认 → 校验 → 审计 |
| capability 过宽 | `capabilities/default.json` 仅允许本 app 的命令与必要插件（clipboard write；P1 opener） |
| 本地数据 | 本地用户级信任；不加密 DB（与 usage-ledger 一致）；不做账号/云同步 |

---

## 12. 与 usage-ledger 的可选集成（已实现）

- 只读打开 `~/.local/share/usage-ledger/usage.db`（`PERCH_USAGE_DB` / `USAGE_LEDGER_DB` /
  `USAGE_LEDGER_HOME` 可覆盖），不写、不迁移、不触发 sync。
- 匹配：`usage_turn.source = task.source AND usage_turn.session_id = task.source_id`；
  Qoder credits / est 也按 `session_id` 挂到 qoder 任务。
- 数据来源：`usage_turn`（未归档）与 `usage_daily`（已 prune 的历史）合并；
  `usage_credit`、`usage_est` 分表读取。旧版 `usage_est` 无 `source` 列时按 `qoder.est` 处理。
- 展示：
  - token 真值（Claude/Codex/OpenCode/Kilo/Qoder 历史）→ “本任务 token”
  - Qoder `usage_credit`（billable / free credits、请求数）→ “本任务 credits”，单独标注
  - Qoder `usage_est` → “est”，单独标注，永不与真值/credits 相加
  - 选中单一来源时，顶部显示该来源的 7 天 token / credits / est 摘要；不同来源单位不同
    （Qoder 是 credits、CLI agent 是 token），不做跨来源汇总；不展示无法挂到任务的 activity
  - 侧边栏页脚的用量报告图标：调用 usage-ledger 的 `usage html`，报告写入 Perch 缓存目录后
    用默认浏览器打开（等价 `usage html --open`）。这是全局报告，不按来源过滤。
    与直接运行该命令一样，usage-ledger 可能会先自动同步自己的库；Perch 不参与写入
- usage-ledger DB 不存在、表缺失或 schema 不兼容 → 自动隐藏这些区块，不影响任务总览。
- 该集成只在 Inspector 与顶部摘要中展示，不改变任务快照表结构；按需查询/聚合，结果不落库。
- usage-ledger 主库或 `-wal` mtime 变化时，Tauri 的 3 秒 probe 会刷新前端；Perch 本身不写该库。

---

## 13. 目录结构（计划）

```
perch/                                  # 仓库名
├── README.md
├── docs/
│   ├── DESIGN.md                       # 本文
│   ├── DATA-PROBE.md                   # 本机实测
│   └── NAMING.md                       # 命名短名单
├── package.json                        # Svelte + Vite + Tauri CLI
├── vite.config.ts
├── tsconfig.json
├── index.html
├── src/                                # Svelte 5 + TS 前端
│   ├── main.ts
│   ├── App.svelte
│   ├── lib/
│   │   ├── api.ts                      # invoke 封装 + 类型
│   │   ├── stores.ts                   # 任务/筛选/选中状态
│   │   ├── components/                 # Sidebar/List/Inspector/Row/...
│   │   └── styles/
│   └── assets/                         # 内联 SVG 图标
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   ├── icons/
│   └── src/
│       ├── main.rs
│       ├── lib.rs
│       ├── commands.rs
│       ├── state.rs
│       └── events.rs
├── crates/
│   └── perch-core/                   # 纯 Rust core，可独立 cargo test
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── models.rs
│           ├── collectors/
│           │   ├── mod.rs
│           │   ├── qoder.rs
│           │   ├── claude.rs
│           │   ├── codex.rs
│           │   ├── opencode.rs         # OpenCode/Kilo 共用
│           │   └── trae.rs
│           ├── probe/
│           │   ├── mod.rs              # Probe/Change/Scan 调度接口
│           │   ├── sqlite.rs           # data_version + 指纹
│           │   ├── files.rs            # manifest + file_cursor
│           │   └── hooks.rs            # P2 事件文件
│           ├── store.rs
│           ├── overlay.rs
│           ├── query.rs
│           └── error.rs
├── crates/perch-cli/                   # P2 可选伴生 CLI
└── tests/                              # Rust 集成测试 + fixtures
```

依赖方向无环：

```
src-tauri → perch-core
perch-cli → perch-core
perch-core → models / collectors / probe / store / overlay / query
query → store, overlay
store → models
collectors → models
probe → models
```

`probe` 与 `collectors` 共用源路径/连接定位，但 probe 只做轻量查询，不调用全量解析。

主要 Rust 依赖：

| crate | 用途 | 备注 |
|---|---|---|
| `tauri` v2 | app 壳、IPC、窗口、菜单 | 核心 |
| `rusqlite`（`bundled`） | 读写本地 SQLite | 自带 SQLite，确保 JSON1/一致性 |
| `serde` / `serde_json` | IPC 与源 JSON | 核心 |
| `chrono` / `time` | 时间解析/本地时区 | 二选一 |
| `thiserror` | 错误类型 | 核心 |
| `walkdir` | 递归扫描 JSONL | 可选，std 也能做 |
| `notify` | 文件监听（P1） | 默认关闭 |
| `tauri-plugin-clipboard-manager` | 复制 resume 命令（后续） | 仅写权限；或前端 Clipboard API |
| `tauri-plugin-opener` | 打开数据目录（MVP）/ 项目路径（后续） | 仅白名单路径，或自写 `open` 命令 |

前端依赖：`svelte`、`vite`、`typescript`、`@tauri-apps/api`、`@tauri-apps/cli`（dev）。
不引入 UI 框架/图标库/远程字体；样式手写，图标内联 SVG。

---

## 14. 测试计划

| 类别 | 用例 |
|---|---|
| 采集- Qoder | 标题优先级、archived/unread、turn state 映射、side_chat 父键、消息计数 |
| 采集- Claude | custom/ai title、顶层 vs subagents、tasks JSON 状态、无标题 fallback |
| 采集- Codex | session_index 与 rollout 合并、goals 状态、归档副本去重、`.zst` 跳过、automation 标记 |
| 采集- OpenCode/Kilo | 顶层/子会话、archived、todo 计数、双 DB 去重 |
| 采集-Trae | hook 事件聚合、无 hooks 时 unavailable |
| 存储 | upsert 幂等、source_missing 标记/恢复、失败源不误标、快照重建 |
| 一致性 | 签名未变跳过、全量对账发现删除、incremental 不标记缺失、多副本去重、schema 漂移降级 |
| 运行时 probe | SQLite `data_version` 变化/不变、成员 vs 活动指纹、JSONL cursor 半行/截断/inode 变化、隐藏时暂停、扫描串行/coalesce、live 不误标 missing |
| 存储拆分 | 扫描路径不打开 `overlay.db`；删除 `cache.db` 重建后 overlay 仍在 |
| 覆盖层 | pin/unpin、幂等、重启后保留、源删除后 pin 保留；状态/标签/备注等后续能力单独测试 |
| **只读保证** | 扫描前后源文件 mtime/hash 不变；Qoder `sidebar_placements.pinned` 不变 |
| 查询 | workspace 分组（`project_path` 优先、缺失回退）、7 天窗口（`updated_at` 缺失用 `seen_at`）、pinned 强制显示、来源视图同样按 workspace 分组、中文搜索、前缀歧义、子会话折叠 |
| IPC 命令 | 参数校验、非法 key/参数拒绝、并发写串行、错误序列化 |
| 前端 | Vitest：筛选/排序/搜索/乐观更新/错误回滚；Svelte 组件快照 |
| 打包 | `cargo tauri build` 产出 `.app`；手工冒烟：启动、刷新、pin、重启后保留 |
| 安全 | CSP 无外链、无 `{@html}`、capability 仅最小集、`open` 路径来自 DB |

测试手法：Rust 单测/集成测试用临时 HOME + `PERCH_SOURCE_HOME` / `PERCH_CACHE_DB` / `PERCH_OVERLAY_DB`；
各源最小 SQLite/JSONL fixture；Tauri 端到端在 macOS 上以手工冒烟为主（Tauri WebDriver 对 macOS 支持有限，
不承诺自动化 UI E2E）；真实 launchd/上游 schema 漂移不在单测范围，由 Diagnostics 兜底。

---

## 15. 里程碑

| 阶段 | 内容 | 验收 |
|---|---|---|
| M1 | Rust core：五个 P0 采集器 + `cache.db` 快照 + `cargo test`；Tauri shell 显示只读任务列表 | 本机 300+ 任务可列出；源只读校验通过；`cargo tauri dev` 可开窗 |
| M2 | `overlay.db`（仅 pin 字段）+ Tauri commands + Inspector Pin/Unpin | app 内完成 pin；重启后保留；源置顶不变；没有其他任务写操作 |
| M3 | macOS 原生打磨：三栏/侧边栏/菜单/快捷键/平铺任务列表 + 7 天窗口/签名跳过/全量对账/G 全源 probe（SQLite `data_version` + JSONL cursor）/打包 `.app`/`.dmg` | 双击 `.app` 可用；默认平铺列表、行内展示 workspace，最近视图只取近 7 天；扫描不阻塞 UI；任意源新任务/新信息数秒内出现；无外部请求 |
| M4 | 覆盖层扩展（人工状态、标签、备注、隐藏、重命名、复制 resume、打开项目）+ 置顶排序 + 跨 agent 分组 + 静态导出 | 后续操作可用；分组可筛选；导出可离线打开 |
| M5 | usage-ledger 关联（P2）+ Trae hooks（P2）+ 伴生 CLI（可选） | Inspector 显示 token/credits，分轨标注 |

M1–M3 是核心价值；M4/M5 可按反馈调整顺序。

---

## 16. 风险与已知缺口

| 风险 | 影响 | 处置 |
|---|---|---|
| “Task”定义分歧 | 做成 todo 列表则碎片化 | 本方案明确会话=Task，todo=子项；见 §17 决策 3 |
| 源状态不完整 | Claude/Kilo 无完成状态；OpenCode 仅 busy/idle 二态 | 无证据时显示“空闲/无显式状态”；MVP 不承诺完成状态，人工状态后续再加 |
| Codex 标题覆盖率 | session_index 38/69 | goal objective 兜底 + fallback；Diagnostics 报告覆盖率 |
| Qoder schema 漂移 | 采集失败 | 只读 + 缺少表时 `schema drift` 错误，其他源继续 |
| Qoder 无会话深链 | 不能精确跳回会话 | `qoder://chat/<id>` 被 Qoder 判为 `DEEP_LINK_ROUTE_UNSUPPORTED`（官方日志），只能激活 App。Codex `codex://threads/<id>` 是 Codex 自身生成的格式。OpenCode/Kilo App 同样不能定位会话，只能 CLI resume 或切 App 前台并提示限制 |
| Kilo resume 未验证 | 不能一键恢复 | MVP 展示命令 + 项目路径；复制后续加入；不猜测参数 |
| Trae/ChatGPT 普通聊天无本地任务 | 总览不完整 | Diagnostics/UI 显式说明；Trae 可选 hook；ChatGPT Codex 会话经 Codex 采集；MVP 不解密。后续按 §11.2 评估加密源/官方 API |
| 标题含 prompt 片段 | 隐私顾虑 | MVP 只存标题元数据；正文能力默认关闭、opt-in；可切 fallback 模式 |
| mtime 不可靠 | 增量漏数 | 签名只做跳过；启动/周期全量对账兜底；不把 mtime 当唯一真相 |
| 源 ID 迁移 | overlay 孤立 | `source_missing=1` 保留 overlay；P1 提供人工 re-link，不自动合并 |
| 跨 agent 自动合并误判 | 错误合并任务 | 只做人工 group，不做自动语义聚类 |
| Tauri/WKWebView 差异 | 样式/剪贴板/滚动行为与 Chrome 不同 | 以 macOS WKWebView 为目标测试；避免冷门 CSS/API |
| macOS 签名/公证 | 他人机器首次打开被拦 | 个人使用右键 Open；分发前 codesign/notarize |
| Rust + 前端双栈成本 | 迭代慢于纯 Python | core 与 UI 解耦；先完成 M1/M2 再打磨 |
| 快照库删除导致 overlay 丢失 | pin（以及后续备注）丢失 | 已拆 `cache.db` / `overlay.db`；MVP 可通过 Reveal Data Folder 手动备份 `overlay.db`，导出功能后续加入 |
| 任务量大后 UI 卡顿 | 首屏慢 | 平铺列表 + 最近视图 7 天窗口 + 前端本地筛选；P1 分页/虚拟滚动 |

---

## 17. 已确认决策

以下决策已于 2026-11-02 确认，实现按此执行；未列出的细节按本方案默认值。

1. **名称**：**Perch**。
2. **形态**：Tauri v2 + Rust + 系统 WebView，产物 `.app`/`.dmg`；不做本地 HTTP 服务。
3. **Task 粒度**：顶层会话 = Task，todo = 子项；不把 todo 提升为独立任务。
4. **存储拆分**：`cache.db`（快照，可重建）+ `overlay.db`（用户数据，独立备份）。
5. **前端栈**：Svelte 5 + TypeScript + Vite。
6. **MVP 操作范围**：**只做全局置顶/取消置顶**。人工状态、标签、备注、隐藏、重命名、复制 resume、
   打开项目目录、置顶排序、跨 agent 分组、静态导出全部后置，不在 M1–M3 实现。
7. **默认视图**：任务列表平铺，行内展示 workspace；最近视图取近 7 天，全部任务/已归档展示全部；
   所有全局 pin 始终显示；子会话/自动化默认隐藏，源归档任务只在已归档视图出现。
8. **标题与正文**：MVP 只存标题等元数据；正文/摘要默认不落盘，后续按源 opt-in；
   标题默认显示源标题；如后续有隐私需求，可加 `title_mode=fallback` 开关（默认关闭）。
9. **跨 agent 分组**：不进 M1–M3，后续版本做人工分组。
10. **菜单栏模式**：默认 P2；若要求 Perch 隐藏/关闭时也持续感知，必须启用该模式。
11. **伴生 CLI**：P2，可选；app 内功能优先。
12. **一致性策略**：确认 A+B+G；G 覆盖所有已接入 source（Qoder/Claude/Codex/OpenCode/Kilo）：
    SQLite 用 `data_version` + 指纹，JSONL 用 manifest + `file_cursor`；Trae 依赖 hooks；
    FSEvents P1、hooks P2。
13. **后续能力**：正文、加密源、agent 内操作不进 M1–M3，作为 opt-in 预留。

---

## 附：术语

| 术语 | 含义 |
|---|---|
| Task | agent 的一个顶层会话/线程，本项目的核心单位 |
| Child task | 子代理、侧聊、分支会话；默认折叠 |
| Todo | 任务内部 checklist 项 |
| Overlay | 用户自己的 pin/status/tag/note 等，独立于源 | MVP 只有 pin；其余后续加入 |
| Global pin | 跨 agent 的全局置顶，不修改源置顶 |
| Source status | 源自己表达的状态，可能缺失 |
| Manual status | 用户人工标记，不声称源真实完成 |
| Snapshot | 可重建的任务元数据索引 |
| Resume command | 展示用的回到会话命令，不由 app 执行 |
