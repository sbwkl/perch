# 2026-11-27 列表状态对齐与详情精简

| 项 | 内容 |
| --- | --- |
| 状态 | 开发中 |
| 类型 | 变更文档 |
| 关联产品规格 | [agent-sources.md R24](../../product-specs/agent-sources.md)、[session-transcript.md T13](../../product-specs/session-transcript.md) |
| 关联设计 | [GLOSSARY.md](../../../GLOSSARY.md) |
| 关联 ticket | 无 |

## 背景

真实使用中收到四个体验问题：

1. 同一列表里 Claude Code（有对话图标）与 Trae CN（无对话图标）并存时，状态胶囊右边界错开一个图标宽度（28px），状态列不整齐。
2. 详情面板的「激活 X / 打开 X」按钮随来源品牌色变化，同一控件在不同 Agent 下视觉不一致。
3. 会话详情展示 usage-ledger 用量解释区块，与侧边栏用量 pill 和全局报告重复，占去详情空间。
4. 会话详情底部常驻「Perch 只读取源元数据，不会修改任何 agent 数据。」，与对话视图「不显示常驻只读小字」的口径不一致。

四项均为界面呈现调整；不修改源数据、不修改继续工作通道与命令边界。

## 目标行为

- 每个任务行固定保留 26px 对话图标列；有正文能力的来源渲染可点击图标，无正文能力的来源留空占位但仍不渲染 `.task-transcript`、不显示解释文字。状态胶囊在所有来源右边界一致。
- 详情面板的继续工作按钮退回统一中性样式：`background: var(--accent)`、`color: var(--bg)`，没有内联来源色、没有来源色描边；文案、hover / disabled 反馈、点击调用与错误提示均不变。
- 会话详情删除整个 usage-ledger 用量区块（标题、token / credits / est 三轨、空态与「四轨口径不同」说明）；侧边栏用量 pill 与全局报告入口不变。
- 会话详情删除底部只读说明文字；只读语义继续由 AGENTS.md、ARCHITECTURE.md、GLOSSARY.md 与文档承载，产品实现不变。
- 本变更不改变领域词表：任务、来源、会话、继续工作、深链打开、激活 App、未决通道的定义保持有效，无需新增术语或 ADR。

## 验收

### 正常路径

1. Claude Code 与 Trae CN / CodeBuddy 任务行混排时，三类状态胶囊右边界偏差 ≤ 1px；Claude Code 有对话图标，Trae CN / CodeBuddy 无对话图标。
2. 分别选中 Claude Code / 激活类与 Codex / 打开类会话，按钮文案仍按通道分别显示「激活 X」「打开 X」；按钮背景、文字颜色、描边、圆角、字号、字重一致，且无来源色内联样式。
3. 任一会话详情不再出现「用量 usage-ledger · 只读」区块、三轨用量、空态说明或四轨说明。
4. 任一会话详情不再出现「Perch 只读取源元数据，不会修改任何 agent 数据。」。

### 异常与边界

1. 继续工作失败时仍显示危险色错误提示；成功仍静默。
2. 按钮 hover 仍有 `brightness(1.06)`，disabled 仍降透明度并不可点击。
3. 无正文来源的图标占位不是按钮，不可聚焦、不可点击；不引入新的可访问性标签。
4. 侧边栏来源筛选下的 token / credits / est pill、页脚用量报告入口、报告视图不受影响。

## 回滚

回滚会恢复状态列错位、来源品牌色按钮、详情用量块与只读脚注。改动仅限前端与文档，不涉及源 agent 数据、`cache.db` schema 或 Rust 命令；直接回退提交即可。

## 验证记录

- 2026-11-27：新增 `demo/cdp/u1-list-inspector-ux.mjs`，先于实现验证到三条失败（状态胶囊偏差 28px、按钮内联来源色不同、usage-block 存在）。
- 2026-11-27：实现后 `bash scripts/gates.sh` 六条门禁全部通过（cargo fmt / clippy / perch-core 测试 / perch lib 测试 / svelte-check / vite build）。
- 2026-11-27：`npm run cdp` 全部 11 个套件通过；新增 U1 套件 3 条覆盖状态右边界、按钮统一样式与详情区块删除。
