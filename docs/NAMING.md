# 命名参考：多 agent 任务总览

> 结论：2026-11-02 已确认使用 **Perch**。
> 日期：2026-11-02 ｜ 关联：[DESIGN.md](DESIGN.md)
> 目标：一个能表达“多个 agent 的任务汇到一处、可全局置顶”的名字；短、好念、有画面感，且不与主流 agent / 开发工具重名。

## 1. 命名约束

- **达意**：一听/一看能联想到“汇总、俯瞰、置顶、回到任务”。
- **眼前一亮**：具体名词 > 抽象缩写；有画面感 > 说明文。
- **短**：1–2 个音节最佳；Dock、菜单栏、窗口标题、命令行都放得下。
- **可扩展**：既能做 app 名，也能做 repo / crate / CLI 前缀。
- **低冲突**：避开 Cursor / Copilot / Codex / Claude / Qoder / Trae / Kilo / OpenCode / WorkBuddy 等现有 agent 名；避开 Nest / Hive / Swarm / Crew 等已被 AI 工具用滥的词。
- **中性偏暖**：这是个人效率工具，不是监控/管控工具；避免 Overseer / Warden / Supervisor 这类“监视”语义。

## 2. 三条隐喻线

| 隐喻线 | 画面 | 适合的气质 |
|---|---|---|
| **栖居**（鸟 / 巢 / 枝） | 每个 agent 是一只鸟，各有各的树；Perch 是它们共同落脚的横枝 | 轻、暖、自然；适合个人 app |
| **集结**（集合 / 点名 / 调度） | 把散落的任务召集到一块板前点名 | 明确、利落；适合“总览 / 指挥台” |
| **编织**（线 / 梭 / 布） | 多条 agent 的线，织成一张可看的布 | 优雅、抽象；适合强调“关联” |

## 3. 短名单

### A. 栖居线

| 名字 | 含义 / 画面 | 眼前一亮 | 风险 | 备注 |
|---|---|---|---|---|
| **Perch** | 鸟落脚的横枝；也是“俯瞰点”。All your agents. One perch. | ★★★★★ | 有同名 CMS / 零售分析产品；个人使用无碍 | **推荐** |
| **Roost** | 鸟群夜宿处；任务“归巢” | ★★★★☆ | 英文习语 “come home to roost” 带负面含义；有同名产品 | 强备选 |
| **Aviary** | 百鸟园；多 agent 共居 | ★★★★☆ | Adobe Aviary 曾是大牌图片应用；名字偏长 | 气质优雅 |
| **Aerie / Eyrie** | 高处的猛禽巢，视野极好 | ★★★★☆ | 拼写不唯一；有服装品牌 | 适合“俯瞰” |
| **Bower** | 园亭 / 鸟的求偶亭；安静的一角 | ★★★☆☆ | Bower 包管理器（已弃用） | 温柔但略偏 |

### B. 集结线

| 名字 | 含义 / 画面 | 眼前一亮 | 风险 | 备注 |
|---|---|---|---|---|
| **Muster** | 集合、点名、检阅；“把所有任务 muster 起来” | ★★★★★ | 有物流 / 人力软件同名；词感偏硬 | **第二推荐** |
| **Baton** | 接力棒；任务在 agent 之间交接 | ★★★★☆ | 有金融 / 风控产品同名；偏“交接”而非“总览” | 适合强调 handoff |
| **Switchboard** | 老式电话总机；所有线路汇到一块板 | ★★★★☆ | 名字长；有协作产品同名 | 表意最直白 |
| **Relay** | 接力 / 中继；任务在工具间传递 | ★★★☆☆ | Facebook Relay（GraphQL）重名 | 开发圈歧义 |
| **Atrium** | 建筑中央大厅；所有任务在中庭汇合 | ★★★★☆ | 有法律科技 / 销售运营产品同名 | 优雅、略冷 |

### C. 编织线

| 名字 | 含义 / 画面 | 眼前一亮 | 风险 | 备注 |
|---|---|---|---|---|
| **Loom** | 织布机；多条 agent 的线织成一张总览 | ★★★★★ | Loom 录屏产品太有名 | 意象最好，冲突最大 |
| **Weft** | 纬线；横向穿过所有经线的那根线 | ★★★★☆ | 生僻词，需要解释 | 独特、安静 |
| **Tessera** | 马赛克的一小块；许多任务拼成一幅图 | ★★★★☆ | 生僻；偏“碎片”而非“总览” | 设计感强 |

### D. 直白线（适合做副标题 / 功能名，不建议做主名）

Corkboard / Pinboard / Taskboard / TaskHub / AgentHub / AgentBoard / TaskDeck / TaskDock

- 达意但缺少“眼前一亮”；且 Pinboard（书签服务）、TaskBoard（开源看板）等已有大量同名。
- 适合做功能名：例如 “Global pins” / “Corkboard 视图”。

## 4. 推荐组合

### 已确认：Perch

| 项 | 建议 |
|---|---|
| App 名 | **Perch** |
| 仓库名 | `perch`（2026-11-03 已从 `task-hub` 重命名） |
| 副标题 | **多 Agent 任务总览** |
| 英文 tagline | **All your agents. One perch.** |
| 中文 tagline | **所有 Agent 的任务，落在同一根栖木上。** |
| 图标方向 | 一根横枝 + 两三只极简几何鸟；单色、克制，不用卡通鸟 |
| Bundle ID | `com.wayne.perch`（占位，首次发布前可改） |
| Rust crate | `perch-core`；CLI（P2）：`perch` 或 `perch-cli` |
| DB 环境变量 | `PERCH_CACHE_DB`、`PERCH_OVERLAY_DB`、`PERCH_SOURCE_HOME` |

**为什么是它：**

1. **一词双关**：鸟落枝头 = 任务落脚；perch 也有“置于高处”的动作感，天然贴合“全局置顶”。
2. **画面干净**：一根横枝 + 几只鸟，就是“多个 agent、一个总览”的极简插图，图标好做。
3. **短**：一个音节，菜单栏、命令行、窗口标题都不啰嗦。
4. **不抢 agent 的戏**：它是“栖息处”，不是“新 agent”；语义上不会让用户误以为要在这里聊天。
5. **可扩展**：置顶区可以叫 “On the perch”；分组可以叫 “Branches”；子会话可以叫 “Fledglings”（可选，别过度拟人）。

**风险：**

- Perch 在 CMS、零售分析等领域有同名产品；英文里也是“鲈鱼”。个人本地工具无碍；图标用“横枝 + 鸟”可消除歧义。若未来公开分发，GitHub 仓库用 `perch-tasks` / `perch-app` 区分，App 名可保留。
- 鸟的意象如果做得太可爱，会偏离“效率工具”；图标建议几何、克制。

### 次选：Muster

- App 名：**Muster**
- Tagline：**Every task, mustered.** / **所有任务，在此点名。**
- 气质：更像“指挥台 / 点名册”，比 Perch 硬朗；适合你希望它看起来像工作台而不是宠物。
- 风险：词感偏军事 / 物流；不如 Perch 轻。

### 第三：Roost

- App 名：**Roost**
- Tagline：**All tasks come home.** / **任务归巢。**
- 气质：温暖、有“回家”感；比 Perch 更像“收件箱”。
- 风险：英文习语 “come home to roost” 是贬义（自食其果）；对外传播时略尴尬。

### 如果更想要“交接”而不是“总览”：Baton

- Tagline：**Pass the baton, keep the thread.** / **任务交接，线索不断。**
- 适合未来强调“从一个 agent 转到另一个 agent 继续”的工作流；当前核心是“找到”，所以排后。

## 5. 按气质快速选

| 你想要的感觉 | 选它 |
|---|---|
| 轻、暖、有画面，个人工具 | **Perch** / Roost |
| 工作台、点名、利落 | **Muster** / Switchboard |
| 交接、流动、接力 | Baton / Relay |
| 优雅、设计感、建筑感 | Aviary / Atrium / Tessera |
| 极简、抽象、织物 | Loom / Weft |

## 6. 不推荐

| 名字 | 原因 |
|---|---|
| Nest | Google Nest 强冲突；太泛 |
| Hive | 已被大量 AI / 协作产品使用 |
| Swarm | OpenAI Swarm；且“蜂群”有失控感 |
| Crew | CrewAI 强冲突 |
| Flock | Flock 浏览器 / 团队聊天；且“一群”偏社交 |
| Overseer / Warden / Supervisor | 监控 / 管控语义，不符合个人工具气质 |
| TaskHub / AgentHub | 达意但平庸，且 GitHub 重名多 |
| Cursor / Copilot / Codex … | 与现有 agent 重名，绝对避免 |

## 7. 冲突检查清单（定名前 10 分钟）

- GitHub 搜索同名 repo（按 star 排序，看是否有同领域项目）
- App Store / Setapp 搜索同名 app
- `perch.app` / `perch.dev` / `getperch.com` 等域名是否可用（不一定要买，确认没有被同领域占用）
- USPTO / 中国商标网快速检索（仅公开分发时需要）
- 本机 `mdfind "kMDItemCFBundleIdentifier == '*perch*'"` 看是否与已装 app 冲突

> 结论：**Perch** 作为正式名称足够好；真正发布前再做一轮商标 / 域名检索即可。
