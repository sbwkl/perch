# 2026-11-24 demo fixture 初始化顺序缺陷修复

| 项 | 内容 |
| --- | --- |
| 状态 | 已验证 |
| 类型 | 变更文档 |
| 关联产品规格 | 无（演示入口缺陷，不改变产品行为） |
| 关联设计 | 文字引用 PR #13 的变更文档 `docs/exec-plans/completed/2026-11-24-continue-work-affordance.md`（该变更的 demo 验收被本缺陷阻塞）。本 PR 不依赖那个文件，故不建链接 |

## 背景

`npm run demo` 页面白屏，控制台报 `Uncaught ReferenceError: Cannot access 'UUID_PATTERN' before initialization`。

原因：`demo/fixture.ts` 在模块顶层构造 `TASKS` 数组时逐个调用 `makeTask()`，`makeTask()` 又调用 `continueHint()`，而该函数引用的 `const UUID_PATTERN` 声明在同一文件的更靠后位置。`const` 处于暂时性死区（TDZ），模块初始化时读取即抛错，整个演示模块加载失败，`#app` 渲染为空。

该缺陷由 2026-11-23 的 Qoder 深链变更引入（新增 Codex UUID 判断时把常量留在了函数定义附近），影响所有 demo 截图与 UI 验收渠道。

## 目标行为

- `npm run demo` 正常渲染演示数据。
- 除声明位置外，不改变 `continueHint()` 的判定逻辑。

## 只读边界影响

无。改动只发生在前端演示入口 `demo/fixture.ts` 内，把一条 `const` 声明上移：

- 不新增任何源数据读取路径，`demo/` 仍然完全不接触真实 agent 数据。
- 不改变 `crates/perch-core`、`src-tauri` 或 IPC 命令，`continue_task` 的固定模板与 session id 校验不变。
- 不写 `cache.db` / `overlay.db`，不引入插件或远程资源。

## 验收

### 正常路径

1. `npm run demo` 打开后任务列表渲染，不再出现白屏。
2. 浏览器控制台无 `ReferenceError`。

### 异常与边界

1. Codex 桌面 / 自动化会话仍判定为深链通道，非 UUID 的 Codex 会话仍判定为 App 通道，与 `perch_core::continue_work::describe_continue` 一致。

### 不新增回归测试的理由

本仓库没有前端单元测试框架（`package.json` 只有 `svelte-check` 类型检查，没有 vitest / jest），而 `demo/` 是 Vite 别名替换 Tauri API 的演示入口，不参与 `cargo test -p perch-core`。为一条 `const` 声明位置写测试需要先引入整套前端测试基建，超出本缺陷的修复范围；等价保障由 `npm run check`（覆盖 `demo/fixture.ts` 的类型与作用域）与下面的 demo 实跑记录提供。若将来引入前端测试框架，这个用例应作为「demo 可加载」的冒烟测试补上。

## 验证记录

- 修复前：`npm run demo` 白屏，控制台 `Cannot access 'UUID_PATTERN' before initialization`（`makeTask → continueHint`，`demo/fixture.ts`）。
- 修复后：同一页面渲染 13 条任务，控制台无错误；Codex 深链 / App 两类会话判定与修复前逻辑一致。
- 门禁（本分支上全部重跑）：

| 命令 | 结果 |
| --- | --- |
| `npm run check` | 93 files，0 errors / 0 warnings |
| `npm run build` | 通过 |
| `cargo fmt --all --check` | 通过 |
| `cargo clippy -p perch-core --all-targets -- -D warnings` | 通过 |
| `cargo test -p perch-core` | 82 passed / 0 failed（30 单元 + 5 + 25 + 7 + 10 + 5 集成） |

CI 在两个 PR 上首次运行时结论为 pending；后续修复推送后以 CI 结果为准。
