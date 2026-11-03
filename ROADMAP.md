# Roadmap

Perch 以“先找到任务，再回 agent 继续”为核心。里程碑沿用
[DESIGN.md](docs/DESIGN.md) 第 15 节，这里记录当前实现进度与下一步。

状态说明：`已完成` / `进行中` / `计划中`。顺序会随反馈调整，但 **只读源数据** 始终是硬约束。

## M1 · 只读聚合与任务列表 — 已完成

- [x] 五个 P0 采集器：Qoder、Claude Code、Codex、OpenCode、Kilo
- [x] `cache.db` 快照（task / task_todo / sync_state / scan_run）
- [x] 完整扫描后的 `source_missing` 判定与防误删保护
- [x] Tauri 2 壳 + `bootstrap` / `refresh_sources` / `continue_task`
- [x] 三栏界面、详情面板、来源筛选
- [x] Rust 单元/集成测试 + `svelte-check`

## M2 · 用户覆盖层（置顶） — 计划中

- [ ] `overlay.db`：只存 Perch 自己的用户数据，扫描永不写入
- [ ] 全局置顶 / 取消置顶，跨来源固定区
- [ ] 重启后保留；源任务删除或迁移后覆盖层不丢失
- [ ] “显示更多 workspace”之外的置顶排序

> 当前界面的置顶徽标只读展示源置顶（如 Qoder sidebar pinned、Kilo 置顶），
> Perch 自己的 pin 尚未实现。

## M3 · macOS 体验与实时性 — 进行中

- [x] 三栏布局、平铺任务列表、行内 workspace
- [x] 最近（7 天）/ 全部 / 已归档视图
- [x] 窗口可见时每 3 秒 probe，变化后自动重扫并刷新
- [x] 手动刷新（`⌘R`）、扫描状态提示
- [x] 来源图标与品牌色、只读脚注
- [ ] 启用 CSP（当前 `tauri.conf.json` 中为 `null`）
- [ ] 打包、codesign、notarize，产出可分发的 `.dmg`
- [ ] 应用菜单、窗口状态记忆等原生细节打磨

## M4 · 覆盖层扩展与效率 — 计划中

- [ ] 人工状态、标签、备注、隐藏、重命名
- [ ] 复制 resume 命令、打开项目目录
- [ ] 搜索（按标题/项目，中文可用）
- [ ] 分页或虚拟滚动（任务量增大后）
- [ ] 导出离线快照（只含元数据）

## M5 · 集成与可选能力 — 计划中

- [ ] 可选读取 usage-ledger 的 `usage.db`，在 Inspector 附带 token / credits（分轨标注，永不相加）
- [ ] Trae CN hooks 事件（仅在安装 hook 后可见，默认关闭）
- [ ] 伴生 CLI（可选）
- [ ] 其他来源的只读适配，先做可行性验证

## 工程化与文档

- [x] GitHub Actions：前端检查/构建 + Rust fmt/clippy/test
- [x] 演示模式（`demo/`）与脱敏截图
- [x] README、DEVELOPMENT、ARCHITECTURE、SOURCES、FAQ、SECURITY、CONTRIBUTING
- [x] Issue / PR 模板、CHANGELOG
- [ ] 发布流程（tag → 构建 → GitHub Release）
- [ ] 依赖更新与安全公告流程

## 当前默认不做（后续可 opt-in）

以下能力不是永久排除，而是当前版本不默认开启；引入时需要独立开关、按源说明与最小化存储：

- 读取消息正文、prompt、tool 输出或 Claude todo 的 description
- 回写 agent（修改源状态、置顶、归档、重命名）
- 解密 Trae 的本地加密数据或调用非公开接口
- 云同步、账号体系、遥测

## 已知风险

- 各 agent 的本地 schema 可能随版本变化；采集器需要持续验证，失败时保留旧快照并提示。
- Qoder 不支持会话级深链，Claude 桌面 App 不能恢复指定会话；这是上游限制。
- 未签名构建在他人机器上可能被 Gatekeeper 拦截。
