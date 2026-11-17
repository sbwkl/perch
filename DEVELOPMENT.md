# Development

本文面向在本地构建、调试 Perch 的开发者。产品与数据设计见 [docs/DESIGN.md](docs/DESIGN.md)，
代码结构见 [ARCHITECTURE.md](ARCHITECTURE.md)。

## 环境要求

| 依赖 | 版本 | 说明 |
| --- | --- | --- |
| macOS | 14+（已实测） | 依赖系统 WKWebView；更低版本未验证 |
| Xcode Command Line Tools | 最新 | `xcode-select --install` |
| Rust | stable | 通过 [rustup](https://rustup.rs/) 安装；开发环境使用 1.9x |
| Node.js | 20+ | 开发环境使用 Node 24 / npm 11 |

首次构建 Tauri 会编译较多 crate，耗时明显长于后续增量构建。

## 常用命令

```bash
npm install              # 安装前端依赖
npm run tauri dev        # 启动 Tauri 开发窗口（Vite 1420 + Rust 后端）
npm run check            # svelte-check 类型检查
npm run build            # 前端生产构建
npm run tauri build      # 打包 Perch.app / .dmg
npm run demo             # 仅前端演示模式（端口 1421，假数据，不启动 Tauri）

cargo fmt --all --check
cargo clippy -p perch-core --all-targets -- -D warnings
cargo test -p perch-core
```

`perch-core` 是纯 Rust crate，不依赖 Tauri，可在无窗口环境中单独测试。

## 项目结构

```
src/                     Svelte 5 + TypeScript 前端
  App.svelte             三栏界面、视图与筛选状态
  lib/types.ts           IPC 数据类型
  lib/source-icons.ts    各来源品牌图标（生成物）
src-tauri/               Tauri 2 壳
  src/lib.rs             bootstrap / refresh_sources / continue_task、3 秒 probe 循环
crates/perch-core/       采集、缓存、查询、继续工作决策
  src/collectors/        七个来源的只读采集器
  src/store.rs           cache.db schema 与 upsert
  src/sync.rs            ScanEngine 扫描事务
  src/query.rs           任务视图与 BootstrapPayload
  src/usage.rs           只读 usage-ledger，按会话聚合 token / credits / est
  src/probe.rs           SQLite data_version / JSONL 签名探测
  src/continue_work.rs   打开 App / 终端恢复的决策
demo/                    开发专用演示模式与截图夹具
docs/                    设计、逐源规格、FAQ、截图
```

## 数据路径

| 数据 | 路径 |
| --- | --- |
| 任务快照缓存 | `~/Library/Application Support/app.perch.desktop/cache.db` |
| Qoder 源 | `~/Library/Application Support/com.qoder.app.stable/main.sqlite` |
| Claude 源 | `~/.claude/` |
| Codex 源 | `~/.codex/` |
| OpenCode 源 | `~/.local/share/opencode/opencode.db`（回退 `opencode-stable.db`） |
| Kilo 源 | `~/.local/share/kilo/kilo.db` |
| usage-ledger（可选） | `~/.local/share/usage-ledger/usage.db` |

`cache.db` 是完全可重建的派生数据：退出 Perch 后删除 `cache.db`、`cache.db-wal`、`cache.db-shm`，
下次启动会重新扫描。不要删除或修改任何源文件。

### 环境变量覆盖

测试和特殊安装位置可用以下变量覆盖默认路径：

| 变量 | 作用 |
| --- | --- |
| `PERCH_SOURCE_HOME` | 替换所有默认路径的 home 基准 |
| `PERCH_QODER_DB` | Qoder 数据库路径 |
| `PERCH_CLAUDE_DIR` | Claude 数据目录 |
| `PERCH_CODEX_DIR` | Codex 数据目录 |
| `PERCH_OPENCODE_DB` | OpenCode 主数据库 |
| `PERCH_OPENCODE_STABLE_DB` | OpenCode 回退数据库 |
| `PERCH_KILO_DB` | Kilo 数据库 |
| `PERCH_CACHE_DB` | Perch 自己的 `cache.db` 路径 |
| `PERCH_USAGE_DB` | usage-ledger 的 `usage.db` 路径（优先于下面两个） |
| `USAGE_LEDGER_DB` | 与 usage-ledger 自身一致的数据库覆盖变量 |
| `USAGE_LEDGER_HOME` | usage-ledger 数据目录；默认 `~/.local/share/usage-ledger` |

集成测试使用临时目录 + 这些变量构造最小 fixture，不会读取真实用户数据。

## 演示模式

`demo/` 是一套仅用于开发和文档截图的假数据入口：它用 Vite alias 把
`@tauri-apps/api/core` / `event` 替换成 stub，注入 `demo/fixture.ts` 中的
`BootstrapPayload`，因此不需要启动 Tauri，也不会读取真实任务。

```bash
npm run demo
# http://localhost:1421/
```

支持通过查询参数驱动界面，便于截图：

```
http://localhost:1421/?view=已归档
http://localhost:1421/?select=接入 OAuth 登录回调
```

截图要求：

- 只使用 `demo/` 的假数据，**不得**把真实会话标题、路径或项目名提交进仓库。
- 更新 `docs/images/` 后检查图片中不含个人信息。
- `docs/images/social-preview.png` 由 `demo/social.html` 渲染，尺寸固定 1280×640。

## 测试与检查

提交前至少运行：

```bash
npm run check
cargo fmt --all --check
cargo clippy -p perch-core --all-targets -- -D warnings
cargo test -p perch-core
```

CI（`.github/workflows/ci.yml`）在 Linux 上执行相同的检查，使用 `perch-core` 的单元/集成测试；
macOS 打包与 Tauri 壳的实机行为目前靠手工冒烟验证。

## 常见问题

**启动后列表为空**

先看侧边栏来源行是否显示 `unavailable`。确认对应 agent 已安装并产生过会话，且默认路径存在；
也可以用环境变量指向测试数据。点击右上角刷新（`⌘R`）重新扫描。

**窗口已在运行，但数据不更新**

实时刷新只在窗口可见时工作（最小化或隐藏会暂停）。`bootstrap` 读缓存，`refresh_sources`
才会主动扫描；自动探测每 3 秒运行一次，源变化后重扫该来源。

**端口 1420 被占用**

`npm run tauri dev` 需要固定端口 1420。结束占用进程，或同时修改 `vite.config.ts` 与
`src-tauri/tauri.conf.json` 的 `devUrl`。

**首次打开 `.app` 被系统拦截**

本地构建未签名/未公证，右键 → 打开，或在“系统设置 → 隐私与安全性”中允许。

**Qoder 的“打开”只把 App 切到前台**

Qoder 不支持会话级深链（`qoder://chat/<id>` 返回 `DEEP_LINK_ROUTE_UNSUPPORTED`），
这是上游限制，详见 [docs/FAQ.md](docs/FAQ.md)。

**如何清空 Perch 数据**

退出应用后删除 `~/Library/Application Support/app.perch.desktop/cache.db*`。不会影响任何 agent。
