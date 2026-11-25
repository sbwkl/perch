# AGENTS.md

本目录是 `Perch`——macOS 上的本地多 agent 任务总览：只读汇总 Qoder、Claude Code、Codex、OpenCode、Kilo、Trae CN、Cline 等本地会话元数据，找到任务后回到对应 agent 继续。

## 硬性规则

1. **只读源数据**：源 SQLite 只用 `SQLITE_OPEN_READ_ONLY` 并设 `PRAGMA query_only=ON`；源 JSONL / JSON 只读打开。不得写入、重命名、删除任何 agent 源文件。
2. **只写自己的缓存**：扫描路径只写 Perch 自己的 `cache.db`，不持有任何源写连接；删除 `cache.db` 只能导致重建，不能影响 agent。
4. **口径不可混加**：token / credits / est / activity 四轨分表分示，任何两轨都不相加；不同来源不做跨来源汇总。
5. **命令边界固定**：`continue_task` 只接受任务 key，由 Rust 侧固定模板决定 `open` 的会话深链 / App 激活；session id 必须先做字符集校验，前端不能传入任意 shell 命令或 URL；继续工作不启动终端。
7. **最小 IPC**：前端只调用窄命令；不引入远程页面 / 字体 / 图片 / 脚本，不启用 Tauri `fs` / `shell` / `http` 插件；`{@html}` 仅用于内联固定来源图标。
8. **隐私与 fixture**：测试与文档截图只能用合成数据；不得提交真实会话标题、项目路径、数据库或日志。日志 / PR 贴出前先脱敏。
9. `main` 是保护分支，只接受合并，不直接提交；**任何分支合并进 main 都必须先征得用户同意**
10. **需求先行**：产品行为变化先更新对应 `docs/product-specs/` 文档；新功能 / 跨模块行为变更先建变更文档并链接对应产品规格，验收覆盖正常 / 异常 / 边界后再写代码。历史 `REQ-xxxx` / `CHG-xxxx` / `ADR-xxxx` 编号冻结，不再为新文档分配顺序编号；新增文档使用语义化主题路径（变更文档用 `YYYY-MM-DD-<主题>.md`，开发中放 `docs/exec-plans/active/`，验证后移入 `docs/exec-plans/completed/`）。

## 核心设计要点（已定，勿反复）

- **只读聚合**：各 agent 自己落盘的元数据是真值；Perch 的 `cache.db` 是可重建的快照索引。
- **来源事实源**：当前支持 Qoder、Claude Code、Codex、OpenCode、Kilo、Trae CN、Cline、Antigravity、MiniMax Code、CodeArts、CodeBuddy；新增来源按 `ARCHITECTURE.md` 扩展点实现 `Collector`、路径、probe、continue、前端图标与测试。
- **用量可选**：只读 usage-ledger 的 `usage.db`，按 `session_id` 关联；报告仍由 usage-ledger 自己渲染。四轨分开展示，永不相加。
- **实时性**：窗口可见时检查 SQLite `data_version`，节流计算 JSONL / 日志签名；变化经去抖 / 冷却后只重扫到期来源，可暂停实时刷新；`⌘R` 手动刷新优先。
- **搜索**：P0 元数据搜索已实现（标题、项目、路径、待办、模型、分支、状态、来源、类型、会话 ID；中文 / 拼音 / 拉丁模糊匹配）；P1 内容搜索仅为 opt-in 设计，未实现。
- **覆盖层未实现**：置顶、标签、备注、隐藏、重命名等只写独立 `overlay.db`，扫描永不写覆盖层（见 ROADMAP）。
- **当前边界**：v0.1.0，仅 macOS（Apple Silicon 实测）；未启用 CSP（`csp: null`）；未 codesign / notarize；无 Windows / Linux 计划。

## 文档地图（真相在这里）

| 文档 | 内容 | 何时读 |
|---|---|---|
| `ARCHITECTURE.md` | As-Built 架构总览：代码结构、数据流、只读不变量、扩展点 | 了解整体结构、改 core / Tauri 命令、审计时 |
| `docs/product-specs/` | 产品规格与验收事实源：`usage-report.md`、`agent-sources.md`、`index.md` | 产品行为、需求、评审、开始新特性前 |
| `docs/design-docs/index.md` | 设计知识库索引：状态、领域、当前事实源 | 查设计理由 / ADR / RFC / 历史时 |
| `docs/design-docs/core-beliefs.md` | 核心工程信念：只读、事实源、渐进披露、机械约束 | 沉淀 / 回顾工程价值观时 |
| `docs/design-docs/` | 分层设计知识库：`adr/`、`rfcs/`、`source-designs/`、`history/` | 理解设计理由、查来源规格 / 追溯历史时 |
| `docs/exec-plans/active/` | 进行中变更文档与阶段 spec | 推进阶段实现时 |
| `docs/exec-plans/completed/` | 已验证变更记录；历史编号文档与语义化主题文档并存，状态以文档头为准 | 开发 / 评审时读本次变更文档 |
| `docs/exec-plans/tech-debt-tracker.md` | 技术债账本索引；权威清单见 ARCHITECTURE 与 ROADMAP | 评审、重构、排期时 |
| `docs/references/DATA-PROBE.md` | 本机只读数据探针证据 | 查数据可得性 / schema 边界时 |
| `docs/references/agent-session-deeplinks.md` | 各来源会话级深链调研与证据等级 | 改“继续工作”深链 / App 激活行为时 |
| `docs/FAQ.md` | 常见问题（深链限制、数据位置、卸载等） | 写用户文档或答疑前 |
| `README.md` | 项目定位、覆盖能力、文档与开发约定、门禁 | 首次了解项目、开发前核对约定 |
| `DEVELOPMENT.md` | 环境、命令、数据路径、环境变量、演示模式、调试 | 本地构建 / 测试 / 排障时 |
| `CONTRIBUTING.md` | 只读红线、代码风格、提交 / PR 要求、review 关注点 | 准备提交 / PR 前 |
| `SECURITY.md` | 安全模型、数据处理、已知限制、漏洞报告渠道 | 涉及隐私 / 命令执行 / 发布时 |
| `ROADMAP.md` | 里程碑、当前进度、默认不做事项、已知风险 | 排期或判断特性范围时 |
| `CHANGELOG.md` | 用户可见变更记录 | 发版或修改用户可见行为时 |

## 扩展点：新增一个来源

1. `crates/perch-core/src/collectors/` 新建采集器，实现 `Collector`，返回 `SourceResult`。
2. `models.rs` 增加来源常量与 label；`paths.rs` 增加默认路径和环境变量覆盖。
3. `collectors/mod.rs` 的 `all_collectors` 注册；`probe.rs` 增加变化探测。
4. `continue_work.rs` 定义继续工作通道和固定命令模板。
5. 前端补来源图标 / 标签（`src/lib/source-icons.ts` 与颜色表）。
6. 用临时目录 + fixture 写集成测试；更新 `docs/design-docs/source-designs/SOURCES.md`、`ROADMAP.md`，必要时更新 README / CHANGELOG。

## 常用命令

```bash
npm install              # 安装前端依赖
npm run tauri dev        # 启动 Tauri 开发窗口（Vite 1420 + Rust 后端）
npm run check            # svelte-check 类型检查
npm run build            # 前端生产构建
npm run tauri build      # 打包 Perch.app / .dmg
npm run demo             # 无 Tauri 的 UI 演示 / 截图（端口 1421，假数据）

cargo fmt --all --check
cargo clippy -p perch-core --all-targets -- -D warnings
cargo test -p perch-core
cargo run -p perch-core --example scan   # 用覆盖变量扫描并打印结果
```

## Agent skills

### Issue tracker

issue 与规格存放在 GitHub Issues（`sbwkl/perch`），用 `gh` CLI 读写。详见 `docs/agents/issue-tracker.md`。

### Triage labels

使用默认五个 triage 标签：`needs-triage`、`needs-info`、`ready-for-agent`、`ready-for-human`、`wontfix`。详见 `docs/agents/triage-labels.md`。

### Domain docs

单上下文：根目录 `GLOSSARY.md` + `docs/design-docs/adr/`。详见 `docs/agents/domain.md`。
