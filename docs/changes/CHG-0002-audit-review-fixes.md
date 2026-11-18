# CHG-0002 审计复核项修复（7 项）

- **状态**：已验证
- **日期**：2026-11-18
- **所属模块/入口**：perch-core（usage / probe / store / collectors::cline）、src-tauri（lib.rs）、前端（App.svelte、demo、构建配置）
- **分支**：`feat/audit-review-fixes`（基于 main `1b3d50c`）
- **关联**：CHG-0001（PR #3，未合并）列出的 7 项待审核项；用户授权由执行者决定修改方式

## 理由

CHG-0001 审计后留下 7 项需要决策的问题。逐项决定如下并实施：

1. **trae-cn.est 用量被丢弃** → 纳入。usage 桥接的白名单扩为 6 个采集源（qoder/claude/codex/opencode/kilo/trae-cn）；`usage_est` 按行内 `source` 归因（旧 schema 无 source 列时回退 `'qoder'`），trae-cn.est 行计入 Trae CN 的估算轨。四轨（token 真值/credits/估算）仍不相加。
2. **credits 全部记到 qoder** → 按 `usage_credit.source` 列分组归因；旧 schema 无该列时回退 `'qoder'`（与既有 legacy 容错模式一致）；非 Perch 采集源的行忽略。
3. **SQLite 探测连接启动后不再重建** → `SqliteProbe` 持有候选路径列表；连接缺失或 `data_version` 读取失败时丢弃句柄，下轮 poll 重新打开（数据库可能在 Perch 启动后才出现）。
4. **scan_run 无限增长** → `finish_scan` 后按源裁剪，每源保留最近 200 条（诊断用途上限）。
5. **continue_task 全表加载** → 新增 `CacheStore::load_task(key)` 单行查询，命令改用之。
6. **usage mtime 逻辑重复** → 删除 src-tauri 本地 `usage_mtime`，统一用 `perch_core::usage::newest_mtime`（改为 pub）。
7. **Cline resume_cmd 与实际执行不一致** → `resume_cmd` 改用过滤后的 `project_path` 生成；内置聊天工作区（`~/.cline/data/workspaces/`）不再出现在 `cd` 目标中，展示为裸命令 `cline --id <id>`，与 `continue_task` 实际执行一致。同步更新 docs/SOURCES.md。

附带小修（同属审计杂项）：

- 搜索页脚 “仅显示前 N 条” → “显示 N 条”（结果已按 payload 过滤，旧文案暗示截断）。
- demo `continue_task` 桩消息按任务来源显示名称，不再硬编码 Codex。
- demo/social.html 补齐 Trae CN、Cline 两个来源芯片与导语（与 README 七源一致）。
- vite HMR 端口 1421 → 1422，避免与 demo dev server（1421）在 `TAURI_DEV_HOST` 场景冲突。
- tsconfig include 纳入 `demo/**/*.ts`（exclude 各 vite.config.ts，与根配置此前不被检查一致），svelte-check 覆盖 demo 代码。

**明确保留**：session_db 采集器内的 todo 计数不删除——collectors 测试断言采集输出携带 todo_total/todo_done，store 覆写属设计行为，非冗余。

## AC delta

无 REQ（一次性修复集合）。行为验收：

- trae-cn 估算出现在 bootstrap usage 概览与任务明细，来源标记 `trae-cn.est`。
- credits 按 source 归因；未知来源行不进入任何源。
- 启动时不存在的 SQLite 源，出现后下一轮 poll 即被探测到。
- 每源 scan_run ≤ 200，且不跨源误删。
- `cline` 内置聊天会话的 resume_cmd 为 `cline --id <id>`（无 cd）。

## 验证（2026-11-18，本机）

- TDD：usage 3 例、probe 1 例、store 2 例、cline 1 例先红后绿。
- `cargo fmt --all`、`cargo check --workspace`、`cargo clippy --all-targets -- -D warnings` 零告警。
- `cargo test --workspace`：61 通过（2+20+5+16+5+8+5）。
- `npm run check`：0 errors 0 warnings；`npm run build` 成功。
- 集成测试：tmp/integration-audit-review-fixes（见提交记录）。

## 回滚

单分支 revert 即可；无 schema 迁移、无数据写入变更（scan_run 裁剪仅删诊断日志）。