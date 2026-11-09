# 搜索能力设计方案

> 状态：P0 元数据搜索已实现（2026-11-09）；P1 内容搜索按 opt-in 方案后置。
> 关联文档：[DESIGN.md](DESIGN.md) §10、[SOURCES.md](SOURCES.md)、[SECURITY.md](../SECURITY.md)

## 1. 目标与非目标

**目标**

- 在任务规模约 300 条时，用一次输入快速定位会话，中文可直接匹配。
- 元数据搜索（标题、项目、待办等）默认可用、即时响应。
- 会话内容搜索作为**显式 opt-in**能力；只在本地进行，不联网。
- 模糊搜索与语义搜索分别评估，按价值/成本决定是否进入当前版本。

**非目标（当前版本）**

- 不做云端搜索、远程 embedding、跨设备同步。
- 不搜索 tool 输入/输出、思考过程、附件、图片。
- 不改变“源数据只读”的硬约束；所有源仍以只读方式打开。

## 2. 现状

- 界面已有视图（最近/更早/已归档）、workspace 筛选、来源筛选，全部在前端 `flatTasks` 中完成。
- `cache.db` 的 `task` / `task_todo` 已保存任务元数据；`bootstrap` 一次性返回全部 workspace 与任务。
- P0 元数据搜索已实现：`perch-core::search` + `search_tasks` 命令 + 侧边栏搜索入口（点击或 ⌘F 在列表头部展开搜索框）；搜索范围固定为全部会话（含归档、子会话、自动化），只读 `cache.db`。
- 内容搜索（P1）尚未实现，当前隐私声明（README、SECURITY）“不读取消息正文、prompt、assistant 输出、tool 输入/输出”仍然成立。
  **一旦实现内容搜索，该表述必须同步修订并限定为 opt-in。**

## 3. 可搜索字段

### 3.1 元数据（已在 `cache.db` / `bootstrap` 中，无需新增读取）

| 字段 | 来源 | 示例 | 价值 | 备注 |
| --- | --- | --- | --- | --- |
| `title` | 各来源 | “修复登录超时” | 最高 | agent 生成，可能含 prompt 片段 |
| `project_name` / `workspace_name` | cwd basename | “perch” | 高 | 与 workspace 筛选一致 |
| `project_path` | cwd | `/Users/…/perch` | 高 | 支持按路径片段匹配 |
| `source_label` | 固定映射 | “Claude Code” | 中 | 也可用来源筛选 |
| `model` | 会话模型 | “Claude Sonnet 4.5” | 中 | Qoder 已做键→显示名映射 |
| `git_branch` | 会话记录 | `feat/search` | 中 | 仅部分来源有 |
| `status` / `status_raw` | 源状态 | “运行中” / “busy” | 中 | 状态筛选后续可另做 |
| `agent_kind` | 采集器 | `cli` / `desktop` / `automation` | 低 | 建议映射为中文标签再搜 |
| `source_id` / `key` | 源会话 ID | `d796df52…` | 高 | 精确跳转/排障 |
| `todo.subject` | `task_todo` | “补充单元测试” | 高 | 已落库，直接可搜 |
| `parent_key` | 子会话关系 | — | 低 | 不建议作为文本字段 |

不建议作为文本搜索字段：`created_at` / `updated_at` / `message_count` / `todo_done` / usage 数值
（应做筛选或排序）、`deeplink` / `resume_cmd`（内部命令，误命中噪音大）、`fingerprint`。

### 3.2 会话内容（需要额外读取源数据）

下表为 2026-11-09 本机实测，仅统计“用户可见文本”，已排除 tool、reasoning、system/developer 提示词。
不同机器差异可能很大，设计时必须做上限保护。

| 来源 | 文本位置 | 本机可见文本量 | 是否完整对话 | 说明 |
| --- | --- | --- | --- | --- |
| Qoder | `chat_session_messages.payload_json.text` | ≈3.9 MB / 4,426 条 | 是（user + assistant） | `role` 在 payload 内；`parts` 约 143 MB、`tools` 约 63 MB，均为 tool/附件，**不索引** |
| Claude Code | `projects/**/<session>.jsonl` 的 `message.content` | ≈48 KB（本机会话偏 tool 型；原始 14 MB） | 是 | 只取 `type=text`；跳过 `thinking` / `redacted_thinking` / `tool_use` / `tool_result`；仅顶层会话 |
| Codex | rollout JSONL 的 `response_item.message` + `event_msg.user_message/agent_message` | ≈0.7 MB（user+assistant） | 部分 | 原始 77 MB；跳过 `developer` / `reasoning` / `function_call*`；事件与 response_item 需去重 |
| OpenCode | v2 `session_message.data.content` + 旧 `part.data`（`type=text`） | ≈11.8 MB + 1.7 MB | 是 | 跳过 reasoning / tool / patch；v2 与旧表并存 |
| Kilo | `part.data`（`type=text`） | ≈0.15 MB | 是 | 跳过 reasoning / tool |
| Trae CN | memory JSONL 的 `intent` / `outcome` / `learned` | 约 100 KB 级 | **否**，只有摘要 | hook 事件无正文；`intent` 已作为标题，`outcome`/`learned` 可选 |
| ChatGPT 桌面 | — | — | — | 没有本地会话数据，不支持 |

**结论**：五个来源可以做真正的“按对话内容搜索”；Trae CN 只能做摘要级搜索，应在 UI 中如实标注。

### 3.3 未来字段

`overlay.db`（M2）落地后，人工备注、标签、重命名标题、人工状态也应进入搜索。当前不提前建表。

## 4. 搜索能力分级

### L1 精确/子串（必须）

- 大小写不敏感；Unicode NFKC（全角/半角归一）；中文直接子串匹配，无需分词。
- 多关键词空格分隔，默认 **AND**：每个词至少在任一字段命中。
- 支持双引号短语（可选，成本低）。
- 结果按字段权重排序，而不是简单按时间。

### L2 模糊搜索（P0 已包含）

在 L1 基础上增加：

- **子序列匹配**（拉丁字符）：`clst` 可命中 `claude status`，用于快速输入。
- **拼写容错**：ASCII 词长度 3–5 允许编辑距离 ≤1，≥6 允许 ≤2；中文不做编辑距离（误报高）。
- **拼音搜索**（已确认需要，P0 已实现）：全拼 `denglu` 与首字母 `dl` 均可命中“登录”；中英混排时只取 CJK 与 ASCII 字母。
- 不建议一开始上 SQLite FTS5：
  - 元数据只有约 300 条，Rust 内存扫描 <10 ms；
  - FTS5 `unicode61` 对中文分词不友好，`trigram` 又要求查询 ≥3 字符，1–2 字查询仍要回退；
  - 字段权重、模糊匹配、子序列都要在 SQL 外实现，复杂度大于收益。
  - 触发条件：内容索引超过约 50 MB 或任务超过 2,000 条时，再评估 FTS5 trigram。

### L3 会话内容搜索（建议做，opt-in）

- 搜索范围只包含“用户可见文本”，见 §3.2。
- 返回命中字段 + 片段（建议 ±60 字符，最多 120 字符），高亮查询词。
- 默认关闭；首次开启时说明“会读取本机会话正文，仅在本机内存中处理”，需用户确认。
- 结果排序低于标题/项目命中；用户消息片段权重高于 assistant。

### L4 语义搜索（可行，但建议后置）

**能力判断**：技术上可行，价值真实（同义改写、中英混合检索），但不适合首版。

| 方案 | 体积/依赖 | 中文效果 | 隐私 | 复杂度 | 结论 |
| --- | --- | --- | --- | --- | --- |
| 内置 ONNX 小模型（如 bge-small-zh / multilingual-e5-small） | +约 100–500 MB 模型/运行时 | 好 | 全本地 | 高（模型分发、推理、升级） | 可选，后置 |
| macOS `NLEmbedding` / `NLContextualEmbedding` | 无模型下载 | 中等，需验证 | 全本地 | 中高（ObjC/Swift 桥接） | 可评估，不作为首版 |
| 本地 OpenAI 兼容端点（Ollama / llama.cpp `/v1/embeddings`） | 无内置模型 | 取决于模型 | 仅 127.0.0.1 | 中 | **后置首选**（opt-in） |
| 云端 embedding API | 无 | 好 | 违反“无网络”原则 | 低 | 拒绝 |

**建议**：语义搜索放到 M5 之后，作为可选实验能力：
- 只对“会话摘要”做 embedding（标题 + 首条用户消息 + 最近 2 条消息 + todos），不对全文做；
- 关键词与向量结果用 RRF/加权融合，UI 标注“语义（beta）”；
- 不内置模型；先支持本地端点，保持“默认不联网”。

## 5. 推荐方案

### 5.1 分阶段

| 阶段 | 范围 | 状态 |
| --- | --- | --- |
| P0 | 元数据搜索：标题/项目/路径/待办/模型/分支/状态/来源/类型/会话 ID；NFKC + 小写归一；多词 AND + 子串 + 拼音 + 拉丁子序列/编辑距离；含归档全部会话；⌘F；前 10 条 | **已实现（2026-11-09）** |
| P1 | 内容搜索 opt-in：五个来源提取器 + 内存索引 + 片段 + 失效重建 | 待实现 |
| P2 | 规模优化：持久化索引 / FTS5（仅在实测查询变慢后启用） | 待评估 |
| P3 | 语义搜索（可选实验，本地端点） | 待评估 |

### 5.2 架构与 IPC

搜索统一由 Rust 侧完成，前端只负责输入与渲染：

```
App.svelte ──invoke("search_tasks", {query, source?, workspace?})──▶ src-tauri
                                                                      │
                                              perch-core::search ──▶ cache.db（元数据/待办）
```

- P0 只读取 `cache.db` 的 `task` / `task_todo`；搜索词不落库、不写日志。
- 前端已有 `bootstrap` 全量任务，搜索只返回命中 `task_key` + 分数 + 命中字段标签，列表复用现有行组件渲染。
- 输入防抖 150 ms；响应带序号防护，避免旧查询覆盖新结果。
- P1 将扩展同一命令（增加内容开关与片段字段），不新增前端数据通道。

当前命令结构：

```rust
// 请求
{ "query": "登录 超时", "source": null, "workspace": null }

// 响应
{
  "hits": [
    { "task_key": "claude:...", "score": 24.6, "fields": ["标题", "待办"] }
  ],
  "total": 3
}
```

`total` 是截断前的命中总数；`hits` 最多 10 条。

### 5.3 排序

当前字段权重：

| 字段 | 权重 |
| --- | --- |
| 标题 | 6 |
| 项目 / 项目路径 | 4 |
| 待办（合并 subject） | 3 |
| 模型 / 分支 / 状态 / 来源 | 2 |
| 类型 / 会话 ID | 1 |

匹配质量系数：精确 1.0、前缀 0.9、子串 0.8；ASCII 词另支持 token 精确 0.85 / 前缀 0.75 / 编辑距离 0.55 / 子序列 0.45；拼音全拼 1.0 / 0.85 / 0.7，首字母 0.8 / 0.6。字段得分 = 质量 × 权重，每个查询词取最高字段，多词相加。

加成：最近活动 `0.6 / (1 + 天数/30)`，运行中 +0.4，源置顶 +0.2。排序为分数倒序 → `updated_at` 倒序 → key 字典序；不再单独把“运行中”提到最前（仅保留加分）。

### 5.4 UI 交互

- 侧边栏「任务」分组内、「已归档」下方是搜索入口；点击入口或按 `⌘F` 后，在列表头部展开实际搜索框并聚焦。
- 搜索框 placeholder：`搜索标题、项目、待办…`；`Esc` 先清空查询，再按一次收起搜索框；点击左侧视图按钮也会收起并清空。
- 输入即搜（150 ms 防抖）。
- 查询非空时列表切换为“搜索结果”，行下显示命中字段（如 `匹配 标题 · 待办`）；复用现有任务行，不引入新组件。
- 归档、子会话、自动化命中时在行内显示对应徽标，避免“搜到了但不知道是什么”。
- 无结果空状态：`没有匹配“…”的任务` + 可搜索字段说明；搜索中显示 `搜索中…`；失败显示错误与重试提示。
- 点击命中行仍打开 Inspector；清空搜索后回到原视图与原筛选。

### 5.5 搜索范围与视图关系

已确认并实现：

- 查询非空时**忽略视图**（最近 / 更早 / 已归档的 7 天窗口），在全部会话中搜索。
- **默认包含已归档、子会话、自动化会话**；搜索结果用徽标标注。
- workspace / 来源筛选继续叠加，与现有交互一致；副标题显示 `全部会话（含已归档）` 与当前筛选。
- 结果先返回 10 条，但 `total` 保留真实命中数，副标题显示 `N 个匹配 · 显示 10 条`。
- 清空搜索后回到原视图，不改变用户的视图选择；点击左侧视图按钮会同时清空搜索。
- P1 内容搜索加入后，搜索范围仍遵循同一规则，内容开关只控制是否读正文。

### 5.6 内容索引设计（P1）

**先做内存索引，不落盘**，避免在 `cache.db` 复制一份会话正文：

```rust
struct ContentIndex {
    built_at: f64,
    entries: Vec<ContentEntry>,          // 每个任务一条
}
struct ContentEntry {
    task_key: String,
    source: String,
    segments: Vec<ContentSegment>,       // 按消息分段，便于片段展示
}
struct ContentSegment {
    role: String,                        // user / assistant
    text: String,                        // 原始文本，已截断
    folded: String,                      // NFKC + lower，用于匹配
}
```

- 上限：单条消息 8,000 字符、单会话 200,000 字符；超出截断并标记。
- 跳过：system/developer、thinking/reasoning、tool 输入输出、附件、疑似 base64/二进制。
- 去重：相同 `folded` 片段只保留一次（Codex 的 response_item 与 event_msg 会重复）。
- 失效：复用 `SourceProbe` 的变更信号；对应来源变化时只重建该来源的条目。
- 首次内容搜索建立索引（本机约 300 MB 原始 JSON/DB，预计 1–3 秒），UI 显示“正在建立内容索引…”；
  后续查询只做内存扫描，目标 <100 ms。
- 内存占用目标：<50 MB（按可见文本约 20 MB 估算）。

**未来持久化选项**：单独的 `search.db`（可删除重建），或 FTS5 trigram 外部内容表；
仅在实测查询变慢后启用，并同步修订 SECURITY。

### 5.7 隐私与安全

- 元数据搜索：不改变现有边界，始终可用。
- 内容搜索：默认关闭；只读、只在内存、无日志、无网络；关闭开关即释放索引。
- 搜索词和片段不写入 `cache.db`、不写日志、不进入错误信息。
- 发布内容搜索前必须修订：
  - README“当前不包含：全文搜索”；
  - SECURITY“不读取消息正文”改为“默认不读取；开启内容搜索后仅在本机内存中读取用户可见文本”；
  - FAQ 增加关闭方法与数据边界说明。
- 截图/demo 使用合成 fixture，不出现真实标题、路径或正文。

### 5.8 测试

- Rust 单测（`perch-core::search`，已实现 8 个）：归一化、多词 AND、拼音全拼/首字母、拉丁子序列与编辑距离、待办/会话 ID、归档/子会话、10 条上限与 `total`、来源/workspace 筛选。
- 前端：`svelte-check` 通过；demo 模式已实现 `search_tasks`（子串 + 多词 AND + 前 10 条），并人工验证搜索、归档命中、筛选联动、`⌘F`/`Esc`、清除。
- P1 增加后：每个来源的 fixture，断言只提取用户可见文本、tool/reasoning/system 不被命中；关闭内容搜索时完全不读取正文。

## 6. 风险与限制

| 风险 | 影响 | 处置 |
| --- | --- | --- |
| 内容搜索与“只读不读正文”的既有承诺冲突 | 信任/文档不一致 | 默认关闭 + 明确 opt-in + 修订文档 |
| 原始数据体量大（Qoder 212 MB、Codex 77 MB） | 首次索引慢、内存高 | 只提取可见文本；分来源懒加载；上限与截断 |
| Trae CN 无正文 | 搜索覆盖不完整 | UI 标注“摘要级”；不承诺全文 |
| 中文分词/模糊误报 | 结果噪音 | 默认子串；模糊仅对拉丁；中文不做编辑距离 |
| schema 漂移 | 提取器失败 | 单源 warning；保留元数据搜索；Diagnostics 记录 |
| 正文包含密钥/隐私 | 泄露风险 | 不落盘、不日志、不联网；截图用合成数据 |
| 搜索范围与视图不一致 | 用户困惑 | 固定范围提示“全部会话（含已归档）”；视图按钮清空搜索 |

## 7. 已确认决策

| 决策 | 结论 |
| --- | --- |
| 搜索范围 | 默认含归档，搜索所有会话；忽略最近/更早/归档视图 |
| 模糊强度 | 需要拼音；子序列 + 英文拼写容错一并实现 |
| 结果上限 | 首版只返回 10 条，`total` 保留真实命中数 |
| 内容搜索 | 后置为 P1，显式 opt-in、默认关闭、只在本机内存处理 |
| 索引落盘 | P1 首版不落盘；查询变慢后再评估 `search.db` / FTS5 |
| 语义搜索 | 放到 M5 之后，仅本地端点实验，不内置模型 |
| Trae CN | 只承诺摘要级搜索，UI 如实标注 |
| 搜索实现位置 | 统一在 Rust（`perch-core::search`），前端只输入与渲染 |

## 8. 实测附录

采集时间：2026-11-09，只读查询本机数据。

- Qoder：`chat_session_messages` 4,426 行；`payload.text` 约 3.9 MB；`payload_json` 总长约 212 MB；
  `parts` 143 MB、`tools` 63 MB（不索引）。
- Claude Code：14 个顶层 JSONL，原始 14 MB；用户可见文本约 48 KB（本机会话以 tool 为主）。
- Codex：77 MB rollout；`response_item.message` 文本约 2.2 MB（其中 developer 1.5 MB 不索引），
  `event_msg` 用户/助手消息约 0.16 MB。
- OpenCode：`part` 33,403 行，`text` 约 1.7 MB；v2 `session_message` 11,074 行，assistant 内容文本约 11.8 MB。
- Kilo：`part` 2,899 行，`text` 约 0.15 MB。
- Trae CN：memory JSONL 约 104 KB，字段为 `intent` / `actions` / `outcome` / `learned`，无完整正文。
