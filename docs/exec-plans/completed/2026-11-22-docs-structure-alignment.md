# 2026-11-22 文档结构与内容对齐 usage-ledger

| 项 | 内容 |
|---|---|
| 状态 | 已验证 |
| Owner | wayne |
| 所属模块 / 入口 | `AGENTS.md`、`README.md`、`ARCHITECTURE.md`、`DEVELOPMENT.md`、`CONTRIBUTING.md`、`SECURITY.md`、`ROADMAP.md`、`docs/` |
| 关联 | 兄弟项目 `usage-ledger` 的 CHG-0020 ~ CHG-0024 与 “retire sequential document IDs” |

## 1. 摘要

参照兄弟项目 `usage-ledger` 的文档重构结果，把 Perch 的文档从混合目录转为语义分层的事实源结构：`product-specs` / `design-docs` / `exec-plans` / `references`；同步更新地图、入口文档、变更记录与全仓引用。

## 2. 变更理由与证据

- `usage-ledger` 在 CHG-0020 ~ CHG-0024 中建立了文档结构与开发流程规范，并在后续提交中冻结顺序编号、改用语义化主题路径。
- Perch 当前 `docs/requirements/`、`docs/changes/`、根目录 `docs/DESIGN.md` 等路径与文档语义不一致。
- 两个项目共享同样的本地只读数据边界，统一结构有助于跨项目 agent 操作与维护。

## 3. 改动范围

- 需求卡迁移到 `docs/product-specs/usage-report.md` 与 `docs/product-specs/agent-sources.md`。
- 变更记录迁移到 `docs/exec-plans/completed/`，新增 `active/` 与 `tech-debt-tracker.md`。
- 设计文档迁移到 `docs/design-docs/`，分层为 `adr/`、`rfcs/`、`source-designs/`、`history/`，新增 `index.md` 与 `core-beliefs.md`。
- 数据探针迁移到 `docs/references/DATA-PROBE.md`。
- 新增 / 重写 `AGENTS.md`，并同步 README、ARCHITECTURE、DEVELOPMENT、CONTRIBUTING、SECURITY、ROADMAP 与文档内引用。
- 历史 `REQ-xxxx` / `CHG-xxxx` 编号冻结；新增文档使用语义化主题路径。

## 4. 验证

- 命令：`npm run check`、`cargo fmt --all --check`、`cargo clippy -p perch-core --all-targets -- -D warnings`、`cargo test -p perch-core`、本地 Markdown 链接解析检查。
- 环境：macOS（Apple Silicon）；分支 `codex/docs-structure-alignment`。
- 结果：svelte-check 0 errors / 0 warnings；fmt 通过；clippy 无警告；82 个 Rust 测试全部通过；85 个本地 Markdown 链接全部可解析。
- 本次只改文档，无运行时行为变化。

## 5. 回滚

按 git revert 与文件移动反向恢复；无运行时行为变化。
