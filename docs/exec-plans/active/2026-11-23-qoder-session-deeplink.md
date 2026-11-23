# 2026-11-23 Qoder 会话深链定位

| 项 | 内容 |
| --- | --- |
| 状态 | 开发中 |
| 类型 | 变更文档 |
| 关联产品规格 | [agent-sources.md R3](../../product-specs/agent-sources.md) |
| 关联设计 | [SOURCES.md](../../design-docs/source-designs/SOURCES.md)、[深链调研](../../references/agent-session-deeplinks.md) |

## 背景

Perch 当前对 Qoder 会话的“打开”只激活 App，不能回到指定会话。严谨实机验证（两次独立交叉验证）确认 Qoder 支持会话级深链 `qoder://chat/open?sessionId=<id>`，可从非目标会话切到目标会话。

## 目标行为

- Qoder：当用户对 Qoder 会话触发“继续工作”，Rust 固定生成 `qoder://chat/open?sessionId=<source_id>`，提示“深链打开 Qoder”，`locates_session = true`。
- `sessionId` 必须匹配 `^[A-Za-z0-9_-]{1,128}$`；不符合时回退现有“激活 App”行为，提示无法定位具体会话。
- 除 Qoder 深链与 Codex 桌面深链外，其他来源的“继续工作”统一改为激活对应 App，不再启动 CLI 恢复。
- 前端仍只传任务 key；URL 只能由 Rust 固定模板生成。

## 验收

### 正常路径

1. 标准 Qoder session id（UUID 风格，含 `-`）生成 `qoder://chat/open?sessionId=<id>`。
2. `describe_continue` 返回 Deeplink 通道，标签“深链打开”，detail 指向 Qoder，`locates_session = true`。
3. `plan_continue` 返回 `ContinueAction::OpenUrl`，target 为 Qoder。
4. 点击后在 Qoder 中打开目标会话；无需额外确认。

### 异常与边界

1. `source_id` 为空、含非法字符或超过 128 字符：`describe_continue` 与 `plan_continue` 都回退“激活 App”，提示“无法定位到具体会话”，不构造 URL；不返回错误打断用户。
2. 会话已被 Qoder 归档/删除：Perch 不预判可用性；URL 由 Qoder 自行处理（其主进程会校验并报会话不可用）。
3. `requestSetId` 与 `qoder-app://` 变体不在本次范围，不启用。
4. Codex 桌面深链同样只接受字符集校验通过的 `source_id`；不合法时回退激活 Codex App。
5. 其他来源（Claude、OpenCode、Kilo、Cline、Trae CN、Antigravity、MiniMax、CodeArts、CodeBuddy）无论 `agent_kind` 为何，都只激活对应 App；前端“继续工作”不再出现终端通道，Tauri 侧也不再保留终端执行代码。

## 验证记录

- 2026-11-23：TDD 完成。`continue_work` 单测覆盖：Qoder UUID / 下划线 ID 深链、超长 ID 回退、非法字符回退且不构造 URL、Codex desktop/automation 深链与其他 kind 激活 App、全部 App-only 来源、未知来源；`store_query` 覆盖 `TaskView.continue_hint` 的 Qoder Deeplink 与 Claude App 通道。
- 2026-11-23：前端契约收敛为 `deeplink` / `app` 两通道；`ContinueOutcome.channel` 固定 `app`；demo fixture、CSS 与 stub 同步删除终端分支，`npm run check` 0 errors 0 warnings。
- 2026-11-23：`npm run demo` 实机核对：Qoder 任务显示“深链打开 / 通过会话深链打开 Qoder”；Claude、Cline、Kilo 任务显示“仅激活 App / 激活 …，无法定位到具体会话”。
- 2026-11-23：用本机 `cache.db` 的只读副本核对真实任务（822 条，全部 `source_id` 通过字符集与长度校验）：Qoder 生成 `qoder://chat/open?sessionId=<uuid>`、Codex `desktop` 生成 `codex://threads/<uuid>`，其余来源（含 Codex `cli` / `automation` 之外的 kind、Claude、OpenCode、Kilo、Trae CN、Cline、Antigravity、MiniMax、CodeArts、CodeBuddy）全部为 `ActivateApp`；未打开任何 App。
- 2026-11-23：限制：本机屏幕锁定，`npm run tauri dev` 可启动并完成首次扫描，但无法在真实窗口点击“继续工作”验证 Qoder 深链的实际跳转；该跳转已有两次独立实机验证记录（见深链调研），本次未重复执行。
- 2026-11-23：`cargo fmt --all --check`、`cargo clippy -p perch-core --all-targets -- -D warnings`（另含 `cargo clippy -p perch --all-targets -- -D warnings`）、`cargo test -p perch-core`、`npm run check`、`npm run build`、Markdown 本地链接检查（38 个文件，0 missing）全部通过。
