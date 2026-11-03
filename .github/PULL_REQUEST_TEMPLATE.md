## 概要

<!-- 这个 PR 解决什么问题？为什么这样改？ -->

## 改动类型

- [ ] Bug fix
- [ ] New feature
- [ ] Collector / data source
- [ ] Refactor
- [ ] Documentation / CI

## 只读检查

- [ ] 没有新增对源 SQLite / JSONL 的写路径
- [ ] 没有把消息正文、prompt、tool 输出或凭据写入缓存/日志
- [ ] 失败时保留旧快照，没有误标 `source_missing`
- [ ] `continue_task` 仍只使用固定模板命令并校验 session id

## 验证

<!-- 跑了哪些检查？UI 改动如何手工验证？ -->

- [ ] `npm run check`
- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy -p perch-core --all-targets -- -D warnings`
- [ ] `cargo test -p perch-core`

## 截图（如有）

<!-- 只使用 demo/ 假数据；不要包含真实会话标题或路径 -->
