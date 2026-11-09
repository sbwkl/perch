# Security Policy

## 支持的版本

| 版本 | 支持状态 |
| --- | --- |
| 0.1.x | 当前开发版本，接受安全报告 |
| < 0.1 | 未发布 |

## 报告漏洞

请使用 GitHub 的私密漏洞报告：

https://github.com/sbwkl/perch/security/advisories/new

不要在公开 Issue 中粘贴真实会话标题、项目路径、数据库内容或任何凭据。
报告时请尽量提供：影响版本、复现步骤、影响范围，以及必要的脱敏日志。

## 安全模型

Perch 是本机单用户的只读查看器：

- 不发起网络请求，没有遥测、账号或云同步。
- 不启用 Tauri 的 `fs` / `shell` / `http` 等插件；前端只能调用五个窄命令：
  `bootstrap`、`refresh_sources`、`search_tasks`、`continue_task`、`open_usage_report`。
- `search_tasks` 只读 `cache.db` 中已有的元数据与待办，不读取会话正文；搜索词只在内存中使用，
  不写入数据库、不写日志、不进入错误信息；只返回命中任务 key、分数与字段标签。
- 所有源读取在 Rust core 中完成；源 SQLite 使用只读连接和 `PRAGMA query_only=ON`，
  JSONL 只读打开。usage-ledger 的 `usage.db` 同样只读打开，Perch 不写入、不迁移、
  不触发它的 sync。
- `continue_task` 只接受任务 key，由 Rust 侧按来源固定模板决定 `open`、URL scheme 或终端命令；
  session id 先经过字符集校验，前端不能传入任意命令。
- 页面不加载远程页面、字体、图片或脚本；`{@html}` 只用于内联的固定来源图标常量，
  不渲染任务字段。

## 数据处理

**读取并保存在本机 SQLite 的元数据**：会话标题、项目路径、时间、模型、分支、消息计数、
todo 的 subject、状态、置顶/未读标志、恢复命令。用量块额外读取 usage-ledger 的来源、
会话 ID、模型、日期、token / credits / est 计数；这些值不写入 `cache.db`，只随
`bootstrap` 返回给前端。

Trae CN 只读 usage-ledger hook 事件中的 `session`、`ts`、`project`（basename）与 `model`；
并为历史会话读取 `session_memory_*.jsonl` 首条记录的 `intent` 摘要作为标题。
`tool`、`tool_use_id`、`loop`、`chars_in`、`chars_out` 等字段不读取、不存储；
memory 的 `actions`、`outcome`、`learned` 与消息正文也不读取。

**不读取**：消息正文、prompt、assistant 输出、tool 输入/输出、附件、思考过程、账号凭据或密钥。

缓存位置：`~/Library/Application Support/app.perch.desktop/cache.db`。数据库不加密，
与本地用户账号的隐私边界一致；能读取该用户目录的进程本来也能读取源 agent 数据。
标题由 agent 生成，可能包含 prompt 片段，请勿公开分享包含真实会话的截图。

## 已知限制

- 当前未启用 CSP（`tauri.conf.json` 中 `csp: null`）。前端没有远程资源，但加固仍在
  [ROADMAP](ROADMAP.md) 中。
- 本地构建未 codesign / notarize；从源码构建时应自行确认来源。
- 终端恢复依赖 `osascript` 调用 Terminal；命令模板是固定的，但仍会在用户终端中执行，
  请确认 session id 与项目路径来自可信的本地 agent 数据。
- 第三方 agent 的本地 schema 可能变化；采集失败时 Perch 保留旧快照，不会写源。

## 不在范围内

- 已被入侵的本地账号或恶意 agent 写入的伪造数据。
- 第三方 agent 自身的安全问题（请报告给对应项目）。
- 未签名构建被 Gatekeeper 拦截等安装体验问题（可开普通 Issue）。
