# Roadmap

Perch 以“先找到任务，再回 agent 继续”为核心。里程碑沿用
[v0.3.3-design-review.md](docs/design-docs/history/v0.3.3-design-review.md) 第 15 节，这里记录当前实现进度与下一步。

状态说明：`已完成` / `进行中` / `计划中`。顺序会随反馈调整，但 **只读源数据** 始终是硬约束。

## M1 · 只读聚合与任务列表 — 已完成

- [x] 六个来源的采集器：Qoder、Claude Code、Codex、OpenCode、Kilo、Trae CN
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
- [x] 最近（7 天）/ 更早 / 已归档视图
- [x] 窗口可见时 probe、变化去抖 / 冷却后重扫并刷新，实时刷新常开（无暂停入口；`set_live_refresh` 保留为后端能力）
- [x] 手动刷新（`⌘R`）优先于后台扫描；扫描状态栏与耗时提示已按界面收敛删除
- [x] 来源图标与品牌色、只读脚注
- [ ] 启用 CSP（当前 `tauri.conf.json` 中为 `null`）
- [x] 打包 `.dmg` 并发布 GitHub Release
- [ ] codesign / notarize（等待 Developer ID 证书与协议确认）
- [ ] 应用菜单、窗口状态记忆等原生细节打磨

## M4 · 覆盖层扩展与效率 — 计划中

- [ ] 人工状态、标签、备注、隐藏、重命名
- [ ] 复制 resume 命令、打开项目目录
- [x] 搜索（元数据 + 拼音 + 模糊；含归档全部会话；全量命中 + 分页展示）
- [x] 分页（任务量增大后按 60 条展开；搜索结果按 50 条展开）
- [ ] 导出离线快照（只含元数据）

## M5 · 集成与可选能力 — 计划中

- [x] 可选读取 usage-ledger 的 `usage.db`，在 Inspector 附带 token / credits / est（分轨标注，永不相加）
- [x] 会话对话正文视图：9 个来源按需、只读、内存内读取（单次快照、8k / 200k 预算；见 `docs/product-specs/session-transcript.md`）
- [ ] 用量报表视图（按 workspace / 时间聚合图表，复用 usage-ledger 的聚合口径）
- [ ] 会话内容搜索（opt-in、默认关闭、仅本机内存；见 `docs/design-docs/source-designs/SEARCH.md` P1）
- [x] Trae CN hooks 事件聚合（需安装 usage-ledger hook；未安装时来源不可用）
- [x] Cline 来源：只读 CLI 与桌面 App 共用的 `sessions.db`；继续工作只激活 Cline App（无会话级深链，不启动终端）
- [ ] 伴生 CLI（可选）
- [ ] 其他来源的只读适配，先做可行性验证

## 工程化与文档

- [x] GitHub Actions：前端检查/构建 + Rust fmt/clippy/test
- [x] 演示模式（`demo/`）与脱敏截图
- [x] README、DEVELOPMENT、ARCHITECTURE、SOURCES、FAQ、SECURITY、CONTRIBUTING
- [x] Issue / PR 模板、CHANGELOG
- [x] 发布流程（tag → 构建 → GitHub Release；当前为未签名包）
- [ ] 依赖更新与安全公告流程

## 当前默认不做（后续可 opt-in）

以下能力不是永久排除，而是当前版本不默认开启；引入时需要独立开关、按源说明与最小化存储：

- 正文后台预读、正文索引、正文导出与全文搜索（正文只在打开对话视图时按需读取）
- 读取 prompt、tool 输出或 Claude todo 的 description（对话视图与标题派生之外不扩大读取面）
- 回写 agent（修改源状态、置顶、归档、重命名）
- 解密 Trae 的本地加密数据或调用非公开接口
- 云同步、账号体系、遥测

## 已知风险

- 各 agent 的本地 schema 可能随版本变化；采集器需要持续验证，失败时保留旧快照并提示。
- Claude 桌面 App 不能恢复指定会话；Codex 桌面以外的会话与其余来源只激活 App，不能定位具体会话。这是上游限制。
- Qoder 会话深链依赖私有路由 `qoder://chat/open`，上游改版可能失效；失效时 App 自行报错，Perch 不预判。
- 未签名构建在他人机器上可能被 Gatekeeper 拦截。
- 继续工作按钮的对比度选择硬编码深色字亮度 `0.0104`（`#1d1d1f` 实际约 `0.01239`），代码阈值与
  [2026-11-24-continue-work-affordance.md](docs/exec-plans/completed/2026-11-24-continue-work-affordance.md)
  的验证表不一致；当前 11 个来源的前景选择结果不变。
- 继续工作按钮只在白色与深色字之间取更优对比度，没有 4.5:1 下限；未来新增中间调品牌色的来源可能达不到 AA。
- 继续工作请求在途时切换选中任务，加载文案与失败原因会显示到新任务上（`continuing` 是全局布尔，
  `catch` 未校验 `selectedKey`）；失败提示未使用 `aria-live`，读屏用户可能感知不到。
- CI 只对 `src-tauri` 执行 `cargo fmt`，不编译该 crate；Tauri 壳的类型错误需本地 `cargo check -p perch` 发现。
- `demo/fixture.ts` 在模块顶层构造数据，依赖被引用的常量全部声明在其上方；`npm run check` 无法捕获 TDZ，
  回归仍需实跑 `npm run demo`。
- `docs/references/agent-session-deeplinks.md` 的验证建议仍写「在 UI 明示无法定位」，当前由按钮动词与
  [GLOSSARY.md](GLOSSARY.md) 表达。
