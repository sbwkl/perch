# 设计文档索引

本目录是 Perch 的**设计知识库**：收录当前仍有效的源 / 子系统设计、架构决策、RFC、设计历史与核心工程信念。它服务于 agent 的渐进披露：`AGENTS.md` 是地图，根目录 `ARCHITECTURE.md` 是当前实现总图，`docs/product-specs/` 是产品行为事实源；本索引说明“为什么这样设计、哪些结论仍然有效、哪些已被替代”。

## 状态说明

| 状态 | 含义 |
|---|---|
| current | 当前有效，已与产品规格 / `ARCHITECTURE.md` / 代码核对 |
| partial | 部分有效；头部必须同时包含 `superseded_by` 与 `当前事实源` 指针 |
| historical | 历史记录，不作为当前事实或验收标准 |
| proposed | 提案 / RFC，尚未接受 |

## 当前源与子系统设计

| 文档 | 领域 | 状态 | 当前事实源 / 代码入口 |
|---|---|---|---|
| [source-designs/SOURCES.md](source-designs/SOURCES.md) | 各来源字段映射、状态推断、已知限制 | current | `crates/perch-core/src/collectors/`；`ARCHITECTURE.md` |
| [source-designs/SEARCH.md](source-designs/SEARCH.md) | 搜索字段、模糊 / 拼音与隐私边界 | partial | P0 元数据搜索已实现；P1 内容搜索仅为 opt-in 设计 |

## 架构决策 ADR

| 文档 | 领域 | 状态 | 当前事实源 / 代码入口 |
|---|---|---|---|
| [adr/claude-session-title-read-scope.md](adr/claude-session-title-read-scope.md) | Claude 标题读取范围、缓存最小化与 sidecar 生命周期 | partial | 读取范围表述被 [ADR：按需读取会话正文](adr/session-transcript-on-demand-read.md) 部分取代；标题口径仍以 `crates/perch-core/src/collectors/claude.rs`、`agent-sources.md` R1 为准 |
| [adr/session-transcript-on-demand-read.md](adr/session-transcript-on-demand-read.md) | 会话正文按需读取、隐私与缓存边界 | proposed | [session-transcript.md](../product-specs/session-transcript.md)；`SECURITY.md` |

只有已做出、难回退且影响架构 / 数据模型 / 安全边界的决策才建 ADR；文件按主题命名。

## RFC

- [rfcs/README.md](rfcs/README.md) — RFC 触发条件与模板；当前无 RFC。

## 历史文档

- [history/v0.3.3-design-review.md](history/v0.3.3-design-review.md) — v0.3.3 历史设计稿；当前实现以 `ARCHITECTURE.md` 为准。
- [history/naming.md](history/naming.md) — 命名决策与设计讨论记录，仅用于追溯。

## 更新规则

- 新增设计文档必须在本索引登记状态、领域、当前事实源与代码入口。
- `partial` 文档不得删除事实源指针；头部同时写清 `superseded_by` 与 `当前事实源`。
- 历史文档不得作为验收标准；当前产品行为以 `docs/product-specs/` 为准，当前实现以 `ARCHITECTURE.md` 为准。
- 历史 `ADR-xxxx` 编号冻结；新增 ADR / RFC / 设计文档使用语义化主题路径。
