# 2026-11-24 继续工作入口收敛与详情面板布局缺陷修复

| 项 | 内容 |
| --- | --- |
| 状态 | 已验证 |
| 类型 | 变更文档 |
| 关联产品规格 | [agent-sources.md R3](../../product-specs/agent-sources.md) |
| 关联设计 | [深链调研](../../references/agent-session-deeplinks.md)、[GLOSSARY.md](../../../GLOSSARY.md) |

## 背景

详情面板的“继续工作”入口目前有三层重复表达：一个通道胶囊（`仅激活 App` / `深链打开`）、一个按钮（`仅激活 App OpenCode` / `深链打开 Qoder`）、下方一段说明（`激活 OpenCode，无法定位到具体会话；点击前请确认当前 App 中的会话。`）。同一件事说三遍，把详情内容挤出首屏。

更严重的是布局缺陷：通道胶囊的修饰类 `.continue-channel.app` 与全局应用壳类 `.app`（`display: grid; grid-template-columns: 238px minmax(0, 1fr) 356px; height: 100vh`）撞名。任何走 App 通道的来源（除 Qoder 与 Codex 桌面会话外的全部来源），其胶囊都会被撑成整个 grid 容器。实测在 1440×694 窗口下该元素为 **542×694 px**，占据详情面板几乎全部高度，并把按钮、说明与后续详情推到右上角，导致第一眼看不到详情。深链通道使用 `.deeplink` 修饰类，不撞名，因此 Qoder / Codex 正常。

## 目标行为

- 继续工作在详情面板只呈现**一个按钮**，不显示通道胶囊，也不显示按钮下方说明段。
- 按钮文案为「动词 + 来源名称」：能定位到具体会话的来源用 **打开**（`打开 Qoder`、`打开 Codex`）；只激活 App 的来源用 **激活**（`激活 Trae CN`、`激活 Claude Code`、`激活 OpenCode`）。名词取 `source_label`，不取 App 名。
- 按钮背景取该来源的主题色（`SOURCE_COLORS`），前景色在深色字 `#1d1d1f` 与白色之间取对比度更高者，并恒定一条 1px 内描边。描边颜色随主题：浅色面板用暗描边，深色面板用亮描边——否则 OpenCode 的近黑填充（`#201d1d`）在深色面板（`--pane: #1c1c1f`）上的亮度对比只有 1.02:1，连描边一起沉进背景。
- 通道不再有颜色区分：深浅链通道按钮长相一致，只有动词不同。
- 点击成功后不显示任何提示；点击失败显示一行危险色（`--danger`）错误文本，内容为底层 `open` 失败原因。
- `continue_hint` 缺失时**保留按钮**（这是用户明确选择的行为）：该分支只在未来新增来源漏注册 `plan_continue` 时出现，按钮用中性动词，点击后由 Tauri 错误通道返回失败原因，而不是把动作藏起来。
- 已知来源集合与 Rust `plan_continue` 的决策分支保持不变：仍只有 Qoder（合法 session id）与 Codex 桌面 / 自动化（UUID）走深链，其余只激活 App，不启动终端或 CLI。

## 验收

### 正常路径

1. 选中任何 App 通道来源的会话，详情面板中**不存在**被撑大的胶囊元素；`任务信息` 区块在首屏可见。以 1440×694 窗口为基准，继续工作行高度不超过 32px。
2. OpenCode 会话显示单一按钮「激活 OpenCode」，背景为 `#201d1d`、文字为白色。
3. Codex 桌面会话（UUID session id）显示「打开 Codex」，背景 `#ffffff`、文字为深色，在浅色面板上可见描边。
4. Qoder 会话显示「打开 Qoder」，背景 `#2adb5c`。
5. 点击成功后，详情面板不出现任何新增文字。
6. 点击失败（例如目标 App 未安装）时，按钮右侧出现危险色错误文本，无需重新加载即可重试。

### 异常与边界

1. 白色 / 近白色来源色（`codex: #ffffff`、`cline: #e7e5e7`）使用深色前景 `#1d1d1f`；同理 `kilo: #ffdd00`、`trae-cn: #32f08c`、`qoder: #2adb5c`、`claude: #d97757` 也取深色前景。只有真正偏暗的品牌色——`opencode: #201d1d` 与 `minimax: #c026d3`——取白色前景。选择规则只有一条（比较深色字与白色的对比度），不是按来源列表硬编码，新增来源无需第二张表。
2. 深色模式下 `codex` 白色按钮与 `opencode` 深色按钮都保持可见边界：浅色面板的描边为 `rgba(0, 0, 0, 0.14)`，深色面板换成 `rgba(255, 255, 255, 0.22)`，始终落在填充色的反侧。
3. 非 UUID 的 Codex 会话、非法 Qoder session id 仍显示「激活 …」，与 Rust `plan_continue` 一致。
4. `continue_hint` 缺失时保留按钮（用户明确要求），动词取「打开」，点击后失败原因以危险色显示在按钮右侧。
5. 按钮点击进行中显示「打开中…」/「激活中…」，期间不可重复点击。
6. 切换选中任务会清除上一条失败错误。
7. 前端仍只传任务 key；`continue_task` 失败通过 Tauri 错误通道返回，前端不传 URL 或命令。

### 阻塞验收的既有缺陷（已拆分到独立变更）

1. `npm run demo` 白屏：`demo/fixture.ts` 的 `UUID_PATTERN` 声明在 `makeTask` 调用之后，模块初始化时 `makeTask → continueHint` 读取处于 TDZ，抛出 `ReferenceError: Cannot access 'UUID_PATTERN' before initialization`，演示页渲染为空。
2. 该缺陷不改变产品行为；若不修，demo 渠道无法验收本次 UI 改动。
3. 按 `AGENTS.md` 「不相关的修复另开独立变更文档」，该修复**不属于本次变更**，已单独提交为 [2026-11-24-demo-fixture-tdz.md](2026-11-24-demo-fixture-tdz.md)（PR #14）。本分支从该分支切出，因此不受该缺陷阻塞。

## 验证记录

### 缺陷复现（修复前）

- `npm run demo` 白屏，控制台：`Uncaught ReferenceError: Cannot access 'UUID_PATTERN' before initialization`（`demo/fixture.ts`，`makeTask → continueHint`）。修复后同一页面渲染 13 条任务。
- App 通道胶囊实测：1440 宽窗口下 `.continue-channel.app` 计算样式为 `display: grid`、`grid-template-columns: 214px 0px 312px`，元素尺寸 **542×694 px**，`.continue-row` 被撑到 694 px 高；`.deeplink` 通道不受影响。修复后该元素删除，`.continue-row` 高 28.5 px。

### 单测与门禁

- `cargo test -p perch-core`：82 passed / 0 failed；`continue_work` 覆盖 Qoder 深链 / 回退、Codex UUID 深链 / 非 UUID 回退、全部 App-only 来源、非法 session id、未知来源；`store_query` 覆盖 `continue_hint.channel` 的 Deeplink / App 分支。
- `npm run check`：93 files，0 errors / 0 warnings。
- `cargo fmt --all --check`、`cargo clippy -p perch-core --all-targets -- -D warnings`：通过。
- `cargo check`（`src-tauri`）：通过，`continue_task` 已收窄为 `Result<(), String>`。

### demo UI 验收（端口 1421）

- 逐来源核对按钮文案与配色（浅色与深色两套主题）。前景列是对 `contrastText` 同一算法逐来源求值的结果，对比度为该前景与品牌填充的 WCAG 比值：

| 来源 | 按钮 | 背景 | 前景 | 对比度 |
| --- | --- | --- | --- | --- |
| Qoder | 打开 Qoder | `#2adb5c` | `#1d1d1f` | 9.12:1 |
| Codex（UUID 桌面） | 打开 Codex | `#ffffff` | `#1d1d1f` | 16.83:1 |
| Codex（非 UUID） | 激活 Codex | `#ffffff` | `#1d1d1f` | 16.83:1 |
| Claude Code | 激活 Claude Code | `#d97757` | `#1d1d1f` | 5.39:1 |
| OpenCode | 激活 OpenCode | `#201d1d` | `#ffffff` | 16.73:1 |
| Kilo | 激活 Kilo | `#ffdd00` | `#1d1d1f` | 12.50:1 |
| Trae CN | 激活 Trae CN | `#32f08c` | `#1d1d1f` | 11.20:1 |
| Cline | 激活 Cline | `#e7e5e7` | `#1d1d1f` | 13.43:1 |
| Antigravity | 激活 Antigravity | `#0891b2` | `#1d1d1f` | 4.57:1 |
| MiniMax Code | 激活 MiniMax Code | `#c026d3` | `#ffffff` | 4.71:1 |
| CodeArts | 激活 CodeArts | `#d946ef` | `#1d1d1f` | 4.87:1 |
| CodeBuddy | 激活 CodeBuddy | `#f97316` | `#1d1d1f` | 6.00:1 |

- 全部 11 个来源的按钮前景 / 背景对比度均 ≥ 4.5:1，最低为 Antigravity 4.57:1；只有 OpenCode 与 MiniMax Code 取白色前景，其余都取深色字。浅色面板使用暗描边、深色面板使用亮描边，因此白色与近白色品牌色在浅色面板、深色品牌色在深色面板上都有可见边界。
- demo fixture 的策划任务只覆盖 6 个来源；为让「逐来源核对」名副其实，临时向 `demo/fixture.ts` 注入 Trae CN、Antigravity、MiniMax Code、CodeArts、CodeBuddy 五条合成任务，在浏览器里逐个读取 `.continue-button` 的计算样式，核对完即还原（`git diff` 确认只剩改名）。浅色主题下五者前景依次为 `#1d1d1f` / `#1d1d1f` / `#ffffff` / `#1d1d1f` / `#1d1d1f`，与上表一致；深色主题下（`--pane: rgb(28, 28, 31)`）描边为 `rgba(255, 255, 255, 0.22)`，浅色主题下为 `rgba(0, 0, 0, 0.14)`。OpenCode 在深色主题下实测：填充 `rgb(32, 29, 29)`、前景白、亮描边，按钮与面板不再糊在一起。
- 通道胶囊与说明段不再渲染（`.continue-channel` / `.continue-detail` DOM 节点数为 0）；选中任务后「任务信息」标题在首屏可见。
- 成功点击不产生任何新增文字；失败路径以临时注入的错误验证：`open 执行失败（退出码 1）` 以 `--danger` 色（`rgb(192, 57, 43)`）显示在按钮右侧，切换任务即清除。验证后已还原注入。

### 真实 Tauri 窗口验收（`npm run tauri dev`）

- 通过 macOS 无障碍树读取按钮标题：Claude Code 会话为「激活 Claude Code」，Qoder 会话为「打开 Qoder」。
- 对「激活 Claude Code」执行 AXPress：前台进程由 Perch 切到 Claude，未出现任何提示文本。
- 对「打开 Qoder」执行 AXPress：前台进程切到 Qoder，未出现任何提示文本，说明深链通道与 App 通道在真实窗口下分别生效。
- 未在本机逐个点击其余 App-only 来源（会反复抢焦点）；其决策分支由 `continue_work` 单测覆盖，按钮文案由 demo 逐来源核对。
- 深色主题描边在同一窗口实测：系统外观切到深色后截取 OpenCode 会话，按钮填充为 `rgb(31, 29, 29)`，其外侧一两像素为 `rgb(80, 79, 79)`（即 `rgba(255, 255, 255, 0.22)` 叠在面板上的合成值），再外侧才是面板 `rgb(28, 28, 31)`——近黑按钮在深色面板上有明确边界，不是只靠 CSS 计算值推断。截图与系统外观随后已还原。