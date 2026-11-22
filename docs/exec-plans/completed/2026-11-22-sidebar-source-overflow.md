# 2026-11-22 侧边栏来源增多后的重叠与裁切

| 项 | 内容 |
|---|---|
| 状态 | 已验证 |
| Owner | wayne |
| 所属模块 / 入口 | 前端三栏布局：`src/App.svelte`、`src/app.css` |
| 关联 | [agent-sources.md](../../product-specs/agent-sources.md) R1–R2、R5；来源集合扩充至 11 个 |
| 触发 | 用户 2026-11-22 反馈“随着 Agent 的增加，perch 第一栏出现了不同程度的重叠” |

## 1. 摘要

第一栏是固定高度侧边栏，内部依次放置品牌、任务、Workspaces、来源和页脚。来源从 7 个增至 11 个后，静态高度超过窗口可用空间，但来源列表本身没有滚动容器，侧边栏又以 `overflow: hidden` 裁切溢出内容，造成来源项、同组说明与页脚互相覆盖或被截掉。

本次要把侧边栏改为“头部固定、中部可滚动、页脚固定”的纵向布局：导航内容共享一个滚动区，滚动区可以收缩到 0 高度，来源再多也不会挤压或覆盖页脚。任务、Workspaces、来源的分组顺序和筛选语义保持不变。

## 2. 问题与证据

### 2.1 复现环境

- 实机：macOS Apple Silicon，Perch 开发窗口，`npm run tauri dev`。
- 数据：本地真实来源 11 个，演示和测试仍只使用合成 fixture。
- 窗口：默认 `1200×780`，最小 `900×560`；`minWidth/minHeight` 定义于 `src-tauri/tauri.conf.json`。

### 2.2 观察结果

- `1200×780`：11 个来源全部排到页脚上沿，底部说明与页脚没有缓冲，窗口再小会进入重叠/裁切区。
- `900×560`：来源列表在 `Antigravity` 之后与页脚“只读采集 · 不改动源数据”文字发生重叠，`MiniMax Code` 向上挤压来源区；`CodeArts`、`CodeBuddy`、底部说明和部分页脚不可见。
- 侧边栏整体无法滚动，来源列表无法单独滚动；被裁掉的来源不能通过鼠标滚轮或键盘到达。

### 2.3 根因

当前结构如下：

```text
.sidebar (display:flex; column; height:100%; overflow:hidden)
├── header.brand
├── nav.nav-section                 任务（固定）
├── nav.nav-section.scroll-section  Workspaces（max-height:38vh; overflow-y:auto）
├── nav.nav-section.sources         来源（无高度约束、无滚动）
└── footer.sidebar-foot             margin-top:auto
```

- 只有 Workspaces 被设为滚动区，来源项全部按内容高度展开。
- 来源区是普通 flex item，默认 `flex-shrink: 1`，空间不足时会被压缩；其子项仍按内容绘制，因而与页脚发生覆盖。
- `.sidebar` 的 `overflow: hidden` 只负责裁切，不提供滚动，也不能保证被裁切项可访问。
- 固定 `38vh` 只限制 Workspaces，无法给来源区或其他分组提供可预测的空间分配。

### 2.4 影响

- 用户看不到靠后的来源，无法从侧边栏筛选这些来源的任务。
- 页脚隐私提示、用量报告入口等固定操作可能被遮住。
- 来源越多问题越明显，违背“来源可扩展”的产品目标。

## 3. 目标与非目标

### 目标

- 第一栏在 `900×560` 至任意常规窗口高度下，任何分组内容都不与页脚或其他分组重叠。
- 11 个来源全部可到达：来源列表溢出时在侧边栏内部滚动，滚轮与键盘都可操作。
- 页脚始终固定在侧边栏底部，不被导航内容覆盖。
- 视觉上保留现有分组顺序、图标、名称、任务数和筛选行为。
- 修复只依赖本地 CSS / 结构，不引入新依赖、远程资源或 IPC 变更。

### 非目标

- 不重新设计第一栏信息架构，不把来源折叠成弹层或二级菜单。
- 不改变来源采集、排序、筛选或 `continue_task` 行为。
- 不改变窗口最小尺寸、三栏宽度或右侧 Inspector。
- 不在本次处理任务列表、搜索弹层或用量报告页的无关布局问题。

## 4. 方案

### 4.1 推荐方案：单一侧栏滚动容器

把“任务 / Workspaces / 来源”三个 `nav` 包进一个可滚动的侧栏内容容器，品牌和页脚保持固定：

```text
.sidebar (column, height:100%, overflow:hidden)
├── header.brand
├── .sidebar-scroll (flex:1; min-height:0; overflow-y:auto)
│   ├── nav 任务
│   ├── nav Workspaces
│   └── nav 来源
└── footer.sidebar-foot (flex:none)
```

建议实现要点：

- `.sidebar-scroll { flex: 1 1 0; min-height: 0; overflow-y: auto; overflow-x: hidden; }`。
- `.sidebar-foot { flex: none; }`，不再依赖 `margin-top: auto` 去挤压内容。
- 移除 Workspaces 专用的 `max-height: 38vh`；Workspaces 是否“显示更多”仍沿用现有 `WORKSPACE_VISIBLE_LIMIT = 8` 逻辑。
- 若保留 Workspaces 内部滚动，也必须给它 `flex: none` 和明确的最大高度，并保证来源区仍有可用空间；首版不推荐嵌套两个滚动区。
- 可选：滚动区加 `overscroll-behavior: contain`，避免滚动到底后带动整页。
- 保持 `.sidebar` 的 `overflow: hidden`，只负责防止侧栏内容横向或整体溢出。

该方案让导航区域在高度不足时优先滚动，而不是压缩子项；来源数量增加只会增加滚动内容，不会改变页脚位置。

### 4.2 备选方案：仅让来源区滚动

给 `.sources` 增加 `flex: 1 1 auto; min-height: 0; overflow-y: auto`，并把 Workspaces 继续保持 `max-height`。优点是改动小；缺点是 Workspaces 和来源两个滚动区会争夺高度，最小窗口下容易再次出现嵌套滚动、焦点跳变和“内容存在但不易发现”的问题，因此不作为首选。

### 4.3 交互与可达性

- 来源列表超出一屏时，鼠标滚轮应滚动首选内容区，不滚动整个窗口。
- Tab 聚焦到被遮挡的来源项时，浏览器应将该项滚入可视区；聚焦环不能被页脚或侧栏裁掉。
- 滚动条沿用全局样式；浅色/深色主题都应可见。
- 不改变来源按钮点击、当前筛选高亮、错误 `!` 和 `title` 提示的行为。
- 页脚中的用量报告按钮保持可点击，不因内容区滚动而移动。

## 5. 验收标准（EARS）

### 正常

- R1：When 来源数量不超过窗口可容纳高度, the system shall 按现有顺序完整展示任务、Workspaces、来源和页脚，且不出现滚动条或内容重叠。
- R2：When 用户在任意窗口尺寸下点击某个来源, the system shall 保持该来源筛选行为不变，并让当前来源项处于可见区域。
- R3：When 窗口高度足以容纳导航内容, the system shall 让页脚固定在侧边栏底部，导航内容从品牌下方自然排列。

### 异常 / 空间不足

- R4：When 来源或 Workspaces 内容总高度超过侧边栏可用高度, the system shall 在导航内容区内部滚动，而不是压缩、覆盖或裁切来源项。
- R5：When 窗口缩小到项目最小尺寸 `900×560`, the system shall 保持页脚完整可见，并允许用户滚动到达全部 11 个来源及组内说明。
- R6：When 某个来源带有错误或不可用状态, the system shall 保留其名称、计数和红色 `!` 的可读性，且不会因新增状态文本挤压其他来源行。
- R7：When 用户聚焦到滚动区外不可见的来源按钮并通过 Tab 移动焦点, the system shall 自动将焦点项滚入可视范围，焦点环不被裁切。

### 边界 / 回归

- R8：When Workspaces 为空、恰好 8 个或超过 8 个, the system shall 不改变“显示更多 / 收起”的阈值与筛选语义，且不与来源区重叠。
- R9：When 侧边栏在浅色、深色或窗口缩放后重新布局, the system shall 不出现水平滚动、来源名称溢出遮挡计数或页脚漂移。
- R10：When 用量报告按钮存在且页脚已固定, the system shall 保持按钮可见、可聚焦、可点击；报告视图切换不受影响。

## 6. 影响范围与兼容性

- 代码：主要为 `src/app.css` 的侧栏 flex/overflow 规则；`src/App.svelte` 仅在采用 `.sidebar-scroll` 容器时需要增加包装层。
- 不影响：Rust core、Tauri 命令、IPC、来源采集、缓存 schema、任务排序和继续工作。
- 无障碍：新增滚动容器应保持语义分组，不改变按钮角色和名称。
- 兼容性：不改变支持窗口范围；使用 CSS flex 与 `min-height: 0`，无需浏览器新特性。

## 7. 验证计划

- 自动化门禁：
  - `npm run check`
  - `npm run build`
  - `cargo fmt --all --check`
  - `cargo clippy -p perch-core --all-targets -- -D warnings`
  - `cargo test -p perch-core`
- UI 实机验证：
  - `npm run tauri dev`，在 `1200×780`、`900×560` 两个尺寸检查 11 个来源。
  - 滚动到 `CodeBuddy`，确认页脚不移动、来源不被覆盖。
  - 切换来源筛选，确认高亮和任务列表结果不变。
  - 在 Workspaces 为空/8 个/超过 8 个三种状态下检查“显示更多”。
  - 用键盘 Tab 遍历来源，确认焦点项自动滚入可视区。
  - 浅色/深色主题各检查一次。
- 回归检查：
  - 用法向量侧栏连续滚动，确认没有双滚动区互相抢事件。
  - 打开/关闭用量报告视图，确认侧栏和报告宽布局均正常。

## 8. 实施状态与证据

当前状态：已实现并验证；分支 `feat/sidebar-source-overflow`。

| 时间 | 状态 | 证据 |
|---|---|---|
| 2026-11-22 | 已复现 | `1200×780`：来源列表贴到页脚；`900×560`：`Antigravity` 后来源项与“只读采集”页脚重叠，后续来源与说明被裁切 |
| 2026-11-22 | 已实现 | 品牌与页脚固定；任务、Workspaces、来源共享 `.sidebar-scroll` 滚动区，移除 `38vh` 嵌套滚动约束 |
| 2026-11-22 | 自动化通过 | `npm run check` 0 errors / 0 warnings；`npm run build` 成功；`cargo fmt --all --check`、`cargo clippy -p perch-core --all-targets -- -D warnings`、`cargo test -p perch-core` 全通过（82 tests） |
| 2026-11-22 | 实机尺寸验证 | `900×560`：滚动区 `clientHeight 442 / scrollHeight 674`，页脚 `top 517` 高于滚动区底部 `507`，无重叠；滚到底及 Tab 聚焦 `CodeBuddy` 时 11 个来源与说明均可见。`1200×780`：`clientHeight 662 / scrollHeight 674`，末端仅需滚动 12px |
| 2026-11-22 | 回归验证 | 长来源名省略且计数/红色 `!` 不被挤出；水平溢出为 false；打开用量报告后页脚仍可见、侧栏宽度不变 |

## 9. 回滚

布局改动仅限前端 CSS/模板。若需回滚，恢复 `.scroll-section` 和相关侧栏规则即可；无数据迁移、无缓存或源数据副作用。

## 10. 关联

- 产品规格：[agent-sources.md](../../product-specs/agent-sources.md)
- 当前侧栏结构：`src/App.svelte` 中 `.sidebar`、`.sidebar-scroll`、`.nav-section`、`.sources`、`.sidebar-foot`
- 当前样式：`src/app.css` 中 `.sidebar`、`.sidebar-scroll`、`.nav-item`、`.nav-note`、`.sidebar-foot`
- 历史布局约束：[v0.3.3-design-review.md](../../design-docs/history/v0.3.3-design-review.md) §10.2
