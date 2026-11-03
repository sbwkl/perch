# Perch

> 多 agent 任务总览：把 Qoder / Claude Code / Codex / OpenCode / Kilo 的本地会话任务汇总到一处，
> 支持跨 agent 全局置顶（MVP 唯一的任务操作）；默认只读采集，pin 只写入 Perch 自己的
> `overlay.db`，**不改动任何 agent 的数据**。人工状态、标签、备注等后续加入；回写 agent 的动作
> 单独开关与确认。

**状态**：设计阶段（v0.3.3，产品决策已确认，待实现）。当前仓库只有设计文档与数据探测记录，没有可运行代码。

- 设计方案：[docs/DESIGN.md](docs/DESIGN.md)
- 命名参考：[docs/NAMING.md](docs/NAMING.md)
- 本机数据探测：[docs/DATA-PROBE.md](docs/DATA-PROBE.md)

## 一句话

你同时用多个 coding agent，任务散落在各自的侧边栏里。Perch 只读它们的本地会话元数据，
给你一个跨 agent 的总览：**先找到任务，再决定回哪个 agent 继续**。全局置顶存在 Perch 自己的
数据里，源 agent 的置顶/归档/删除默认一概不碰；人工状态、标签、备注和回写动作后续单独加入。

## 数据一致性

- 源是唯一事实来源；`cache.db` 是可重建的快照，`overlay.db` 是用户数据。
- 启动先读快照秒开，后台做全量对账；聚焦/手动刷新用签名跳过未变源；每 6 小时再全量对账一次。
- 窗口可见时每 3–5 秒对全部 source 做 probe（SQLite `data_version`/指纹、JSONL manifest/offset、
  hooks 事件）；隐藏/最小化暂停，关闭后下次启动追平；任意源的新任务/新信息通常几秒内出现，
  不写源、不常驻 daemon。Trae CN 仅在安装 hooks 后可见。
- 只有完整扫描成功才标记 `source_missing`；源不可用/部分失败不会误删快照。
- 全局 pin 永远只存 `overlay.db`，即使源里删除或迁移了任务，也不会丢。

详细方案与方案对比见 [DESIGN §8](docs/DESIGN.md)。

## 形态

- macOS 原生 `.app`：Tauri v2 + Rust 后端 + 系统 WKWebView 前端（不是 Electron）
- 三栏布局：智能列表 / Workspaces / 来源 → 任务列表（Global pinned + workspace 分组，近 7 天展开、更早折叠）→ Inspector 详情
- MVP 任务操作只有全局 pin/unpin；状态、标签、备注、隐藏、重命名等后续实现
- 启动即读本地快照秒开，后台扫描源；MVP 无本地 HTTP 服务、无云、无外部请求、无常驻 daemon
- Rust core 可独立 `cargo test`；前端 Svelte 5 + TypeScript + Vite
- 产物：`cargo tauri build` → `Perch.app` / `.dmg`

## 与 usage-ledger 的关系

两个项目互相独立：

| | usage-ledger | Perch |
|---|---|---|
| 回答的问题 | 我花了多少 token / credits | 我的任务在哪个 agent、现在什么状态 |
| 主键 | 一次模型调用（turn） | 一个 agent 会话 / 线程（task） |
| 数据方向 | 只读源 → token 归档 | 只读源 → 任务快照 + 独立覆盖层 |
| 形态 | Python CLI + 单文件 HTML | Rust + Tauri macOS app |

可选（P2）：Perch 只读连接 usage-ledger 的 `usage.db`，按 `session_id` 给每个任务附带
token / credits；两条轨道分别标注，永不相加。没有安装 usage-ledger 时该功能自动隐藏。
