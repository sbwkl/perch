# Changelog

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

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
