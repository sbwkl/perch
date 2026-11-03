# Contributing

感谢参与 Perch。这个项目的核心约束是**只读**：任何改动都不能写入、修改或删除 agent 的源数据。
在改采集器、存储或“打开”逻辑之前，请先阅读
[docs/DESIGN.md](docs/DESIGN.md) 与 [ARCHITECTURE.md](ARCHITECTURE.md)。

## 开始之前

- 小改动可以直接提 PR；新增来源、修改数据模型或“打开”行为请先开 Issue 说明动机与方案。
- 报告安全问题时走 [SECURITY.md](SECURITY.md) 的私密渠道，不要公开 Issue。
- 参与贡献即表示同意以 [MIT License](LICENSE) 授权你的改动。

## 开发环境

见 [DEVELOPMENT.md](DEVELOPMENT.md)。最短路径：

```bash
npm install
npm run tauri dev
```

## 提交前检查

```bash
npm run check
cargo fmt --all --check
cargo clippy -p perch-core --all-targets -- -D warnings
cargo test -p perch-core
```

CI 会执行同样的检查。涉及 UI 的改动请在 `npm run tauri dev` 或 `npm run demo` 中实际走一遍，
并说明验证方式。

## 只读红线

以下行为会被拒绝：

- 对源 SQLite 执行任何写语句，或打开可写连接；
- 写入、重命名、删除源 JSONL / JSON 文件；
- 读取并存储消息正文、prompt、tool 输出、附件或思考过程（见 SECURITY.md）；
- 让前端传入任意 shell 命令或 URL；
- 在没有证据时臆造 agent 状态（例如把 Claude 的 idle 显示成“已完成”）。

新增来源时必须：

1. 实现 `Collector`，只返回 `SourceResult`；
2. 使用只读连接/文件句柄；
3. 用临时 HOME + fixture 写测试，不依赖真实用户数据；
4. 更新 [docs/SOURCES.md](docs/SOURCES.md) 与 [ROADMAP.md](ROADMAP.md)。

## 数据与截图

- 测试 fixture 必须是合成的（如 `demo/fixture.ts` 的假任务），**不得**提交真实会话标题、
  项目路径、数据库或日志。
- 文档截图只使用 `demo/` 假数据；提交前检查图片中没有个人信息。
- Issue / PR 中粘贴日志前先脱敏。

## 代码风格

- Rust：`cargo fmt` + clippy 零警告；错误用 `thiserror`；采集失败要隔离，不影响其他来源。
- TypeScript / Svelte：保持 strict 模式；使用 Svelte 5 runes；不引入 UI 框架或远程资源。
- 注释只解释非显而易见的约束与原因，不写“这段代码做了什么”。
- 文档使用中文；代码标识符、命令和协议字段保持英文。

## Commit 与 PR

- Commit message 用简短的英文祈使句，例如 `Add Cursor collector`、`Fix Qoder running heuristic`。
- 一个 PR 聚焦一件事；附带测试或说明为什么不需要测试。
- PR 描述包含：动机、改动范围、验证步骤、对只读边界的影响。
- 如果改了用户可见行为，同步更新 README / CHANGELOG / 相关 docs。

## Review 关注点

- 是否新增了源写入路径或放宽了命令执行边界；
- 失败时是否保留旧快照、是否误标 `source_missing`；
- 新增字段是否会把正文/凭据带进缓存或日志；
- 状态推断是否有上游证据；
- 测试是否使用临时目录，是否可重复。
