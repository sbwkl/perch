use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use perch_core::continue_work::{
    plan_continue, ContinueAction, ContinuePlan, SystemProbe,
};
use perch_core::models::{BootstrapPayload, SyncReport};
use perch_core::query::build_bootstrap;
use perch_core::{paths, CacheStore, ScanEngine, SourcePaths, SourceProbe};
use tauri::{Emitter, Manager, State};

#[derive(Clone)]
struct AppState {
    engine: Arc<Mutex<ScanEngine>>,
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
    let engine = state.engine.lock().map_err(|error| error.to_string())?;
    build_bootstrap(engine.store(), perch_core::collectors::common::now_seconds())
        .map_err(|error| error.to_string())
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
fn continue_task(
    state: State<'_, AppState>,
    task_key: String,
) -> Result<ContinueOutcome, String> {
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

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let paths = SourcePaths::detect();
            let engine = open_engine(app.handle(), &paths)
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            app.manage(AppState {
                engine: Arc::new(Mutex::new(engine)),
            });

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut probe = SourceProbe::new(&paths);
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
                    if changed.is_empty() {
                        continue;
                    }
                    let state = handle.state::<AppState>();
                    let Ok(engine) = state.engine.lock() else {
                        continue;
                    };
                    for source in changed {
                        engine.scan(Some(source), "live");
                    }
                    drop(engine);
                    let _ = handle.emit("sources-updated", ());
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            refresh_sources,
            continue_task
        ])
        .run(tauri::generate_context!())
        .expect("error while running Perch");
}
