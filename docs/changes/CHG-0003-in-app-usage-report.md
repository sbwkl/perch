# CHG-0003 用量报告应用内嵌展示（方案 A）

类型：CHG
状态：开发中
Owner：wayne
所属模块/入口：src-tauri（lib.rs、tauri.conf.json、capabilities）、前端（App.svelte）、demo
功能卡：[REQ-0001](../requirements/REQ-0001-usage-report.md)
关联：用户 2026-11-19 批准方案 A（iframe 嵌入现有 HTML）；分支 feat/in-app-usage-report（基于 main 1b3d50c）

## 1. 摘要

把"生成 usage-ledger HTML 报告后跳外部浏览器"改为在 Perch 主窗口内用 iframe 展示同一份 HTML，usage-ledger 仍是唯一渲染源。

## 2. 变更理由与证据

- 对应 REQ-0001 的问题：查看用量需离开应用。
- 方案比较（2026-11-19 会话）：A iframe 嵌入（改动最小、零重复）；B Svelte 原生重做（双份实现、需同步维护）；C 抽共享渲染层（需重构 usage-ledger，成本最高）。用户选定 A。
- 可行性证据：报告为单文件 HTML，JS/CSS 全内联、无外部资源请求；localStorage 访问均有 try/catch 兜底（html_report.py 主题与面板状态），存储不可用时优雅降级。

## 3. 行为 / AC delta

- 新增：REQ-0001 R1–R5（应用内展示、手动刷新、usage 变化自动刷新、失败提示、demo 示例页）。
- 补充（2026-11-19 用户反馈）：R1 布局细化——报表视图铺满第 2、3 列（隐藏 inspector），避免报表被限制在列表栏宽度内过窄。
- 修改：`open_usage_report` 不再调用 `run_open` 打开浏览器，仅渲染并返回文件路径（命令名与返回结构不变）。
- 删除：无。
- 边界：报告生成期间按钮禁用并显示加载态；asset 协议经 tauri Cargo feature `protocol-asset` + `app.security.assetProtocol.scope` 启用，scope 仅限 `$APPCACHE/**`，不开放其他目录（tauri 2.12.5 的 asset 协议由配置 scope 管控，core ACL 无 asset 权限项，capabilities 不变）。

## 4. 影响范围与非目标

- 受影响：`open_usage_report` 命令行为；主窗口新增 report 视图；Tauri asset 协议启用（scope 受限）；demo stub 与示例报告页。
- 本次明确不做：原生报表、报表↔任务联动、主题统一、独立报告窗口（A2）。
- 兼容性：无 schema/数据变更；`UsageReportOutcome.path` 结构不变。

## 5. 验证与观测

- 自动化：`cargo test --workspace`（render_usage_report 既有用例保持通过）；`cargo check`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --all --check` 零告警；`npm run check`、`npm run build`。
- 手工（demo，浏览器可自动验收）：`npm run demo` → 点报告按钮 → iframe 展示示例页；刷新重渲染；切走再切回正常。
- 手工（实机冒烟）：`npm run tauri dev` → 点报告按钮 → 报告在应用内渲染、无浏览器弹出；⌘R 重新生成；usage.db 变化后自动更新；卸载 usage CLI 场景显示错误横幅（可用 PATH 覆盖模拟）。
- 重点验证：WKWebView asset 协议下 iframe 内联脚本执行、主题切换、面板状态 localStorage 是否可用（不可用属预期降级，记录结论）。
- 结果与证据（2026-11-19，worktree feat/in-app-usage-report）：
  - 自动化：`cargo fmt --all --check`、`cargo check --workspace`、`cargo clippy --all-targets -- -D warnings` 零告警；`cargo test --workspace` 57 通过（perch_lib 2、perch_core 20、claude_status 5、collectors 16、probe 4、store_query 6、usage 4）；`npm run check` 0 errors 0 warnings；`npm run build` 成功。
  - R1：实机点报告按钮 → 主窗口 iframe 渲染真实报告（Usage Ledger 头部、汇总卡片可见），全程无浏览器启动。截图 /tmp/perch-smoke/win2.png、gate-b.png（含真实用量数据，仅本地留存，不入库）。
  - R2：点"重新生成"→ 报告 mtime 17:18:02→17:18:20；门控修复后复验 mtime 1795080994→1795081087。⌘R 路径在 demo 验证。
  - R3：sqlite 修改副本 db 内容（`UPDATE usage_turn SET input_tokens=input_tokens+12345`）→ 约 7s 内自动重生成一次（mtime 1795080987→1795080994），随后 59s 观察无自激循环。
  - 无误触发：任务源持续 churn 52s（含本会话自身写入）期间报告 mtime 保持 1795080925 不变，iframe 不重载。
  - R4：`PATH=/usr/bin:/bin` + 空 HOME 模拟 CLI 缺失 → 应用内错误横幅"未找到 usage 命令：usage-ledger 未安装，或不在 PATH 中"（/tmp/perch-smoke/err9.png），不打开浏览器。
  - R5：demo iframe 加载 /demo-usage-report.html，内联脚本执行（#generated 时间戳写入、主题切换按钮在 DOM 中可用）。
  - localStorage 结论：asset:// origin 下进程内可用（深色主题跨 iframe 重建保持，t6/t7.png），进程重启后不持久（LocalStorage 目录为空，t8.png 回浅色）→ 预期优雅降级，主题/面板状态为会话级。
  - 门控修复：初版以 `as_of`（db mtime）门控自动重生成，实测 `usage html` 每次运行自身写 db（store.py 读写连接 + schema 维护、关闭时 WAL checkpoint）导致 mtime 自激、约每 9s 循环重生成。改为 usage 汇总内容指纹（source、token_total、token_7d、credits_billable_7d、est_total、last_day；排除随任务匹配变化的 matched_tasks 与 mtime 派生的 as_of），基线在生成开始时取自当前 payload，生成期间落入的真实写入会在下一次 probe 触发补生成。
  - 布局细化复验（2026-11-19 晚）：demo 525px 视口 DOM 测量——report 视图 main 横跨 214→526（铺满 sidebar 右侧）、inspector 未挂载；切回任务视图 inspector 恢复（312px）。实机 1200×780——report 视图铺满第 2、3 列，筛选行与三列 KPI 卡完整、无横向溢出；切回"最近"恢复三栏（截图 /tmp/perch-smoke/full-now.png、back-now.png，本地留存不入库）。`npm run check` 0 告警、`npm run build` 成功、cargo fmt/clippy/test 复跑全绿。

## 6. 回滚方案

- 单分支 revert；恢复 `run_open` 调用、移除 report 视图与 asset 协议配置即可，无数据迁移。

## 7. 风险与未决问题

- asset 协议 origin 下 localStorage 已实测：进程内可用、跨重启不持久（见 §5），报告 try/catch 兜底生效，降级可接受。
- 同源 iframe 拥有完整权限 → 内容为本地 usage-ledger 生成的可信 HTML，风险可控；如后续报表引入外部内容需加 sandbox。
- 自动刷新门控依赖 bootstrap 的 usage 汇总字段；usage-ledger 若新增汇总维度需同步指纹字段，否则该维度变化不触发自动刷新（手动刷新不受影响）。

## 8. 关联与证据

- commit：8bd04bb（feat/in-app-usage-report）；本文档证据回写为后续 docs-only 提交
- PR：https://github.com/sbwkl/perch/pull/5（待人工审核；合并 main 需用户明确同意）
- 集成测试：tmp/integration-in-app-usage-report（merge commit 543e1b4，基于 main 1b3d50c）合入 8bd04bb 后 `cargo test --workspace` 10 套件全 ok、`npm run check` 0 告警；测毕分支已删
- 测试/截图：/tmp/perch-smoke/（win2、gate-a、gate-b、err9、t6–t8.png；含真实用量数据，仅本地留存不入库）；demo 证据为 DOM 结构检查

## 9. 变更记录

| 状态 | 操作人 | 证据/链接 |
|---|---|---|
| 提议 → 就绪 | wayne + agent | 2026-11-19 会话方案比较，用户批准方案 A |
| 开发中 | agent | 本文档建立，worktree feat/in-app-usage-report |
| 开发中 → 已验证 | agent | 2026-11-19 §5 证据：自动化全绿 + 实机 R1–R4 + demo R5 + 集成测试；PR #5 |