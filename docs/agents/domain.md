# 领域文档

工程类技能探索代码库时应如何消费本仓库的领域文档。本仓库为**单上下文（single-context）**。

## 探索前先读

- 根目录 `GLOSSARY.md`（若存在）。
- `docs/design-docs/adr/`：读与本次工作区域相关的 ADR。
- `docs/design-docs/index.md`：ADR / 设计文档的状态与登记规则；新增 ADR 必须在此登记。

上述文件不存在时**静默跳过**：不要提示缺失，也不要主动建议创建。`/domain-modeling` 技能（经 `/grill-with-docs` 与 `/improve-codebase-architecture` 到达）会在术语或决策真正定案时按需创建。

## 文件结构

```
/
├── GLOSSARY.md
├── docs/
│   ├── agents/                 ← 本目录
│   ├── design-docs/
│   │   ├── index.md            ← ADR / 设计文档索引
│   │   └── adr/                ← 架构决策，按语义化主题命名
│   └── product-specs/          ← 产品行为事实源
└── src/
```

ADR 命名遵循项目规则：按语义化主题命名，不再分配 `ADR-xxxx` 顺序编号；已有历史编号冻结。

## 使用词汇表术语

输出中出现领域概念时（issue 标题、重构提案、假设、测试名），使用 `GLOSSARY.md` 中的定义，不要漂移到词汇表明确避免的近义词。

若所需概念尚未进入词汇表，这是一个信号：要么你在发明项目不用的说法（重新考虑），要么存在真实缺口（留给 `/domain-modeling`）。

## 标记 ADR 冲突

输出与既有 ADR 冲突时，明确指出而不是静默覆盖：

> _与 ADR-0007（event-sourced orders）冲突，但值得重新讨论，因为……_