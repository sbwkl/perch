# FAQ

## Perch 会修改我的 agent 数据吗？

不会。Perch 只用只读连接读取本地会话元数据，不写任何 agent 的数据库或 JSONL，也不修改
置顶、归档、标题或任务状态。Perch 自己的缓存是可删除、可重建的 `cache.db`。

## 数据存在哪里？怎么备份或清理？

- Perch 缓存：`~/Library/Application Support/app.perch.desktop/cache.db`
- 源数据位置见 [SOURCES.md](design-docs/source-designs/SOURCES.md) 或 README 的“支持的来源”。

退出 Perch 后删除 `cache.db`、`cache.db-wal`、`cache.db-shm` 即可清空缓存，下次启动会重新扫描。
不要删除源 agent 的数据。

## 会读取聊天正文或 prompt 吗？

默认不读取。扫描只读标题、项目路径、时间、模型、分支、消息计数、todo 的 subject、状态以及
置顶/未读标志等元数据。两个显式例外：

1. **对话正文视图（按需）**：只有你在任务行点击对话图标时，Perch 才只读打开该会话的源文件，
   解析出 user 与 assistant 的可见文本在本机内存中渲染。它是单次快照：不随实时刷新自动重读，
   只有点「重新读取」才重新解析。正文不写入 `cache.db`、不写日志、不联网，关闭面板即丢弃；
   工具调用只折叠为「工具调用 N 次」，不读取 tool 输入/输出、思考过程或附件。
   9 个来源提供正文；Trae CN（只有摘要）与 CodeBuddy（只有日志行）没有对话图标。
2. **Claude 标题派生**：会话缺少原生标题时，Perch 会读取首条候选用户文本，在内存中清洗为
   单行标题后只保存标题结果，不保存正文。

标题可能包含 prompt 片段；对话正文只留在本机内存中，两者都不会发送到任何地方。

## 需要联网吗？

不需要。Perch 不发起网络请求，没有账号、遥测或云同步。界面不加载远程字体、图片或脚本。

## Trae CN 的任务从哪里来？

Trae CN 的会话列表在服务端，本地 agent 数据库是加密 SQLite，Perch 不读取也不解密。
Perch 只读 usage-ledger 的 Trae hook 事件（`~/.local/share/usage-ledger/trae-cn/events-*.jsonl`）
和 Trae 本地会话记忆的文件名与首条 `intent` 摘要（`~/.trae-cn/memory/projects/`）。
需要先安装 hook：

```bash
usage hooks install trae-cn --scope global
```

未安装时该来源显示不可用。hook 只记录安装之后的会话，更早的历史会话从会话记忆文件名补齐。
标题优先取 memory 首条 `intent` 摘要，没有 memory 记录时显示“新会话”；
状态由最新 hook 事件推导（`running` / `idle`），历史会话固定 `idle`；
workspace 名从编码目录名启发式还原，不伪造完整路径。

## Qoder 的“打开”能定位到具体会话吗？

能。Qoder 注册了 `qoder://`，并支持会话级深链 `qoder://chat/open?sessionId=<id>`。
Perch 固定生成该链接，点击后切到目标会话。旧的 `qoder://chat/<id>` 不受支持
（Qoder 日志报 `DEEP_LINK_ROUTE_UNSUPPORTED`），Perch 不使用它。
会话 ID 不合法或超过 128 字符时，Perch 回退为仅激活 Qoder App，按钮相应显示「激活 Qoder」。
证据见 [agent-session-deeplinks.md](references/agent-session-deeplinks.md)。

## Claude Code 的运行状态是怎么来的？

Perch 只读 `~/.claude/sessions/<pid>.json`：Claude 进程忙碌时状态为 `busy`，显示“运行中”；
等待时为 `waiting`，显示“等待中”；空闲或进程已退出显示“空闲”。Claude 不提供会话完成/归档状态，
Perch 不臆造 `done`。待办进度来自 `~/.claude/tasks/<sessionId>/`。

## 为什么 Trae CN 没有任务？ChatGPT 桌面会话去哪了？

- Trae CN：本机 `state.vscdb` 只有模型配置，任务索引为空；没有稳定可读的本地任务列表。
- ChatGPT 桌面：仅 Codex 会话写入 `~/.codex`，计入 Codex 来源；普通聊天存于云端，没有本地任务数据。

没有数据就不会显示在侧边栏。若未来出现稳定的只读数据源，会作为独立采集器加入（见 ROADMAP）。

## 为什么列表里看不到子会话或自动化任务？

子会话（Qoder `side_chat`、OpenCode/Kilo `parent_id`、Codex `subagent` / `tui_side_chat`）和
Codex 自动化线程默认不进入主列表，也不计入来源 / 工作区 / 最近 / 更早的计数，避免任务碎片化。
它们仍保存在快照里，可被搜索找到，详情面板与搜索结果可显示“子会话”“自动化”徽标。
例外：Claude Code 的子代理文件（`subagents/`）只计入变化探测的文件数，不进快照，
因此既搜索不到，也不会出现在详情面板里。

## 会话没有标题时显示什么？

Claude Code 按标题优先级取原生标题、人工标题 sidecar，或由首条候选用户消息派生单行标题；
都不存在时使用 `新会话`。Perch 不会调用模型自行命名：OpenCode 的 `session_v2.title` 在部分会话里为空
（标题生成没有发生或失败），Qoder 新会话也可能暂时没有正式标题，此时统一使用该占位标题。
Codex 的子代理会话缺少正式标题时，用其代理昵称派生「子代理 · <昵称>」，便于在搜索中分辨它来自哪个子代理。

## “源已删除”是什么意思？

上一次完整扫描成功时存在、但本次扫描没有出现的任务会被标记 `source_missing`。
这不代表 Perch 删除了任何东西；源任务可能被 agent 清理或迁移。失败/部分扫描不会触发该标记。

## 为什么某次刷新后任务数突然变少？

如果来源上次有超过 20 个任务、本次结果不足一半，Perch 会认为可能异常，降级为部分扫描并保留旧快照，
不会把大量任务标记为缺失。确认源数据正常后再手动刷新一次即可。

## 详情面板的“打开 / 激活”按钮会执行什么？

见 [ARCHITECTURE.md](../ARCHITECTURE.md) 的“继续工作”一节。动作由 Rust 侧按来源固定模板生成，
session id 会做字符集校验，前端不能传入任意命令。Qoder 会话与 Codex 桌面会话使用会话级深链，
按钮写作「打开 X」；其他来源只激活对应 App，按钮写作「激活 X」，目前不会启动终端或 CLI 恢复。
按钮文案的含义见 [GLOSSARY.md](../GLOSSARY.md)。

## 支持 Windows / Linux 吗？

目前只支持 macOS。采集路径与 App 激活都按 macOS 实现。

## 有现成的安装包吗？

还未提供签名/公证版本。可从 GitHub Release 下载未签名的 `.dmg`，或从源码构建：

```bash
npm install
npm run tauri build
```

产物在 `target/release/bundle/macos/Perch.app` 与 `target/release/bundle/dmg/*.dmg`，未签名版本首次打开需右键 → 打开。

## 如何卸载？

1. 退出 Perch。
2. 删除 `Perch.app`。
3. 可选：删除 `~/Library/Application Support/app.perch.desktop/`。

不会影响任何 agent 的数据。

## 如何新增一个来源？

参考 [ARCHITECTURE.md](../ARCHITECTURE.md) 的“扩展点：新增一个来源”。新采集器必须保持只读，
用临时 fixture 写测试，并更新 [SOURCES.md](design-docs/source-designs/SOURCES.md)。
