# 产品规格：用量报告应用内嵌展示

| 项 | 内容 |
|---|---|
| 原需求卡 | REQ-0001 |
| 文档类型 | 产品规格 |
| 当前版本 | v0.1.0 |
| 适用范围 | 当前产品行为与验收规格 |
| 关联 | [CHG-0003](../exec-plans/completed/CHG-0003-in-app-usage-report.md)、[真实体验确定性缺陷修复](../exec-plans/completed/2026-11-22-real-ux-clarity-fixes.md) |

## 1. 摘要

Perch 侧边栏与任务导航提供 usage-ledger 用量报告入口。报告由 usage-ledger CLI（`usage html`）渲染为单文件 HTML，Perch 只负责生成并在应用内嵌视图展示，不复制报表逻辑。报告是全局视图，不按任务筛选变化；usage-ledger 保持唯一渲染源。

## 2. 问题与证据

- 谁、什么场景：Perch 用户查看 token/credits 用量报表。
- 现在怎么做、代价是什么：点侧边栏报告按钮 → 生成 HTML → 跳到默认浏览器，离开应用上下文。
- 证据：用户反馈希望在应用内查看（2026-11-19 会话）；2026-11-22 真实体验反馈指出报告视图缺少独立导航态与返回路径。
- 不做的后果：查看用量需切换应用，报表与任务视图割裂。

## 3. 目标与非目标

- 目标：报告在 Perch 应用内展示；报表内容与 `usage html` 输出完全一致；usage-ledger 升级报表时 Perch 自动跟进。
- 非目标（当前默认不做，后续可 opt-in）：Svelte 原生重做报表；报表与任务列表双向联动；报表主题与 Perch 主题统一。

## 4. 成功指标

- 主指标：打开报告不再启动外部浏览器（基线：每次打开都跳浏览器 → 目标：0 次）。
- 护栏指标：报告内容与 `usage html --output` 产物一致；生成失败时错误可见。
- 验证方式：实机冒烟 + demo 模式验收（见 CHG-0003）。

## 5. 当前需求与验收（EARS）

- R1：When 用户点击侧边栏或任务导航中的报告入口, the system shall 在应用内报表视图展示最新生成的 usage-ledger HTML 报告，不启动外部浏览器；报表视图铺满列表与详情两列（报表无详情面板，不显示详情栏），并用导航选中态、`aria-current` 与“全局报告 · 不随任务筛选变化”说明当前位置。
- R2：When 用户在报表视图触发刷新（按钮或 ⌘R）, the system shall 重新生成报告并更新展示。
- R3：While 报表视图打开且 usage 数据发生变化, the system shall 自动重新生成并更新报告。
- R4：If 报告生成失败, then the system shall 在应用内展示错误信息，不打开浏览器。
- R5：When 用户在报告视图点击“返回任务”, the system shall 回到进入报告前的最近任务视图，且不遗留报告视图外的 Inspector 选中任务。
- R6：If 未检测到 usage-ledger 可用数据, then the system shall 在报告入口给出不可用的可读说明，点击后仍在应用内显示生成错误，不打开浏览器。
- R7：When demo 模式打开报告, the system shall 展示内置示例报告页。

验证位置：R1–R4 实机冒烟（`npm run tauri dev`）；R5–R7 demo（`npm run demo`）；详见 CHG-0003 §5 与本次变更文档 §6。

## 6. 变更索引

| CHG ID | 标题 | 状态 | 链接 |
|---|---|---|---|
| CHG-0003 | 用量报告应用内嵌展示（方案 A） | 已验证 | [../exec-plans/completed/CHG-0003-in-app-usage-report.md](../exec-plans/completed/CHG-0003-in-app-usage-report.md) |
| 2026-11-22 | 真实体验反馈中的确定性缺陷修复 | 已验证 | [../exec-plans/completed/2026-11-22-real-ux-clarity-fixes.md](../exec-plans/completed/2026-11-22-real-ux-clarity-fixes.md) |

## 7. 链接

- 报表渲染源：usage-ledger `src/usage_ledger/html_report.py`（`usage html`）
- 入口代码：`src-tauri/src/lib.rs` `open_usage_report`、`src/App.svelte`