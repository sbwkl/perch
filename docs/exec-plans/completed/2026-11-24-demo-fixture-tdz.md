# 2026-11-24 demo fixture 初始化顺序缺陷修复

| 项 | 内容 |
| --- | --- |
| 状态 | 已验证 |
| 类型 | 变更文档 |
| 关联产品规格 | 无（演示入口缺陷，不改变产品行为） |
| 关联设计 | [2026-11-24-continue-work-affordance.md](./2026-11-24-continue-work-affordance.md)（该变更的 demo 验收被本缺陷阻塞） |

## 背景

`npm run demo` 页面白屏，控制台报 `Uncaught ReferenceError: Cannot access 'UUID_PATTERN' before initialization`。

原因：`demo/fixture.ts` 在模块顶层调用 `buildTasks()` → `makeTask()` → `continueHint()`，而 `continueHint` 引用的 `const UUID_PATTERN` 声明在文件更靠后的位置。`const` 处于暂时性死区（TDZ），模块初始化时读取即抛错，整个演示模块加载失败，`#app` 渲染为空。

该缺陷由 2026-11-23 的 Qoder 深链变更引入（`continueHint` 新增 Codex UUID 判断时把常量留在了函数定义附近），影响所有 demo 截图与 UI 验收渠道。

## 目标行为

- `npm run demo` 正常渲染演示数据。
- 除声明位置外，不改变 `continueHint` 的判定逻辑。

## 验收

### 正常路径

1. `npm run demo` 打开后任务列表渲染，不再出现白屏。
2. 浏览器控制台无 `ReferenceError`。

### 异常与边界

1. Codex 桌面 / 自动化会话仍判定为深链通道，非 UUID 的 Codex 会话仍判定为 App 通道，与 `perch_core::continue_work::describe_continue` 一致。

## 验证记录

- 修复前：`npm run demo` 白屏，控制台 `Cannot access 'UUID_PATTERN' before initialization`（`makeTask → continueHint`，`demo/fixture.ts`）。
- 修复后：同一页面渲染 13 条任务，控制台无错误；Codex 深链 / App 两类会话判定与修复前逻辑一致。
- `npm run check`：93 files，0 errors / 0 warnings。