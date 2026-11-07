# FAQ

## Perch 会修改我的 agent 数据吗？

不会。Perch 只用只读连接读取本地会话元数据，不写任何 agent 的数据库或 JSONL，也不修改
置顶、归档、标题或任务状态。Perch 自己的缓存是可删除、可重建的 `cache.db`。

## 数据存在哪里？怎么备份或清理？

- Perch 缓存：`~/Library/Application Support/app.perch.desktop/cache.db`
- 源数据位置见 [SOURCES.md](SOURCES.md) 或 README 的“支持的来源”。

退出 Perch 后删除 `cache.db`、`cache.db-wal`、`cache.db-shm` 即可清空缓存，下次启动会重新扫描。
不要删除源 agent 的数据。

## 会读取聊天正文或 prompt 吗？

不会。当前版本只读取标题、项目路径、时间、模型、分支、消息计数、todo 的 subject、状态以及
置顶/未读标志。不读取消息正文、tool 输出、附件或思考过程。
标题由 agent 自己生成，可能包含 prompt 片段；Perch 只在本机保存，不发送到任何地方。

## 需要联网吗？

不需要。Perch 不发起网络请求，没有账号、遥测或云同步。界面不加载远程字体、图片或脚本。

## 为什么 Qoder 的“打开”只把 App 切到前台？

Qoder 官方支持 `qoder://`，但不支持会话级深链。`qoder://chat/<id>` 在 Qoder 日志中会报
`DEEP_LINK_ROUTE_UNSUPPORTED`，只能激活 App。Perch 会如实提示“无法定位到具体会话”。

## 为什么 Claude Code 的任务都显示“无显式状态”？

Claude Code 的本地数据没有会话完成/归档状态。Perch 不臆造状态，统一显示“无显式状态”（内部为
`idle`）。待办进度来自 `~/.claude/tasks/<sessionId>/`。

## 为什么 Trae CN 没有任务？ChatGPT 桌面会话去哪了？

- Trae CN：本机 `state.vscdb` 只有模型配置，任务索引为空；没有稳定可读的本地任务列表。
- ChatGPT 桌面：仅 Codex 会话写入 `~/.codex`，计入 Codex 来源；普通聊天存于云端，没有本地任务数据。

没有数据就不会显示在侧边栏。若未来出现稳定的只读数据源，会作为独立采集器加入（见 ROADMAP）。

## 为什么列表里看不到子会话或自动化任务？

子会话（Claude `subagents/`、OpenCode/Kilo `parent_id`、Qoder `side_chat`）和 Codex 自动化线程
默认不进入主列表，避免任务碎片化。它们仍保存在快照里，详情面板可显示“子会话”“自动化”徽标。

## 会话没有标题时显示什么？

显示 `新会话`。Perch 不会调用模型自行命名：OpenCode 的 `session_v2.title` 在部分会话里为空
（标题生成没有发生或失败），Qoder 新会话也可能暂时没有正式标题，此时统一使用该占位标题。

## “源已删除”是什么意思？

上一次完整扫描成功时存在、但本次扫描没有出现的任务会被标记 `source_missing`。
这不代表 Perch 删除了任何东西；源任务可能被 agent 清理或迁移。失败/部分扫描不会触发该标记。

## 为什么某次刷新后任务数突然变少？

如果来源上次有超过 20 个任务、本次结果不足一半，Perch 会认为可能异常，降级为部分扫描并保留旧快照，
不会把大量任务标记为缺失。确认源数据正常后再手动刷新一次即可。

## “打开”按钮会执行什么？

见 [ARCHITECTURE.md](../ARCHITECTURE.md) 的“继续工作”一节。命令由 Rust 侧按来源固定模板生成，
session id 会做字符集校验，前端不能传入任意命令。Claude Code 和 Codex CLI 会话会在 Terminal 中恢复；
Qoder 只能激活 App。

## 支持 Windows / Linux 吗？

目前只支持 macOS。采集路径、App 激活和终端执行都按 macOS 实现。

## 有现成的安装包吗？

还没有签名/公证的 Release。当前需要从源码构建：

```bash
npm install
npm run tauri build
```

产物在 `src-tauri/target/release/bundle/macos/Perch.app`，未签名版本首次打开需右键 → 打开。

## 如何卸载？

1. 退出 Perch。
2. 删除 `Perch.app`。
3. 可选：删除 `~/Library/Application Support/app.perch.desktop/`。

不会影响任何 agent 的数据。

## 如何新增一个来源？

参考 [ARCHITECTURE.md](../ARCHITECTURE.md) 的“扩展点：新增一个来源”。新采集器必须保持只读，
用临时 fixture 写测试，并更新 [SOURCES.md](SOURCES.md)。
