# 2026-11-23 Claude Desktop 会话深链定位

| 项 | 内容 |
| --- | --- |
| 状态 | 已验证 |
| 类型 | 变更文档 |
| 关联产品规格 | [agent-sources.md R3](../../product-specs/agent-sources.md) |
| 关联设计 | [SOURCES.md](../design-docs/source-designs/SOURCES.md)、[ARCHITECTURE.md](../../ARCHITECTURE.md) |

## 背景

Perch 目前把所有 Claude 会话固定标记为 CLI 会话，「打开」始终在终端执行 `claude --resume <uuid>`。实测同一批会话中有相当一部分由 Claude Desktop 内嵌 Claude Code 创建（JSONL 记录包含 `entrypoint = "claude-desktop-3p"`），用户希望这些会话能回到 Claude Desktop 并定位到同一会话，而不是只打开终端。

已完成只读探针：Claude Desktop 的本地会话索引中存在 `cliSessionId = JSONL UUID` → `sessionId = local_<uuid>` 的映射；Claude Desktop 自身处理 `claude://code/continue?session=<id>`，其中 `session` 接受 `local_<...>` 形式。Perch 只需读取这两个 ID 字段，不读取任何消息正文、prompt、工具输入输出或凭据。

## 目标行为

- 仅对 `entrypoint` 最终为 `claude-desktop-3p`、且存在合法 `local_<uuid>` 映射的 Claude 会话，使用固定模板 `claude://code/continue?session=<local_id>` 在 Claude Desktop 中定位会话。
- 原有 CLI 会话继续走终端 `claude --resume <uuid>`。
- `source_id`、`task_key`、会话状态、todo、搜索和缓存主键保持不变；复用现有 `agent_kind`、`deeplink`、`resume_cmd` 字段，不新增数据库列，不改变 IPC 契约。
- 前端提示与实际 Rust 决策保持一致：Desktop 会话显示「深链打开 Claude Desktop」，CLI 会话显示「终端恢复」。

## 验收

### 正常路径

1. Claude Desktop 创建的会话，若 JSONL 最后一个非空 `entrypoint` 为 `claude-desktop-3p`，且只读索引里 `cliSessionId` 命中并给出合法 `local_<uuid>`，则缓存任务 `agent_kind = "desktop"`、`deeplink` 为该固定深链。
2. 该类任务的 `describe_continue` 返回 Deeplink 通道、`locates_session = true`；`plan_continue` 返回 `OpenUrl`，目标为 Claude Desktop。
3. 在 Tauri 窗口点击「打开」后，Claude Desktop 切到前台并定位到对应会话；不启动终端。
4. 原有 Claude CLI 会话仍返回 Terminal 通道，点击后在终端执行 `claude --resume <uuid>`。

### 异常路径

1. Desktop 来源会话缺少本地映射：`agent_kind` 保持 `cli`，`deeplink` 为空，继续工作走终端；warning 不包含会话 ID。
2. 本地映射中的 `sessionId` 不是 `^local_[A-Za-z0-9-]{1,64}$`：视为无有效深链，回退终端；不构造 `session=last`，不复活归档/删除会话。
3. 缓存中已有损坏的 Claude 深链字段：`describe_continue` 与 `plan_continue` 都不承诺定位，统一回退终端。
4. Claude Desktop 本地索引不存在、不可读或 JSON 行损坏：该来源扫描继续，使用 CLI 回退，不影响其他来源。

### 边界条件

1. 混合来源会话：按 JSONL 中最后一个非空 `entrypoint` 判定；最后一个为 `cli` / `sdk-cli` 等时保持 CLI。
2. 只读取 `local_*.json` 的 `sessionId` / `cliSessionId` 两个字段；索引中的标题、摘要、prompt、工具输出等字段不得进入 Perch 快照或日志。
3. 源索引文件只读打开，不修改、不删除、不迁移；测试全部使用临时目录与合成 fixture。
4. `source_id` 仍先经过现有字符集校验；深链 ID 另做严格正则校验；前端不能传入任意 URL 或命令。

## 验证记录

- 2026-11-23：TDD 完成。新增测试覆盖 Desktop 合法映射、缺映射、非法 ID、空 ID、归档映射、混合入口及空 entrypoint、缓存字段透传、变化探测与继续工作决策；全部通过。
- 2026-11-23：`cargo fmt --all --check`、`cargo clippy -p perch-core --all-targets -- -D warnings`、`cargo test -p perch-core`、`npm run check` 全部通过。
- 2026-11-23：实机 `npm run tauri dev` 验证：缓存中 Claude Desktop 会话出现 `claude://code/continue?session=local_…` 深链，详情显示「深链打开 Claude Desktop」；点击后 Claude Desktop 切到前台并打开同一会话。CLI 会话仍显示「终端恢复」。
- 2026-11-23：集成测试从 `main` 建临时分支 `tmp/integration-claude-desktop-session-deeplink` 合入本特性分支，重复 `cargo fmt --all --check`、`cargo clippy -p perch-core --all-targets -- -D warnings`、`cargo test -p perch-core`、`npm run check` 全部通过后删除临时分支。
