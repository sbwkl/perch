use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use perch_core::continue_work::{plan_continue, valid_task_key, ContinueAction, ContinuePlan};
use perch_core::models::{
    transcript_supported, BootstrapPayload, SyncReport, TaskSnapshot, TranscriptOutcome,
};
use perch_core::probe::LiveScanScheduler;
use perch_core::query::build_bootstrap_with_usage;
use perch_core::transcript::read_transcript;
use perch_core::{paths, CacheStore, ScanEngine, SourcePaths, SourceProbe, UsageSnapshot};
use tauri::{Emitter, Manager, State};

#[cfg(test)]
use std::os::unix::fs::PermissionsExt;

#[derive(Clone)]
struct AppState {
    engine: Arc<Mutex<ScanEngine>>,
    usage_db: PathBuf,
    live_paused: Arc<AtomicBool>,
    manual_scanning: Arc<AtomicBool>,
    live_generation: Arc<Mutex<u64>>,
}

#[derive(Clone, serde::Serialize)]
struct ScanStatus {
    source: String,
    trigger: String,
    phase: String,
    duration_ms: Option<u64>,
    live_paused: bool,
}

fn emit_scan_status(app: &tauri::AppHandle, status: ScanStatus) {
    let _ = app.emit("scan-status", status);
}

fn scan_duration_ms(started: &Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

fn should_emit_sources_updated(due_count: usize, usage_changed: bool) -> bool {
    due_count > 0 || usage_changed
}

struct ManualGuard {
    manual_scanning: Arc<AtomicBool>,
    live_generation: Arc<Mutex<u64>>,
}

impl ManualGuard {
    fn enter(state: &AppState) -> Result<Self, String> {
        let mut generation = state
            .live_generation
            .lock()
            .map_err(|error| error.to_string())?;
        *generation += 1;
        state.manual_scanning.store(true, Ordering::SeqCst);
        Ok(Self {
            manual_scanning: state.manual_scanning.clone(),
            live_generation: state.live_generation.clone(),
        })
    }
}

impl Drop for ManualGuard {
    fn drop(&mut self) {
        self.manual_scanning.store(false, Ordering::SeqCst);
        if let Ok(mut generation) = self.live_generation.lock() {
            *generation += 1;
        }
    }
}

fn cache_db_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Some(path) = paths::cache_db_override() {
        return Ok(path);
    }
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir.join("cache.db"))
}

fn open_engine(app: &tauri::AppHandle, paths: &SourcePaths) -> Result<ScanEngine, String> {
    let path = cache_db_path(app)?;
    let store = CacheStore::open(&path).map_err(|error| error.to_string())?;
    Ok(ScanEngine::new(store, paths))
}

#[tauri::command]
fn bootstrap(state: State<'_, AppState>) -> Result<BootstrapPayload, String> {
    let now = perch_core::collectors::common::now_seconds();
    let engine = state.engine.lock().map_err(|error| error.to_string())?;
    let usage = UsageSnapshot::load(&state.usage_db, now);
    build_bootstrap_with_usage(engine.store(), now, usage.as_ref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn refresh_sources(
    app: tauri::AppHandle,
    source: Option<String>,
) -> Result<SyncReport, String> {
    let state = app.state::<AppState>().inner().clone();
    let guard = ManualGuard::enter(&state)?;
    let started = Instant::now();
    emit_scan_status(
        &app,
        ScanStatus {
            source: source.clone().unwrap_or_else(|| "all".to_owned()),
            trigger: "manual".to_owned(),
            phase: "start".to_owned(),
            duration_ms: None,
            live_paused: state.live_paused.load(Ordering::SeqCst),
        },
    );
    let manual_source = source.clone().unwrap_or_else(|| "all".to_owned());
    let result = tauri::async_runtime::spawn_blocking(move || {
        let engine = state.engine.lock().map_err(|error| error.to_string())?;
        Ok(engine.scan(source.as_deref(), "manual"))
    })
    .await
    .map_err(|error| error.to_string())?;
    emit_scan_status(
        &app,
        ScanStatus {
            source: manual_source,
            trigger: "manual".to_owned(),
            phase: "end".to_owned(),
            duration_ms: Some(scan_duration_ms(&started)),
            live_paused: app.state::<AppState>().live_paused.load(Ordering::SeqCst),
        },
    );
    drop(guard);
    result
}

#[derive(serde::Serialize)]
struct LiveRefreshState {
    paused: bool,
}

#[tauri::command]
fn set_live_refresh(state: State<'_, AppState>, paused: bool) -> LiveRefreshState {
    if let Ok(mut generation) = state.live_generation.lock() {
        *generation += 1;
    }
    state.live_paused.store(paused, Ordering::SeqCst);
    LiveRefreshState { paused }
}

#[tauri::command]
fn search_tasks(
    state: State<'_, AppState>,
    query: String,
    source: Option<String>,
    workspace: Option<String>,
) -> Result<perch_core::search::SearchResponse, String> {
    let engine = state.engine.lock().map_err(|error| error.to_string())?;
    perch_core::search::search_tasks(
        engine.store(),
        &query,
        source.as_deref(),
        workspace.as_deref(),
    )
    .map_err(|error| error.to_string())
}

/// Continue-work succeeds silently: the fixed channel is already visible on the
/// button, so a success banner would restate it. Failures travel as `Err`.
#[tauri::command]
fn continue_task(state: State<'_, AppState>, task_key: String) -> Result<(), String> {
    let task = {
        let engine = state.engine.lock().map_err(|error| error.to_string())?;
        engine
            .store()
            .load_task(&task_key)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("任务不存在：{task_key}"))?
    };
    let plan = plan_continue(&task).map_err(|error| error.to_string())?;
    execute_plan(&plan)
}

/// 按需读取会话对话正文：单次内存内快照，不缓存正文、不写日志、不联网。
/// 前端只提交任务 key；路径由 Rust 侧从缓存指针解析。
/// async 命令：engine 锁只覆盖快照解析（短临界区），文件读取与解析交给
/// 阻塞线程池，避免大会话（可达 64MB 尾部窗口）占住主线程并与实时扫描抢锁。
#[tauri::command]
async fn read_session_content(
    state: State<'_, AppState>,
    task_key: String,
) -> Result<TranscriptOutcome, String> {
    let task = {
        let engine = state.engine.lock().map_err(|error| error.to_string())?;
        resolve_session_task(engine.store(), &task_key)?
    };
    tauri::async_runtime::spawn_blocking(move || read_session_content_impl(&task))
        .await
        .map_err(|error| format!("读取对话正文失败：{error}"))?
}

/// 短临界区内完成：key 字符集校验与「继续工作」同规 → 快照查找。
/// 拿到快照即应释放 engine 锁；后续能力判定与文件读取不依赖缓存。
fn resolve_session_task(store: &CacheStore, task_key: &str) -> Result<TaskSnapshot, String> {
    if !valid_task_key(task_key) {
        return Err(format!("非法任务 key：{task_key}"));
    }
    store
        .load_task(task_key)
        .map_err(|error| format!("读取任务快照失败：{error}"))?
        .ok_or_else(|| format!("任务不存在：{task_key}"))
}

/// `read_session_content` 的读取半程（锁外执行）：来源能力判定 → 核心读取入口。
/// 非法 key、任务不存在已在 `resolve_session_task` 拒绝；来源不支持返回 Err；
/// 源文件缺失等读取期失败属于本次视图的结果，以 Error outcome 返回。
/// 拆成自由函数以便测试驱动（先例：render_usage_report）。
fn read_session_content_impl(task: &TaskSnapshot) -> Result<TranscriptOutcome, String> {
    if !transcript_supported(&task.source) {
        return Err(format!("来源不支持读取对话正文：{}", task.source));
    }
    Ok(read_transcript(task))
}

fn execute_plan(plan: &ContinuePlan) -> Result<(), String> {
    match &plan.action {
        ContinueAction::OpenUrl(url) => run_open(&[url.as_str()]),
        ContinueAction::ActivateApp(bundle_id) => run_open(&["-b", bundle_id.as_str()]),
    }
}

fn run_open(args: &[&str]) -> Result<(), String> {
    let status = std::process::Command::new("/usr/bin/open")
        .args(args)
        .status()
        .map_err(|error| format!("启动 open 失败：{error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "open 执行失败（退出码 {}）",
            status.code().unwrap_or(-1)
        ))
    }
}

/// usage-ledger is installed as a `usage` shim, usually under `~/.local/bin`
/// (uv tool) or a Homebrew prefix. `PERCH_USAGE_BIN` overrides the lookup.
fn usage_bin() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("PERCH_USAGE_BIN").map(PathBuf::from) {
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "PERCH_USAGE_BIN 指向的文件不存在：{}",
            path.display()
        ));
    }
    if let Some(dirs) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&dirs) {
            let candidate = dir.join("usage");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = PathBuf::from(home).join(".local/bin/usage");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err("未找到 usage 命令：usage-ledger 未安装，或不在 PATH 中".to_owned())
}

#[derive(serde::Serialize)]
struct UsageReportOutcome {
    path: String,
}

/// Runs `usage html --output <cache>/usage-report.html` and returns the written
/// file. Split from the command so tests can drive it with a fake CLI.
fn render_usage_report(bin: &Path, dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let file = dir.join("usage-report.html");

    let mut command = std::process::Command::new(bin);
    command.arg("html").arg("--output").arg(&file);
    let output = command
        .output()
        .map_err(|error| format!("启动 usage 失败：{error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim();
        return Err(if detail.is_empty() {
            format!(
                "usage html 执行失败（退出码 {}）",
                output.status.code().unwrap_or(-1)
            )
        } else {
            format!("usage html 执行失败：{detail}")
        });
    }
    Ok(file)
}

/// Renders usage-ledger's own standalone HTML report into Perch's cache
/// directory and returns its path; the frontend embeds it via the asset
/// protocol instead of opening a browser. The usage database is only read by
/// usage-ledger itself.
#[tauri::command]
async fn open_usage_report(app: tauri::AppHandle) -> Result<UsageReportOutcome, String> {
    let bin = usage_bin()?;
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?;
    let file = tauri::async_runtime::spawn_blocking(move || render_usage_report(&bin, &dir))
        .await
        .map_err(|error| error.to_string())??;

    let path = file.to_string_lossy().to_string();
    Ok(UsageReportOutcome { path })
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let paths = SourcePaths::detect();
            let usage_db = paths::usage_db_path();
            let engine = open_engine(app.handle(), &paths)
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            app.manage(AppState {
                engine: Arc::new(Mutex::new(engine)),
                usage_db: usage_db.clone(),
                live_paused: Arc::new(AtomicBool::new(false)),
                manual_scanning: Arc::new(AtomicBool::new(false)),
                live_generation: Arc::new(Mutex::new(0)),
            });

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut probe = SourceProbe::new(&paths);
                let mut scheduler = LiveScanScheduler::new();
                let mut usage_stamp = perch_core::usage::newest_mtime(&usage_db);
                let mut last_generation = 0u64;
                loop {
                    std::thread::sleep(Duration::from_secs(3));
                    let Some(state) = handle.try_state::<AppState>() else {
                        continue;
                    };
                    if let Some(window) = handle.get_webview_window("main") {
                        if window.is_minimized().unwrap_or(false)
                            || !window.is_visible().unwrap_or(true)
                        {
                            continue;
                        }
                    }
                    let paused = state.live_paused.load(Ordering::SeqCst);
                    let generation = state
                        .live_generation
                        .lock()
                        .map(|value| *value)
                        .unwrap_or(last_generation);
                    if generation != last_generation {
                        probe.reset_baseline(Instant::now());
                        scheduler.clear();
                        last_generation = generation;
                        continue;
                    }
                    let now = Instant::now();
                    let changed = probe.changed_sources(now);
                    let next_usage_stamp = perch_core::usage::newest_mtime(&usage_db);
                    let usage_changed = next_usage_stamp != usage_stamp;
                    if paused || state.manual_scanning.load(Ordering::SeqCst) {
                        probe.poll_throttled();
                        usage_stamp = next_usage_stamp;
                        continue;
                    }
                    scheduler.note_changes(&changed, now);
                    let due = scheduler.due_sources(now);
                    if !due.is_empty() {
                        let Ok(engine) = state.engine.lock() else {
                            continue;
                        };
                        for source in &due {
                            let started = Instant::now();
                            emit_scan_status(
                                &handle,
                                ScanStatus {
                                    source: (*source).to_owned(),
                                    trigger: "live".to_owned(),
                                    phase: "start".to_owned(),
                                    duration_ms: None,
                                    live_paused: state.live_paused.load(Ordering::SeqCst),
                                },
                            );
                            engine.scan(Some(source), "live");
                            probe.defer_jsonl(source, started);
                            emit_scan_status(
                                &handle,
                                ScanStatus {
                                    source: (*source).to_owned(),
                                    trigger: "live".to_owned(),
                                    phase: "end".to_owned(),
                                    duration_ms: Some(scan_duration_ms(&started)),
                                    live_paused: state.live_paused.load(Ordering::SeqCst),
                                },
                            );
                        }
                    }
                    usage_stamp = next_usage_stamp;
                    if should_emit_sources_updated(due.len(), usage_changed) {
                        let _ = handle.emit("sources-updated", ());
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            refresh_sources,
            search_tasks,
            continue_task,
            read_session_content,
            open_usage_report,
            set_live_refresh
        ])
        .run(tauri::generate_context!())
        .expect("error while running Perch");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn fake_usage(dir: &Path, record: &Path) -> PathBuf {
        let bin = dir.join("usage");
        // Mimics `usage html --output <file> [--source <id>]`: records the
        // arguments and writes a placeholder report to the requested path.
        std::fs::write(
            &bin,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nprintf '<html></html>' > \"$3\"\n",
                record.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    #[cfg(unix)]
    #[test]
    fn render_usage_report_invokes_the_cli_and_returns_the_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let record = temp.path().join("args.txt");
        let bin = fake_usage(temp.path(), &record);
        let out = temp.path().join("cache");

        let file = render_usage_report(&bin, &out).unwrap();
        assert_eq!(file, out.join("usage-report.html"));
        assert_eq!(
            std::fs::read_to_string(&record).unwrap(),
            format!("html\n--output\n{}\n", file.display())
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "<html></html>");
    }

    #[cfg(unix)]
    #[test]
    fn render_usage_report_surfaces_cli_failures() {
        let temp = tempfile::TempDir::new().unwrap();
        let bin = temp.path().join("usage");
        std::fs::write(&bin, "#!/bin/sh\necho 'database is locked' >&2\nexit 2\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();

        let error = render_usage_report(&bin, &temp.path().join("cache")).unwrap_err();
        assert!(error.contains("database is locked"), "{error}");
    }
}

#[cfg(test)]
mod read_session_content_tests {
    use super::*;
    use perch_core::models::{SourceResult, TaskSnapshot, TranscriptOutcome, TranscriptRole};

    fn claude_task(source_path: &str) -> TaskSnapshot {
        let mut task = TaskSnapshot::new("claude", "session-cmd");
        task.title = "命令层合成会话".to_owned();
        task.source_path = Some(source_path.to_owned());
        task.refresh_fingerprint();
        task
    }

    fn unsupported_task(source: &str, id: &str) -> TaskSnapshot {
        let mut task = TaskSnapshot::new(source, id);
        task.title = "不支持正文的来源".to_owned();
        task.refresh_fingerprint();
        task
    }

    fn store_with(tasks: &[TaskSnapshot]) -> CacheStore {
        let store = CacheStore::open_in_memory().unwrap();
        for (source, group) in [(
            "claude",
            tasks
                .iter()
                .filter(|t| t.source == "claude")
                .collect::<Vec<_>>(),
        )] {
            if group.is_empty() {
                continue;
            }
            let scan = store.begin_scan(source, "full", "source", "test").unwrap();
            let owned: Vec<TaskSnapshot> = group.into_iter().cloned().collect();
            store
                .save_result(&SourceResult::ok(source, owned, Vec::new()), scan)
                .unwrap();
        }
        for (source, group) in [
            (
                "trae-cn",
                tasks
                    .iter()
                    .filter(|t| t.source == "trae-cn")
                    .collect::<Vec<_>>(),
            ),
            (
                "codebuddy",
                tasks
                    .iter()
                    .filter(|t| t.source == "codebuddy")
                    .collect::<Vec<_>>(),
            ),
            (
                "qoder",
                tasks
                    .iter()
                    .filter(|t| t.source == "qoder")
                    .collect::<Vec<_>>(),
            ),
        ] {
            if group.is_empty() {
                continue;
            }
            let scan = store.begin_scan(source, "full", "source", "test").unwrap();
            let owned: Vec<TaskSnapshot> = group.into_iter().cloned().collect();
            store
                .save_result(&SourceResult::ok(source, owned, Vec::new()), scan)
                .unwrap();
        }
        store
    }

    fn write_session_fixture(path: &std::path::Path) {
        std::fs::write(
            path,
            concat!(
                "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"命令层问题\"}}\n",
                "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[",
                "{\"type\":\"text\",\"text\":\"命令层回答\"}]}}\n",
            ),
        )
        .unwrap();
    }

    #[test]
    fn rejects_illegal_keys_before_touching_the_store() {
        let store = CacheStore::open_in_memory().unwrap();
        for key in [
            "",
            "noseparator",
            "claude:",
            ":session-a",
            "claude:../../etc/passwd",
            "claude:abc; rm -rf /",
            "qoder:http://evil.example",
        ] {
            let error = resolve_session_task(&store, key).unwrap_err();
            assert!(error.contains("非法任务 key"), "{key}: {error}");
        }
    }

    #[test]
    fn reports_missing_tasks_with_a_distinct_error() {
        let store = CacheStore::open_in_memory().unwrap();
        let error = resolve_session_task(&store, "claude:nope").unwrap_err();
        assert!(error.contains("任务不存在"), "{error}");
    }

    #[test]
    fn rejects_sources_without_transcript_support() {
        for task in [
            unsupported_task("trae-cn", "t-1"),
            unsupported_task("codebuddy", "b-1"),
        ] {
            let error = read_session_content_impl(&task).unwrap_err();
            assert!(error.contains("来源不支持读取对话正文"), "{error}");
        }
    }

    #[test]
    fn returns_messages_for_claude_tasks() {
        let temp = tempfile::TempDir::new().unwrap();
        let session = temp.path().join("session-cmd.jsonl");
        write_session_fixture(&session);
        let store = store_with(&[claude_task(session.to_str().unwrap())]);

        let task = resolve_session_task(&store, "claude:session-cmd").unwrap();
        let outcome = read_session_content_impl(&task).unwrap();
        let TranscriptOutcome::Ready {
            messages,
            session_truncated,
        } = outcome
        else {
            panic!("期望 Ready，实际：{outcome:?}");
        };
        assert!(!session_truncated);
        let rendered: Vec<(TranscriptRole, &str)> = messages
            .iter()
            .map(|message| (message.role, message.text.as_str()))
            .collect();
        assert_eq!(
            rendered,
            vec![
                (TranscriptRole::User, "命令层问题"),
                (TranscriptRole::Assistant, "命令层回答"),
            ]
        );
    }

    #[test]
    fn maps_unreadable_source_files_to_error_outcomes() {
        let temp = tempfile::TempDir::new().unwrap();
        let missing = temp.path().join("session-missing.jsonl");
        let store = store_with(&[claude_task(missing.to_str().unwrap())]);

        let task = resolve_session_task(&store, "claude:session-cmd").unwrap();
        let outcome = read_session_content_impl(&task).unwrap();
        let TranscriptOutcome::Error { reason } = outcome else {
            panic!("期望 Error outcome，实际：{outcome:?}");
        };
        assert!(reason.contains("源文件不存在"), "{reason}");
    }

    #[test]
    fn produces_no_writes_to_store_or_source() {
        let temp = tempfile::TempDir::new().unwrap();
        let session = temp.path().join("session-cmd.jsonl");
        write_session_fixture(&session);
        let store = store_with(&[claude_task(session.to_str().unwrap())]);
        let before_tasks = serde_json::to_value(store.load_tasks().unwrap()).unwrap();
        let before_file = std::fs::read(&session).unwrap();

        let task = resolve_session_task(&store, "claude:session-cmd").unwrap();
        read_session_content_impl(&task).unwrap();

        assert_eq!(
            before_tasks,
            serde_json::to_value(store.load_tasks().unwrap()).unwrap()
        );
        assert_eq!(before_file, std::fs::read(&session).unwrap());
    }

    /// SQLite 系来源经命令层直通核心读取入口：指针列留空，读取器按配置路径定位。
    #[test]
    fn reads_sqlite_source_tasks_through_the_command_chain() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = temp.path().join("main.sqlite");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE chat_sessions (session_id TEXT PRIMARY KEY, title TEXT NOT NULL);
             CREATE TABLE chat_session_messages (
               session_id TEXT NOT NULL,
               message_id TEXT NOT NULL,
               sequence INTEGER NOT NULL,
               payload_json TEXT NOT NULL,
               PRIMARY KEY (session_id, message_id));",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chat_sessions (session_id, title) VALUES ('ses-cmd', '命令层合成会话')",
            [],
        )
        .unwrap();
        for (id, seq, payload) in [
            (
                "m1",
                1,
                r#"{"id":"m1","role":"user","text":"命令层问题","timestamp":1}"#,
            ),
            (
                "m2",
                2,
                r#"{"id":"m2","role":"assistant","text":"命令层回答","timestamp":2}"#,
            ),
        ] {
            conn.execute(
                "INSERT INTO chat_session_messages (session_id, message_id, sequence, payload_json) \
                 VALUES ('ses-cmd', ?1, ?2, ?3)",
                rusqlite::params![id, seq, payload],
            )
            .unwrap();
        }
        drop(conn);
        let before_bytes = std::fs::read(&db).unwrap();

        let mut task = TaskSnapshot::new("qoder", "ses-cmd");
        task.title = "命令层合成会话".to_owned();
        task.refresh_fingerprint();
        let store = store_with(&[task]);
        std::env::set_var("PERCH_QODER_DB", &db);

        let task = resolve_session_task(&store, "qoder:ses-cmd").unwrap();
        let outcome = read_session_content_impl(&task).unwrap();

        std::env::remove_var("PERCH_QODER_DB");
        let TranscriptOutcome::Ready {
            messages,
            session_truncated,
        } = outcome
        else {
            panic!("期望 Ready，实际：{outcome:?}");
        };
        assert!(!session_truncated);
        let rendered: Vec<(TranscriptRole, &str)> = messages
            .iter()
            .map(|message| (message.role, message.text.as_str()))
            .collect();
        assert_eq!(
            rendered,
            vec![
                (TranscriptRole::User, "命令层问题"),
                (TranscriptRole::Assistant, "命令层回答"),
            ]
        );
        assert_eq!(
            before_bytes,
            std::fs::read(&db).unwrap(),
            "读取前后源库字节必须一致"
        );
    }
}

#[cfg(test)]
mod scan_control_tests {
    use super::*;

    #[test]
    fn sources_updated_is_emitted_only_with_real_changes() {
        assert!(!should_emit_sources_updated(0, false));
        assert!(should_emit_sources_updated(1, false));
        assert!(should_emit_sources_updated(0, true));
        assert!(should_emit_sources_updated(2, true));
    }

    #[test]
    fn manual_guard_blocks_live_scans_and_resets_generation() {
        let state = AppState {
            engine: Arc::new(Mutex::new(ScanEngine::new(
                CacheStore::open_in_memory().unwrap(),
                &SourcePaths::detect(),
            ))),
            usage_db: PathBuf::from("/tmp/perch-synthetic-usage.db"),
            live_paused: Arc::new(AtomicBool::new(false)),
            manual_scanning: Arc::new(AtomicBool::new(false)),
            live_generation: Arc::new(Mutex::new(0)),
        };
        let guard = ManualGuard::enter(&state).unwrap();
        assert!(state.manual_scanning.load(Ordering::SeqCst));
        assert_eq!(*state.live_generation.lock().unwrap(), 1);
        drop(guard);
        assert!(!state.manual_scanning.load(Ordering::SeqCst));
        assert_eq!(*state.live_generation.lock().unwrap(), 2);
    }
}
