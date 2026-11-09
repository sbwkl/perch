# Changelog

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### Added

- 可选只读 usage-ledger 的 `usage.db`：按会话在 Inspector 展示 token 真值、Qoder credits
  与 est 估算，四轨分开展示、永不相加；选中单一来源时，顶部显示该来源的 7 天摘要
  （不同来源单位不同，不跨来源汇总）
- usage-ledger 主库或 `-wal` 变化时自动刷新界面，不触发其 sync
- 侧边栏页脚的用量报告图标：调用 usage-ledger 的 `usage html` 生成 HTML 报告并在浏览器打开
- Qoder 模型键映射为可读名称（如 `dfmodel` → `DeepSeek-Flash`），覆盖 19 个内置模型
- 新增 Trae CN 来源：只读 usage-ledger 的 Trae hook 事件，按 session 聚合会话；
  未安装 hook 时来源显示不可用，继续工作只激活 Trae CN App
- Trae CN 历史会话覆盖：枚举 `~/.trae-cn/memory/projects` 下的
  `session_memory_*.jsonl` 文件名，读取目录、mtime 与首条 `intent` 摘要作为标题，
  不读消息正文；历史会话按 session id 与 hook 事件合并，项目名启发式还原

### Changed

- Trae CN 会话状态改为按时间最晚的 hook 事件推导：`UserPromptSubmit` / `PreToolUse` /
  `PostToolUse` 显示运行中，`SessionStart` / `Stop` 回到空闲
- 无标题会话统一显示「新会话」，不再拼接项目与日期
- 源置顶任务在工作区内优先排序，列表改用图钉图标
- 运行中的会话排在工作区最前，优先于源置顶任务
- Claude Code 会话读取 `~/.claude/sessions/<pid>.json` 实时状态：`busy` 显示运行中、
  `waiting` 显示等待中、`idle` 或进程已退出显示空闲，不再统一显示“无显式状态”；
  探针纳入 session 状态文件，状态变化自动刷新
- 「全部任务」改为「更早」，只显示近 7 天窗口之外的未归档任务，与「最近」互为补集；
  workspace 与来源筛选改为叠加在当前视图上
- 未关联项目的 workspace 固定排在最后
- Kilo 来源图标改为官方像素风格
- 应用图标按 macOS 图标网格重新导出，Dock 尺寸与系统图标一致

### Removed

- 不再展示标题来源（列表徽标与详情字段）
- 详情面板在无待办时不再显示待办区块

### Fixed

- Codex 会话时间改为取 `session_index.jsonl`、rollout 最新活动时间与创建时间的最大值，续用旧会话后不再显示为创建日期

### Planned

- `overlay.db` 与全局置顶（M2）
- CSP 加固、签名/公证与发布流程

## [0.1.0] - 2026-11-03

首个可用版本：只读聚合五个 agent 的本地任务，并提供统一的查看与继续工作入口。

### Added

- Qoder、Claude Code、Codex、OpenCode、Kilo 五个只读采集器
- `cache.db` 快照、完整扫描后的缺失判定、单源故障隔离与防误删保护
- Tauri 2 桌面壳：`bootstrap` / `refresh_sources` / `continue_task`
- 窗口可见时每 3 秒探测源变化，变更后自动重扫并刷新
- 三栏界面：最近（7 天）/ 全部任务 / 已归档，workspace 与来源筛选
- 平铺任务列表，行内显示 workspace；来源侧边栏带品牌图标
- 详情面板：模型、分支、消息数、标题来源、待办进度、恢复命令
- “打开”来源：Qoder 激活 App、Claude/Codex CLI 终端恢复、Codex/OpenCode/Kilo App 打开或终端恢复
- Rust 单元/集成测试（采集器、存储、probe、继续工作决策）
- 开发演示模式（`demo/`）与脱敏文档截图
- 开发、架构、来源规格、FAQ、安全与贡献文档，以及 GitHub CI

### Fixed

- Qoder 运行状态在 turn_states 为空时改用“最后一条消息是否等待回复”的兜底规则
- Codex 运行状态按 rollout 尾部事件与 24 小时窗口推断，避免把历史会话标为运行中
- 已归档列表改为平铺，避免分组标题占用空间
- Qoder 来源图标改为官方 favicon

[Unreleased]: https://github.com/sbwkl/perch/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/sbwkl/perch/releases/tag/v0.1.0
