use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use perch_core::continue_work::{plan_continue, ContinueAction, ContinuePlan};
use perch_core::models::{BootstrapPayload, SyncReport};
use perch_core::probe::LiveScanScheduler;
use perch_core::query::build_bootstrap_with_usage;
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
