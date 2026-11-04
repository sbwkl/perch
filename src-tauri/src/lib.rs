use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use perch_core::continue_work::{plan_continue, ContinueAction, ContinuePlan, SystemProbe};
use perch_core::models::{BootstrapPayload, SyncReport};
use perch_core::query::build_bootstrap_with_usage;
use perch_core::{paths, CacheStore, ScanEngine, SourcePaths, SourceProbe, UsageSnapshot};
use tauri::{Emitter, Manager, State};

#[cfg(test)]
use std::os::unix::fs::PermissionsExt;

#[derive(Clone)]
struct AppState {
    engine: Arc<Mutex<ScanEngine>>,
    usage_db: PathBuf,
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

/// Watches the usage archive without reading it. A missing database yields
/// `None`, so the UI only refreshes once usage-ledger creates or updates it.
fn usage_mtime(path: &std::path::Path) -> Option<std::time::SystemTime> {
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    [path.to_path_buf(), wal]
        .iter()
        .filter_map(|candidate| std::fs::metadata(candidate).ok()?.modified().ok())
        .max()
}

#[tauri::command]
async fn refresh_sources(
    app: tauri::AppHandle,
    source: Option<String>,
) -> Result<SyncReport, String> {
    let state = app.state::<AppState>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let engine = state.engine.lock().map_err(|error| error.to_string())?;
        Ok(engine.scan(source.as_deref(), "manual"))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(serde::Serialize)]
struct ContinueOutcome {
    channel: String,
    target: String,
    message: String,
}

#[tauri::command]
fn continue_task(state: State<'_, AppState>, task_key: String) -> Result<ContinueOutcome, String> {
    let task = {
        let engine = state.engine.lock().map_err(|error| error.to_string())?;
        engine
            .store()
            .load_tasks()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|task| task.key == task_key)
            .ok_or_else(|| format!("任务不存在：{task_key}"))?
    };
    let probe = SystemProbe::new();
    let plan = plan_continue(&task, &probe).map_err(|error| error.to_string())?;
    execute_plan(&plan)?;
    Ok(ContinueOutcome {
        channel: plan.channel.as_str().to_owned(),
        target: plan.target.clone(),
        message: match &plan.action {
            ContinueAction::ActivateApp(_) => {
                format!("已切换到 {}（无法定位到具体会话）", plan.target)
            }
            ContinueAction::OpenUrl(_) => format!("已打开 {}", plan.target),
            ContinueAction::Terminal { .. } => "已在终端恢复".to_owned(),
        },
    })
}

fn execute_plan(plan: &ContinuePlan) -> Result<(), String> {
    match &plan.action {
        ContinueAction::OpenUrl(url) => run_open(&[url.as_str()]),
        ContinueAction::ActivateApp(bundle_id) => run_open(&["-b", bundle_id.as_str()]),
        ContinueAction::Terminal { cwd, command } => open_terminal(cwd.as_deref(), command),
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

fn open_terminal(cwd: Option<&str>, command: &str) -> Result<(), String> {
    // Leading space absorbs the first keystroke a shell-startup prompt (e.g. oh-my-zsh update) may swallow.
    let mut line = String::from(" ");
    if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
        line.push_str(&format!("cd {} && ", shell_quote(cwd)));
    }
    line.push_str(command);

    let script = concat!(
        "on run argv\n",
        "tell application \"Terminal\"\n",
        "do script (item 1 of argv)\n",
        "activate\n",
        "end tell\n",
        "end run\n"
    );
    let output = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", script, "--"])
        .arg(&line)
        .output()
        .map_err(|error| format!("打开终端失败：{error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr);
        Err(format!("打开终端失败：{}", detail.trim()))
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Sources usage-ledger's `html` command accepts. Mirrors the sources Perch
/// knows, so a report can only be generated for data Perch itself displays.
const USAGE_REPORT_SOURCES: [&str; 5] = ["claude", "codex", "kilo", "opencode", "qoder"];

fn usage_report_source(source: Option<&str>) -> Result<Option<&str>, String> {
    match source {
        None | Some("") => Ok(None),
        Some(value) if USAGE_REPORT_SOURCES.contains(&value) => Ok(Some(value)),
        Some(value) => Err(format!("未知来源：{value}")),
    }
}

/// Keeps the per-source reports side by side instead of overwriting one file.
fn usage_report_file(source: Option<&str>) -> String {
    match source {
        Some(value) => format!("usage-report-{value}.html"),
        None => "usage-report.html".to_owned(),
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
    source: Option<String>,
}

/// Runs `usage html --output <cache>/usage-report[-source].html` and returns the
/// written file. Split from the command so tests can drive it with a fake CLI.
fn render_usage_report(bin: &Path, dir: &Path, source: Option<&str>) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let file = dir.join(usage_report_file(source));

    let mut command = std::process::Command::new(bin);
    command.arg("html").arg("--output").arg(&file);
    if let Some(value) = source {
        command.arg("--source").arg(value);
    }
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
/// directory and opens it in the default browser, mirroring `usage html --open`.
/// The usage database is only read by usage-ledger itself.
#[tauri::command]
async fn open_usage_report(
    app: tauri::AppHandle,
    source: Option<String>,
) -> Result<UsageReportOutcome, String> {
    let source = usage_report_source(source.as_deref())?.map(str::to_owned);
    let bin = usage_bin()?;
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?;

    let report_source = source.clone();
    let file = tauri::async_runtime::spawn_blocking(move || {
        render_usage_report(&bin, &dir, report_source.as_deref())
    })
    .await
    .map_err(|error| error.to_string())??;

    let path = file.to_string_lossy().to_string();
    run_open(&[path.as_str()])?;
    Ok(UsageReportOutcome { path, source })
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
            });

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut probe = SourceProbe::new(&paths);
                let mut usage_stamp = usage_mtime(&usage_db);
                loop {
                    std::thread::sleep(Duration::from_secs(3));
                    if let Some(window) = handle.get_webview_window("main") {
                        if window.is_minimized().unwrap_or(false)
                            || !window.is_visible().unwrap_or(true)
                        {
                            continue;
                        }
                    }
                    let changed = probe.poll();
                    let next_usage_stamp = usage_mtime(&usage_db);
                    let usage_changed = next_usage_stamp != usage_stamp;
                    if changed.is_empty() && !usage_changed {
                        continue;
                    }
                    if !changed.is_empty() {
                        let state = handle.state::<AppState>();
                        let Ok(engine) = state.engine.lock() else {
                            continue;
                        };
                        for source in changed {
                            engine.scan(Some(source), "live");
                        }
                    }
                    usage_stamp = next_usage_stamp;
                    let _ = handle.emit("sources-updated", ());
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            refresh_sources,
            continue_task,
            open_usage_report
        ])
        .run(tauri::generate_context!())
        .expect("error while running Perch");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_report_source_accepts_only_perch_sources() {
        assert_eq!(usage_report_source(None).unwrap(), None);
        assert_eq!(usage_report_source(Some("")).unwrap(), None);
        assert_eq!(usage_report_source(Some("qoder")).unwrap(), Some("qoder"));
        assert!(usage_report_source(Some("trae-cn")).is_err());
    }

    #[test]
    fn usage_report_file_is_kept_per_source() {
        assert_eq!(usage_report_file(None), "usage-report.html");
        assert_eq!(usage_report_file(Some("codex")), "usage-report-codex.html");
    }

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

        let file = render_usage_report(&bin, &out, None).unwrap();
        assert_eq!(file, out.join("usage-report.html"));
        assert_eq!(
            std::fs::read_to_string(&record).unwrap(),
            format!("html\n--output\n{}\n", file.display())
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "<html></html>");

        let scoped = render_usage_report(&bin, &out, Some("qoder")).unwrap();
        assert_eq!(scoped, out.join("usage-report-qoder.html"));
        assert!(std::fs::read_to_string(&record)
            .unwrap()
            .contains("--source\nqoder\n"));
    }

    #[cfg(unix)]
    #[test]
    fn render_usage_report_surfaces_cli_failures() {
        let temp = tempfile::TempDir::new().unwrap();
        let bin = temp.path().join("usage");
        std::fs::write(&bin, "#!/bin/sh\necho 'database is locked' >&2\nexit 2\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();

        let error = render_usage_report(&bin, &temp.path().join("cache"), None).unwrap_err();
        assert!(error.contains("database is locked"), "{error}");
    }
}
