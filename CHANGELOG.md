# Changelog

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### Changed

- 任务列表为对话图标保留固定列宽，混排有 / 无正文来源时状态胶囊保持同一列；详情与对话头部的继续工作按钮统一为对比度达标的中性样式（浅 / 深主题 ≥ 4.5:1，文案、hover、通道逻辑不变）；会话详情移除 usage-ledger 用量区块与只读来源数据脚注。

### Added

- 会话对话正文视图：任务行点击对话图标，第 2、3 列整体切换为该会话的对话记录；9 个来源提供
  正文（Trae CN / CodeBuddy 不提供，不显示图标），消息按时间正序，工具调用折叠为「工具调用 N 次」，
  单条超 8,000 字符截断标注、单会话超 200,000 字符只保留最近部分，默认最近 50 条；用户消息以
  靠右气泡呈现（最大宽度约 76%，内文左对齐），助手消息与工具调用折叠行保持原样；「加载更早」
  位于消息列表第一行、滚到顶部才可见，每次追加 50 条并锚定阅读位置；正文按需、只读、内存内读取
  （单次快照），不写入 `cache.db`、不写日志、不联网（[ADR：按需读取会话正文](docs/design-docs/adr/session-transcript-on-demand-read.md)）。
- 新窄命令 `read_session_content`：只接受任务 key（与 `continue_task` 同规校验），
  路径由 Rust 侧解析，前端不传路径或 URL；`cache.db` 的 `task` 表新增可重建的
  `source_path` 定位指针列（`user_version` 1 → 2 幂等迁移，删库重建即恢复）。
- Codex 子代理会话与侧聊会话识别为子会话：默认不进入主列表、不计入来源 / 工作区 / 最近 / 更早计数，
  仍保留在快照中可被搜索，详情与搜索结果带「子会话」徽标；子代理缺少正式标题时用代理昵称
  派生「子代理 · <昵称>」标题。
- Claude Code 会话标题优先级：JSONL `custom-title` → `custom-title.json` sidecar → JSONL `ai-title` → 首条候选用户消息派生标题 → `新会话`；派生标题单行、折叠空白并截断到 60 字符，不使用 `agentName`、`summary` 或 `history.jsonl`。
- 继续工作入口收敛为一个按钮：动词表明通道（`打开 Qoder` / `打开 Codex` 能定位到具体会话，`激活 Trae CN` 等只把 App 切到前台），按钮用来源品牌色填充，删除重复的通道胶囊与说明段；点击成功不再弹出提示，失败显示危险色错误。
- Qoder 会话深链：`qoder://chat/open?sessionId=<id>` 精确打开目标会话；session id 按 `^[A-Za-z0-9_-]{1,128}$` 校验，不合法时回退激活 Qoder App。
- 继续工作收敛为 App 通道：Qoder 与 Codex 桌面会话走会话级深链，其余来源（含 Claude、OpenCode、Kilo、Cline、Trae CN 等）只激活对应 App，不再启动终端或 CLI 恢复。

- 删除第二栏顶部 `Workspace：… ×` / `来源：… ×` 筛选 chip；筛选状态仅由侧栏选中态表达，搜索弹层作用域提示保留。
- 删除 workspace 名称后的路径父目录拼接，Workspace 按钮只显示源数据名称；同名 workspace 不再追加 ` · 路径父目录`。
- 删除 workspace 名称后的 `[临时目录]` / 分组后缀；路径类型不再影响侧栏排序，低活跃项由近 7 天排序与折叠自然后置。
- 侧栏 Workspaces / 来源按近 7 天会话数倒序，同数按最近活动倒序；来源超过 8 项可折叠，当前选中项始终可见。
- P0 / P1 真实体验修复：live probe 节流 / 去抖 / 冷却、实时刷新暂停与扫描状态；手动刷新优先于后台扫描；任务 / 搜索分页、全量搜索结果、筛选联动计数与 workspace 消歧。侧边栏随后收敛为简洁导航，不显示待关注 / 用量报告导航项、workspace 搜索框、来源触发 / 更新时间、来源说明或只读采集脚注。
- 真实数据体验反馈后补齐当前状态说明：11 个来源、应用内用量报告与四视图；视图 / 筛选切换后清除列表外选中任务；搜索显示当前作用域；来源状态与最近扫描可读；报告视图增加导航态与返回路径。
- Cline 来源：只读 `~/.cline/data/db/sessions.db` 的会话元数据（Cline CLI 与桌面 App 共用），
  展示标题、项目、状态、模型与父子会话；继续工作只激活 Cline App（无会话级深链）；
  窗口可见时通过 `PRAGMA data_version` 探测变化
- 会话状态新增 `failed` 映射与危险色显示，用于 Cline 的失败会话

### Changed

- 界面收敛：任务 / Workspaces / 来源三组各自单选（`aria-pressed` 表达选中态），任一操作后至少一组
  保持选中；选中项改为柔色强调底、加粗标签、强调色文字与计数（对比度 ≥ 4.5:1），不再使用实底、
  反白计数或左侧色条，来源组也不按品牌色区分选中文字。
- 删除列表可见的扫描状态栏与提示语：状态文案、进度点、「暂停实时 / 恢复实时」、「扫描 N 个来源 · N ms」、
  「用量可能已过期 / 用量快照 X」、「匹配 N 个会话」；实时刷新常开、没有暂停入口，
  `set_live_refresh` 保留为后端能力但前端不再调用，扫描进度只由刷新按钮「扫描中」文案表达。
- 侧栏品牌位、详情空态与 demo 社交图统一使用 app icon 的矢量内联渲染，删除线描鸟图标。

### Fixed

- 详情面板通道胶囊被全局 `.app` 网格样式撑满面板（实测 542×694 px），把详情内容挤出首屏；随入口收敛一并移除该元素。
- `npm run demo` 因 `demo/fixture.ts` 的 `UUID_PATTERN` 在初始化前被读取而白屏（独立变更 [2026-11-24-demo-fixture-tdz.md](docs/exec-plans/completed/2026-11-24-demo-fixture-tdz.md)）。

### Planned

- `overlay.db` 与全局置顶（M2）
- CSP 加固、签名/公证与发布流程

## [0.1.0] - 2026-11-09

首个可用版本：只读聚合六个来源的本地任务（Qoder、Claude Code、Codex、OpenCode、Kilo、Trae CN），
提供统一的查看、搜索与继续工作入口。

### Added

- Qoder、Claude Code、Codex、OpenCode、Kilo、Trae CN 六个来源的只读采集器
- `cache.db` 快照、完整扫描后的缺失判定、单源故障隔离与防误删保护
- Tauri 2 桌面壳：`bootstrap` / `refresh_sources` / `continue_task` / `search_tasks` / `open_usage_report`
- 窗口可见时每 3 秒探测源变化，变更后自动重扫并刷新
- 三栏界面：最近（7 天）/ 更早 / 已归档，workspace 与来源筛选
- 平铺任务列表，行内显示 workspace；来源侧边栏带品牌图标
- 详情面板：模型、分支、消息数、待办进度、恢复命令
- 元数据搜索：侧边栏「任务」分组内、「已归档」下方新增搜索入口（⌘F），点击后打开居中弹层，
  输入框自动聚焦；覆盖标题、项目、路径、待办、模型、分支、状态、来源、
  类型、会话 ID；支持中文子串、拼音全拼/首字母、拉丁子序列与拼写容错；多词 AND；
  默认搜索全部会话（含归档/子会话/自动化）；结果按字段权重排序，返回前 10 条并显示真实命中数；
  支持 `↑`/`↓` 选择、`Enter` 打开、`Esc` 或点击遮罩关闭
- 可选只读 usage-ledger 的 `usage.db`：按会话在 Inspector 展示 token 真值、Qoder credits
  与 est 估算，四轨分开展示、永不相加；选中单一来源时，顶部显示该来源的 7 天摘要
  （不同来源单位不同，不跨来源汇总）
- usage-ledger 主库或 `-wal` 变化时自动刷新界面，不触发其 sync
- 侧边栏页脚的用量报告图标：调用 usage-ledger 的 `usage html` 生成 HTML 报告并在浏览器打开
- Qoder 模型键映射为可读名称（如 `dfmodel` → `DeepSeek-Flash`），覆盖 19 个内置模型
- Trae CN 来源：只读 usage-ledger 的 Trae hook 事件，按 session 聚合会话；
  未安装 hook 时来源显示不可用，继续工作只激活 Trae CN App
- Trae CN 历史会话覆盖：枚举 `~/.trae-cn/memory/projects` 下的
  `session_memory_*.jsonl` 文件名，读取目录、mtime 与首条 `intent` 摘要作为标题，
  不读消息正文；历史会话按 session id 与 hook 事件合并，项目名启发式还原
- “打开”来源：Qoder 激活 App、Claude/Codex CLI 终端恢复、Codex/OpenCode/Kilo App 打开或终端恢复
- Rust 单元/集成测试（采集器、存储、probe、搜索、继续工作决策）
- 开发演示模式（`demo/`）与脱敏文档截图
- 开发、架构、来源规格、FAQ、安全与贡献文档，以及 GitHub CI

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
- 已归档列表改为平铺，避免分组标题占用空间
- 未关联项目的 workspace 固定排在最后
- Kilo 来源图标改为官方像素风格
- Qoder 来源图标改为官方 favicon
- 应用图标按 macOS 图标网格重新导出，Dock 尺寸与系统图标一致

### Removed

- 不再展示标题来源（列表徽标与详情字段）
- 详情面板在无待办时不再显示待办区块

### Fixed

- Qoder 运行状态在 turn_states 为空时改用“最后一条消息是否等待回复”的兜底规则
- Codex 运行状态按 rollout 尾部事件与 24 小时窗口推断，避免把历史会话标为运行中
- Codex 会话时间改为取 `session_index.jsonl`、rollout 最新活动时间与创建时间的最大值，续用旧会话后不再显示为创建日期
- 打开搜索结果后侧边栏与任务列表未跟随：现在会切换到该会话所在视图与 workspace，
  清除来源筛选，在列表中滚动并高亮该行；子会话/自动化命中也会临时显示并带徽标
- `npm run tauri dev` 持续 200%+ CPU：Vite 文件监听把 6.9 GB 的 Rust `target/` 目录
  也纳入递归监听；忽略 `target/` 后空闲占用回到约 1%（Tauri 运行时本身约 0–2%）

[Unreleased]: https://github.com/sbwkl/perch/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/sbwkl/perch/releases/tag/v0.1.0
