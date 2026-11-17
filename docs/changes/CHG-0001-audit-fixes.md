# CHG-0001 项目审计确定性修复

类型：CHG
状态：已验证
Owner：wayne
所属模块/入口：perch-core（collectors/trae_cn、search、store、models、collectors/common）+ docs
功能卡：无（一次性审计清理）

## 问题

全项目审计（2026-11-17）发现的确定性问题：

1. `collectors/trae_cn.rs` `memory_project_name` 的 roots 数组硬编码个人用户名 `"wayne"`，其他用户机器上 Trae memory 会话的工作区名会带上用户名前缀。
2. `search.rs` `status_label` 缺少 `"failed"` 映射（前端 STATUS_LABELS 有 `failed: "失败"`，Cline 会产出 failed），导致按“失败”搜索不到 failed 状态任务。
3. 死代码（全仓库 grep 零调用）：`common::is_recent`、`common::path_key`、`SourceScanStatus::is_success`、`SourceScanStatus::keeps_snapshot`、`CacheStore::recent_scan_runs`（及其唯一产物 `models::ScanRun`）。
4. `store.rs` `save_result` 逐任务 upsert + replace_todos 无事务，与 ARCHITECTURE.md / DESIGN.md “ok 结果在事务中写入”的声明不符；中途失败会留下半写入状态。
5. 过期文档：SOURCES.md、FAQ.md、DESIGN.md 仍称 Trae hook 会话“没有标题、统一显示新会话”，与已实现的 memory intent 标题 + 事件推导状态矛盾；DEVELOPMENT.md / SOURCES.md 环境变量表缺 `PERCH_TRAE_CN_EVENTS_DIR`、`PERCH_TRAE_CN_MEMORY_DIR`、`PERCH_CLINE_DB`、`PERCH_USAGE_BIN` 等已实现变量。

## 改动

1. `memory_project_name`：移除硬编码 `"wayne"`，改为结构化跳过 `Users`/`home` 后的用户名段（对任意用户生效，不依赖环境变量）。
2. `status_label` 增加 `"failed" => "失败"`。
3. 删除上述 5 个死函数与 `ScanRun` 结构体。
4. `save_result` 用单个事务包裹全部 upsert/replace_todos（`unchecked_transaction`，失败自动回滚），使代码与文档一致。
5. 同步 SOURCES.md / FAQ.md / DESIGN.md 的 Trae 描述；补全 DEVELOPMENT.md / SOURCES.md 环境变量表。

不做：审计中不确定项（trae-cn.est 被 usage 桥丢弃、probe 连接不重试、scan_run 无清理等）另列评审清单，不在本次范围。

## 验证

- TDD：先为 1、2 写失败测试（trae_cn.rs 底部单元测试、search.rs 单元测试），确认 red 后实现转 green。
- 2026-11-17 实测：`cargo fmt --all --check` 通过；`cargo check --workspace` 零警告；
  `cargo clippy --all-targets -- -D warnings` 零警告；`cargo test --workspace` 60 个测试全部通过
  （含新增 `memory_project_name_strips_any_username`、`memory_project_name_keeps_home_directory_itself`、
  `matches_failed_status_label`）。
- 既有 collectors/store_query 测试保证 save_result 行为不变；只读不变量测试（源文件字节一致）继续通过。
- 临时分支集成测试后删除。

## 回滚

- 纯代码 revert 即可；无 schema 变更、无数据迁移。事务化只影响写入原子性，缓存数据格式不变。