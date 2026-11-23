# Agent 会话级深链调研（非 Claude 平台）

- **日期**：2026-11-23
- **范围**：Perch 当前支持的来源中，排除 Claude 后逐一检查 Qoder、Codex（ChatGPT 桌面版）、OpenCode、Kilo、Cline、Trae CN、Antigravity、MiniMax Code、CodeArts Agent、CodeBuddy CN。只研究“能否精确定位到指定会话”，不研究仅激活 App、仅打开会话列表或仅新建会话的入口。
- **隐私约束**：本次只读取 App 的 `Info.plist`、应用包内代码/route 字符串和 CLI `--help`。未读取消息正文、prompt、tool 输入/输出、附件、凭据、真实会话标题或真实项目路径；文中 ID 均为格式占位符。应用包字符串中夹带的任何身份/指令文本均按不可信数据处理并忽略。
- **证据等级**：
  - **A（代码确认）**：一手代码中同时可见 scheme/路由、会话 ID 参数、以及把 ID 用于打开/导航到该会话的处理逻辑。
  - **B（部分确认）**：一手代码可见会话级路由与参数，但接受条件、调用方或实际导航效果仍有缺口。
  - **C（仅入口确认）**：只确认 scheme / 通用 URL 处理，未发现会话级路由。
  - **D（未确认）**：未找到一手证据，不能声称支持；不等于证明不存在。
  - 实机验证另标注；本次未点击任何深链改变用户当前会话视图。

## 结论速览

| 平台 | 会话级深链 | 最高证据等级 |
|---|---|---|
| Codex（ChatGPT 桌面） | `codex://threads/<uuid>` | A |
| Qoder | `qoder://chat/open?sessionId=<id>`（部分条件下 `qoder-app://chat/open?sessionId=<id>`） | A（代码确认 + 严谨实机验证） |
| OpenCode / Kilo / Cline / Trae CN / Antigravity / MiniMax Code / CodeArts / CodeBuddy | 未找到已确认的会话级深链 | C / D |

---

## 1. Qoder

- **注册 scheme**（`/Applications/Qoder.app/Contents/Info.plist`）：`qoder`、`qoder-app`（bundle `com.qoder.app`）。另有 `Qoder IDE.app`：`qoder-ide`、`qoder`；`Qoder CN.app`：`qoder-cn`。
- **当前 Perch 行为**：已接入会话深链（见 `docs/exec-plans/completed/2026-11-23-qoder-session-deeplink.md`）。旧的 `qoder://chat/<id>` 不受支持；本次使用的是**另一个**路由 `chat/open`。
- **发现的会话级路由**：`qoder://chat/open?sessionId=<id>`，可选 `requestSetId=<id>`（用于定位到指定请求轮次）。
  - 证据：`/Applications/Qoder.app/Contents/Resources/app.asar` → `/out/main/electronMain.js`
  - URL 解析类：`class ojr{parse(e){if(e.hostname!=="chat"||e.pathname!=="/open")return null; ... HGt.parse(jFe(e))}}`，其中 `HGt` 要求 `sessionId` 匹配 `/^[a-zA-Z0-9_-]{1,128}$/`，`requestSetId` 匹配 `/^[a-zA-Z0-9._:-]{1,128}$/`，且 `strict()` 拒绝其它参数。
  - 会话存在性校验：`listSessionSummaries([a.sessionId])` 后要求 `find(x=>x.sessionId===a.sessionId)`，否则 `DEEP_LINK_CHAT_SESSION_UNAVAILABLE`；`requestSetId` 通过 `findTurnIdByRequestSetId` 转成 `targetTurnId`。
  - 导航落地：`/out/renderer/assets/B7xIIv6H.js` 中 `kind==="chat.open"` 时 `push({kind:"chatSession",sessionId:n.sessionId,surface:"conversation",...targetTurnId})`。
  - 应用内代码对该动作的描述（同文件 `electronMain.js`）：`通过 chat/open Deeplink 打开已有会话，可选定位到指定请求轮次。`
- **scheme 细节**：主进程 deep-link router 构造为 `new mtn(t, [...], i)`，其中 `t = i.deepLink?.protocol ?? "qoder"`；ingress `new Pzt(Gi.protocol)` 只接受以 `<protocol>://` 开头的 URL。未发现把 `qoder-app://` 映射到本地 router 的代码；`qoder-app://` 在包内只作为 OAuth auth redirect URI 出现（`/out/main/chunks/Bb-Xbnf1.js`）。因此**首选 `qoder://`**；`qoder-app://` 只能在实机验证通过后使用。
- **与历史证据的关系**：`docs/design-docs/source-designs/SOURCES.md` 记录的 `qoder://chat/<id>` → `DEEP_LINK_ROUTE_UNSUPPORTED` 与本次发现的 `chat/open` 不是同一路径：router 对所有未匹配 handler 的 URL 都会抛 `DEEP_LINK_ROUTE_UNSUPPORTED`，不能据此否定 `chat/open`。包内未找到生成 `chat/open` URL 的调用方（MCP App 通过 `ui/open-link` 传入），但有严谨实机证据（见下），因此标 **A**。
- **严谨实机验证（2026-11-23）**：以一个真实目标会话（报告中记为“目标会话 A”，不记录标题或 ID）做两次独立交叉验证。第一次先将 Qoder 切到非目标会话 B，确认前台标题不是目标；打开 `qoder://chat/open?sessionId=<目标会话ID>` 后前台标题切到目标会话 A。第二次先将 Qoder 切到另一个非目标会话 C，再次打开同一深链，前台标题又切到目标会话 A。结论：该深链确实能精确打开指定会话；实机截图仅保留在本地临时目录，不写入文档。
- **可由 Perch TaskView 构造**：**可以**。`TaskSnapshot.source_id` 即 Qoder 的会话 ID（见 `crates/perch-core/src/collectors/qoder.rs`，采集 `sessionId`），且 `valid_session_id()` 允许的字符是 `[A-Za-z0-9_-]`，正好覆盖校验；长度需额外限制 ≤128。固定模板即可，不需要新字段；`deeplink` 字段可保留给第三方传入的 URL，但不应信任外部 URL。
- **建议**：已按此实现：Qoder 的继续工作使用固定模板 `qoder://chat/open?sessionId=<source_id>`，并对长度与字符集做严格校验（`^[A-Za-z0-9_-]{1,128}$`）。`qoder-app://` 与 `requestSetId` 暂不启用，除非后续单独验证。无效 ID 回退激活 App，不构造额外 URL。

## 2. Codex（ChatGPT 桌面版）

- **注册 scheme**（`/Applications/ChatGPT.app/Contents/Info.plist`）：`codex`、`http`、`https`（bundle `com.openai.codex`，即 ChatGPT App）。
- **已确认格式**：`codex://threads/<conversationId>`。
  - 证据 1：`app.asar` → `/.vite/build/early-bootstrap.js`：
    - 生成器：`let a=...safeParse(e.get("codex_thread_id")?.trim()).data, i=new URL(\`codex://threads/${a??"new"}\`); ... i.searchParams.set("hostId",r)`；可选 `q`、`mode`、`prompt`。
    - 解析器：`if("codex:"!==n.protocol||"threads"!==n.host)return null; let [a,...r]=n.pathname.split("/").filter(Boolean)`，schema 要求 `conversationId` 为 UUID 正则 `^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i`，并拒绝额外 path 段。
    - 语义：`conversationId` → `(0,i.JX)(o.data.conversationId)` 后由 `navigateToRoute` / `threadIdToLocate` 传给窗口（同文件 `early-bootstrap-916-*.js` 与 `early-bootstrap-7-*.js` 中 `onDeepLinkReceived`、`threadIdToLocate` 相关代码）。
  - 证据 2：`/webview/assets/async/*.js` 多处把 UI 中的“在浏览器中打开”生成 `codex://threads/${encodeURIComponent(t.threadId)}`。
  - 附加参数：`?view=review`（可选 `&diffFilter=...&path=...`）打开该 thread 的 review 面板，仍以指定会话为目标。
- **可由 Perch TaskView 构造**：**可以，但需格式校验**。Perch 的 `codex::source_id` 来自 rollout 的 `id`/`session_id`（`crates/perch-core/src/collectors/codex.rs:238`）。`plan_continue` 直接拼 `codex://threads/{source_id}`，没有校验 UUID；若 source_id 不是标准 UUID（例如旧版 `plugins_...` 形式），App 解析器会拒绝。建议在 Rust 侧加 UUID 校验；当前失败时不做特别处理（代码生成之后由 App 自行报错）。
- **验证建议**：用合成 UUID 点击 `codex://threads/00000000-0000-4000-8000-000000000000`，确认打开的是“定位失败”还是“选中会话”；再对真实会话做人工确认（本次不执行）。CLI `codex resume <uuid>`（`codex --help` / `codex resume --help`）是已确认的精确终端等价机制，但不在 Perch 当前继续工作范围内。

## 3. OpenCode

- **注册 scheme**（`/Applications/OpenCode.app/Contents/Info.plist`）：`opencode`（bundle `ai.opencode.desktop`）。
- **深链处理链**：`app.asar` → `/out/main/desktop-DX9gxXAI.js`：`app.on("open-url")` 与 `second-instance` 收集以 ``opencode://`` 开头的 URL → `DeepLinks.make` → `DeepLinksOpened` IPC → `/out/renderer/assets/main-DJeK9LOU.js`：`window.__OPENCODE__.deepLinks` + `CustomEvent("opencode:deep-link")`。
- **会话级判定**：`function yh(e){let t=new URL(e); if(t.protocol!=="opencode:"||t.hostname!=="console"||t.pathname!=="/authorized")return; let n=t.searchParams.get("window"); ... return n}`。即唯一被主进程识别的是 **`opencode://console/authorized?window=<windowId>`**，用于把 OAuth 回调路由到指定窗口，**不含 session ID**。渲染进程中也未发现对 `opencode://` URL 本身按 session 解析的代码。
- **等价机制**：`/Applications/OpenCode.app/Contents/Resources/opencode-cli --help` 明确 `--session, -s string  Session ID to continue, or to create if it does not exist`；即 `opencode --session <id>` 可精确恢复会话（终端通道）。
- **可由 Perch TaskView 构造**：深链**不可**；终端命令**可**（`opencode --session <source_id>`），但终端通道当前不在 Perch 的继续工作范围内，Perch 只激活 App。
- **验证建议**：若要把 App 分支升级为精确恢复，需先确认桌面 App 是否暴露 session 路由；目前一手代码没有，建议保持 App 激活并提示“无法定位”。

## 4. Kilo

- **注册 scheme**（`/Applications/Kilo.app/Contents/Info.plist`）：`kilo-desktop`（bundle `ai.kilo.desktop`）。
- **深链处理链**：`app.asar` → `/.vite/build/app.js`：`#j(e)` 调用 `setAsDefaultProtocolClient(t)`、监听 `open-url` / `second-instance`，转 `e.openUrl(n)`；渲染进程 `/.vite/renderer/main_window/assets/index-DOQ_NoWC.js` 的 `Lbn()` 解析 URL：`host = n.hostname || n.pathname...split("/")[0]`，按扩展点 `kilo.shell/deep-link` 的 `metadata.host` 匹配。
- **已注册 host**：包内唯一内置定义是 `{"kilo.shell/deep-link":[{id:"model-catalog",host:"model-catalog",path:"/settings/local-model-server",pathParam:"catalogModel"}]}`，即 `kilo-desktop://model-catalog/<model>` 打开“本地模型服务器设置”，与会话无关。没有 session / conversation / thread host。
- **等价机制**：`/opt/homebrew/bin/kilo --help`：`-s, --session  session id to continue`（另有 `--fork`、`--cloud-fork`）。Perch 不使用该参数：终端通道不在当前继续工作范围内。
- **可由 Perch TaskView 构造**：深链**不可**；终端命令**可**（`kilo --session <source_id>`，需校验 ID 字符），但 Perch 当前只激活 App。
- **验证建议**：若未来恢复终端通道，在临时目录用合成 session id 验证 `kilo --session` 的行为。

## 5. Cline

- **注册 scheme**（`/Applications/Cline.app/Contents/Info.plist`）：**无 `CFBundleURLTypes`**。包内也没有 Electron 主进程 deep-link handler（App 不是 Electron 主包结构）。
- **会话级判定**：**明确无 App 深链**。CLI `/Users/wayne/.nvm/versions/node/v24.16.0/bin/cline --help` 显示 `--id <session-id>  Resume an existing session by ID`；Perch 不使用终端通道。
- **可由 Perch TaskView 构造**：深链**不可**；终端命令**可**，但不在当前继续工作范围内。
- **验证建议**：无需进一步验证 App 深链；保持仅激活 App 并在 UI 明示无法定位。

## 6. Trae CN

- **注册 scheme**（`/Applications/Trae CN.app/Contents/Info.plist`）：`trae-cn`（bundle `cn.trae.app`）；另有 `Trae.app`：`trae`。
- **URL 处理**：`/Applications/Trae CN.app/Contents/Resources/app/out/main.js` 是 VS Code 系 Electron：`ElectronURLListener` 监听 `open-url`，`app#handleProtocolUrl()` 处理。
- **已发现路由**：
  - `trae-cn://workspace?path=<dir>` → 转成文件/目录 workspace URI，打开的是**项目**；
  - `trae-cn://file/<path>`、`trae-cn://vscode-remote/...` → 打开文件/远程目录；
  - `trae-cn://marketplace/plugin/...`、`trae-cn://migration?migrationId=...`（迁移窗口）；
  - 参数 `windowId=_blank`、`continueOn`。
- **会话级判定**：未发现 `sessionId` / `conversationId` 参与 URL 路由；`sessionId` 在该文件中多为 VS Code 内部 tracing/遥测字段。`/usr/local/bin/trae-cn --help` 只有 `chat` 子命令（传入 prompt 新建会话），无恢复指定会话参数。标 **C（仅入口确认）**。
- **可由 Perch TaskView 构造**：**不可**（深链和 CLI 都不能定位会话）。
- **验证建议**：若后续版本新增 agent 深链，优先检查 `handleProtocolUrl()` 是否出现 authority === "chat" 或 query `sessionId`；当前不建议改动。

## 7. Antigravity

- **注册 scheme**（`/Applications/Antigravity.app/Contents/Info.plist`）：`antigravity`（bundle `com.google.antigravity`）；`Antigravity IDE.app`：`antigravity-ide`。
- **包内证据**：`app.asar` 的 `/dist/*.js` 中未出现 `antigravity://` 字面量；`customScheme.js` 只注册内部 `plugin`、`agy-favicon`、`agy-browser-stream`，`externalUrl.js` 只做出站 URL 分类。`open-url` / `second-instance` 监听存在但未发现会话路由。标 **C**。
- **等价机制**：`/Users/wayne/.local/bin/agy --help` 显示 `--conversation  Resume a previous conversation by ID`（以及 `-c/--continue` 只续最近一次）。这是**终端精确恢复**，Perch 当前只用 App 激活。
- **可由 Perch TaskView 构造**：深链**不可**；终端命令**可**（`agy --conversation <source_id>`）。
- **验证建议**：用合成 ID 验证 `agy --conversation` 的退出码/行为后再考虑替换 App 分支；注意该 CLI ID 可能与 Perch 采集的 session key 不同，需要字段映射验证。

## 8. MiniMax Code

- **注册 scheme**（`/Applications/MiniMax Code.app/Contents/Info.plist`）：`minimax-cn`（bundle `com.minimax.agent.cn`）。
- **包内深链文档**（`app.asar` → `/dist/main/modules/deeplink/index.js` 注释，一手代码注释）：
  - `minimax-cn://open` 打开应用；
  - `minimax-cn://chat?message=hello` 打开并发送消息（**新建**会话，不是定位已有会话）；
  - `minimax-cn://navigate?url=xxx` 导航到指定页面；
  - `minimax-cn://workspace/open?path=xxx` 以本机目录新建任务（`mcode app`）。
  - 解析器 `/dist/main/modules/deeplink/parse.js` 仅提取 `action` + `params`，并拒绝含凭据参数；实际注册 handler 的 action 目前只有 `workspace/open`（`const handlers = new Map([[WORKSPACE_OPEN_ACTION, rememberWorkspaceOpenRequest]])`）。
- **会话级判定**：**未确认**。没有 `sessionId` / `conversationId` 深链参数，也没有把深链 ID 用于选中会话的代码。`navigate?url=`  targets 页面而非会话。标 **C/D**。
- **可由 Perch TaskView 构造**：**不可**。
- **验证建议**：若产品侧能提供内部 route 名，可检查 `navigate` 是否支持会话路由；当前不建议尝试。

## 9. CodeArts Agent

- **注册 scheme**（`/Applications/CodeArts Agent.app/Contents/Info.plist`）：`codearts-agent`（bundle `com.huawei.codearts.agent`）。
- **包内证据**：`/Applications/CodeArts Agent.app/Contents/Resources/app/out/main.js` 为 VS Code 系；有 `open-url` / 协议注册，但没有会话级 authority/path/query；`chat` 只是 CLI 子命令描述（`--chat` 传 prompt 新建会话），不是 URL 路由。未找到可用 CLI 恢复参数。标 **C/D**。
- **可由 Perch TaskView 构造**：**不可**。
- **验证建议**：保持现状；若后续版本加入路由，检查 `main.js` 的 protocol handler 中是否出现 `conversationId`。

## 10. CodeBuddy CN

- **注册 scheme**（`/Applications/CodeBuddy CN.app/Contents/Info.plist`）：`codebuddycn`（bundle `com.tencent.codebuddycn`）。
- **包内证据**：`/Applications/CodeBuddy CN.app/Contents/Resources/app/out/main.js` 为 VS Code 系，协议处理只支持 `workspace` 类路径与 `windowId` 等通用参数。`codebuddy/main.js` 中的 `conversationId` 是 IDE 内部消息/队列状态字段，不在 URL scheme 中暴露。标 **C/D**。
- **可由 Perch TaskView 构造**：**不可**。
- **验证建议**：保持现状。

## 11. 参考：Codex CLI 与 ChatGPT 桌面的区别

- `codex resume <uuid>`（CLI，`codex resume --help` 确认）与 `codex://threads/<uuid>`（桌面 App）是两条独立的精确恢复通道。
- 桌面深链解析器要求严格 UUID 格式；CLI resume 接受 session name/UUID，宽松得多。
- Perch 当前只使用桌面深链（`agent_kind = desktop | automation`）；CLI 通道未接入继续工作，其他 Codex 会话只激活 App。

---

## 结论表

### 可立即利用（在 Perch 中按固定模板构造并做 ID 校验）

| 平台 | 格式 | 前置条件 |
|---|---|---|
| Codex 桌面 | `codex://threads/<uuid>` | 已实现（`agent_kind = desktop / automation`）；补充 UUID 校验以避免无效链接 |
| Qoder | `qoder://chat/open?sessionId=<id>`（可选 `&requestSetId=<id>`） | 已实现：证据 A，两次严谨实机验证均能切到目标会话；`sessionId` 必须 `^[A-Za-z0-9_-]{1,128}$` |

### 需进一步验证（一手代码只提供非深链的精确恢复方式，或无会话路由）

| 平台 | 现状 | 建议 |
|---|---|---|
| OpenCode | App 只有 `opencode://console/authorized?window=`；CLI `--session <id>` 可精确恢复 | 保持 App 激活；终端通道当前不在继续工作范围内，不要为 App 编造 session URL |
| Kilo | `kilo-desktop://model-catalog/...` 与会话无关；CLI `--session <id>` 可精确恢复 | 保持 App 激活；若未来恢复终端通道，需加入 `--session` |
| Antigravity | 未发现 App scheme 处理路由；CLI `agy --conversation <id>` 可精确恢复 | 保持 App 激活；验证 CLI ID 与 Perch source_id 的映射后再考虑终端通道 |
| Codex（非 UUID id） | 桌面深链拒绝非 UUID | 保持 App 激活（`codex resume` 不在当前范围内） |

### 明确不支持（就目前一手证据）

| 平台 | 结论 | 依据 |
|---|---|---|
| Cline 桌面 | App 无 URL scheme、无会话深链 | `Info.plist` 无 `CFBundleURLTypes`；CLI `--id` 是终端精确入口，当前不接入继续工作 |
| Trae CN | 仅 workspace/file/migration 路由，无会话 ID | `out/main.js` `app#handleProtocolUrl()`；`trae-cn --help` 无恢复参数 |
| MiniMax Code | `open` / `chat?message=` / `navigate` / `workspace/open` 均非“定位已有会话”；handler 仅 `workspace/open` | `/dist/main/modules/deeplink/index.js`、`parse.js` |
| CodeArts Agent | VS Code 通用协议处理，无会话路由 | `out/main.js` |
| CodeBuddy CN | VS Code 通用协议处理，`conversationId` 不在 URL 中 | `out/main.js`、`codebuddy/main.js` |
| Qoder（`qoder://chat/<id>`） | 不支持，已由历史实机证据 + router 未匹配 handler 行为解释 | `SOURCES.md`；`class mtn.route()` 的默认 `DEEP_LINK_ROUTE_UNSUPPORTED` |
