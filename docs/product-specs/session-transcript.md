# 产品规格：会话对话正文视图

| 项 | 内容 |
|---|---|
| 原需求卡 | 无（语义化主题规格；历史 `REQ-xxxx` 编号冻结） |
| 文档类型 | 产品规格 |
| 当前版本 | v0.1.0 |
| 状态 | 已实现（当前版本行为；变更记录见 [2026-11-26 变更文档](../exec-plans/completed/2026-11-26-ui-convergence-and-transcript-view.md)） |
| 适用范围 | 对话正文视图的入口、内容口径、读取边界与验收 |
| 关联 | [agent-sources.md](agent-sources.md) R22 / R23；[ADR：按需读取会话正文](../design-docs/adr/session-transcript-on-demand-read.md) |

## 1. 摘要

Perch 的详情面板呈现会话元数据；用户在任务行点击对话图标，第 2、3 列整体切换为该会话的**按需**对话正文视图；关闭后回到原来的列表与详情。正文只在用户打开时从本机源文件只读解析，只保留在内存中，不写入 `cache.db`、不写日志、不联网。9 个来源提供正文；Trae CN（只有摘要）与 CodeBuddy（只有日志行）不提供，任务行不显示对话图标。

## 2. 当前事实与边界

- 支持正文的来源（9）：Qoder、Claude Code、Codex、OpenCode、Kilo、CodeArts、Cline、Antigravity、MiniMax Code。不提供正文的来源（2）：Trae CN、CodeBuddy。
- 正文来源事实：Qoder / OpenCode / Kilo / CodeArts 在 SQLite 消息表；Claude Code / Codex / Antigravity / MiniMax Code 在 JSONL transcript；Cline 在 `<id>.messages.json`。Perch 不复制正文，`cache.db` 只保存可重建的源文件定位指针。
- 读取是打开时的单次快照：不随实时刷新自动变化，只能由用户点击「重新读取」重新解析。
- 渲染口径：仅 user 与 assistant 的纯文本；工具调用、tool 输入输出、思考过程、附件不渲染。
- 视图中不出现一次性隐私说明、常驻「只读 / 仅本机内存」小字或全局开关；隐私口径由文档与 ADR 记录。
- 内容搜索（P1）仍为 opt-in 设计、未实现；本规格不引入正文搜索、正文缓存或正文导出。

## 3. 成功指标

- 主指标：9 个正文来源的任意任务都能从任务行进入对话视图并渲染出消息；Trae CN / CodeBuddy 任务行不出现对话图标。
- 护栏指标：`cache.db` 无正文字段；对话关闭后正文不残留在应用状态里；源文件扫描 / 打开前后字节一致；UI 只调用窄命令，前端不传路径或 URL。
- 验证方式：临时目录 fixture 单元测试 + Tauri 命令校验测试 + demo UI 验收（合成对话数据）。

## 4. 当前需求与验收（EARS）

- T1：When 用户在任务行点击对话图标, the system shall 选中该任务并在第 2、3 列展示该会话的对话记录；侧边栏保持可交互，任务列表与详情面板在返回后恢复。
- T2：When 任务所属来源为 Trae CN 或 CodeBuddy, the system shall 不渲染对话图标，也不在详情面板说明缺失原因。
- T3：When 对话视图渲染, the system shall 按时间正序呈现 user 与 assistant 的纯文本消息；只含工具调用的轮次折叠为一行「工具调用 N 次」且不展开内容；不渲染 tool 输入/输出、思考过程、附件或系统注入包装。
- T4：When 会话消息数超过 50, the system shall 默认只加载最近 50 条，并提供「加载更早」每次追加 50 条（该按钮位于消息列表第一行，只有滚到最上面时才可见；剩余为 0 时不在列表中渲染）；When 单条消息超过 8,000 字符, the system shall 截断并标注「已截断」；When 单会话累计正文超过 200,000 字符, the system shall 只保留最近部分并保持消息顺序。
- T5：When 对话首次打开, the system shall 定位到最新一条消息；When 用户加载更早, the system shall 保持当前阅读位置，不强制跳到顶部或底部。
- T6：When 用户点击「返回」或按 `Esc`, the system shall 关闭对话视图并回到打开前的列表与详情（该任务仍为选中态）。
- T7：When 对话视图打开时用户切换任务视图、workspace 或来源筛选, the system shall 关闭对话视图并返回列表。
- T8：When 刷新后该任务不再出现在当前筛选列表中, the system shall 关闭对话视图并清除详情选中，与既有列表选中规则一致。
- T9：When 读取失败、源文件缺失或内容为空, the system shall 在对话面板内显示一行失败 / 空态原因，并复用头部的「重新读取」；不弹额外提示、不自动退出。
- T10：When 用户点击「重新读取」, the system shall 重新按需解析该会话并替换当前内容，保留对话视图与返回路径。
- T11：While 对话视图使用中, the system shall 只读打开源文件，不写入、不重命名、不删除任何源数据；正文只存在于本机内存，关闭视图后丢弃，不写入 `cache.db`、不写日志、不联网。
- T12：When 对话视图打开, the system shall 在头部显示来源图标、标题、状态徽标、「继续工作」、「重新读取」与「返回」，且不显示一次性隐私说明或常驻只读小字。
- 边界：空会话、只有工具轮、消息全为截断超限、子会话、归档任务、源文件读取中被删除、非 UTF-8 / 损坏 JSON 行（跳过该行并继续）、900×560 小窗口、浅色 / 深色主题均须可回到列表，不得卡死或静默白屏。

验证位置：T1–T10 / T12 见 2026-11-26 变更文档的 core / Tauri 测试与 demo UI 验收；T11（只读、内存内、
不落盘）见 core 测试的源文件字节一致断言与 `cache.db` 无正文字段断言；每个来源的字段映射见
[SOURCES.md](../design-docs/source-designs/SOURCES.md)；隐私与读取范围见 [ADR](../design-docs/adr/session-transcript-on-demand-read.md)。

## 5. 变更索引

| 变更 | 标题 | 状态 | 链接 |
|---|---|---|---|
| 2026-11-26 | 三组选中收敛、状态栏 / 提示语删除与会话对话视图 | 已验证 | [../exec-plans/completed/2026-11-26-ui-convergence-and-transcript-view.md](../exec-plans/completed/2026-11-26-ui-convergence-and-transcript-view.md) |

## 6. 链接

- 产品规格索引：[index.md](index.md)
- 来源覆盖与继续工作：[agent-sources.md](agent-sources.md)
- 逐源设计：[SOURCES.md](../design-docs/source-designs/SOURCES.md)
- 搜索与内容边界：[SEARCH.md](../design-docs/source-designs/SEARCH.md)
- 架构与只读不变量：[ARCHITECTURE.md](../../ARCHITECTURE.md)