use std::fs;
use std::io::Write;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use perch_core::paths::SourcePaths;
use perch_core::SourceProbe;
use tempfile::TempDir;

fn paths_under(root: &Path) -> SourcePaths {
    SourcePaths {
        home: root.to_path_buf(),
        qoder_db: root.join("qoder.sqlite"),
        claude_dir: root.join("claude"),
        codex_dir: root.join("codex"),
        opencode_db: root.join("opencode.db"),
        opencode_stable_db: root.join("opencode-stable.db"),
        kilo_db: root.join("kilo.db"),
        trae_cn_events_dir: root.join("usage-ledger/trae-cn"),
        trae_cn_memory_dir: root.join("trae-cn/memory/projects"),
        cline_db: root.join("cline/sessions.db"),
        antigravity_ide_root: root.join("gemini/antigravity"),
        antigravity_cli_root: root.join("gemini/antigravity-cli"),
        minimax_dir: root.join("minimax"),
        codearts_db: root.join("codeartsdoer/opencode.db"),
        codebuddy_cn_logs_dir: root.join("codebuddy-cn/logs"),
        codebuddy_ide_logs_dir: root.join("codebuddy-ide/logs"),
        codebuddy_genie_dir: root.join("codebuddy-cn/genie-history"),
    }
}

#[test]
fn probe_reports_changed_sources_once() {
    let temp = TempDir::new().unwrap();
    let paths = paths_under(temp.path());

    let claude_projects = paths.claude_dir.join("projects/-tmp-alpha");
    fs::create_dir_all(&claude_projects).unwrap();
    let session = claude_projects.join("s.jsonl");
    fs::write(&session, "{\"type\":\"user\"}\n").unwrap();

    let qoder = rusqlite::Connection::open(&paths.qoder_db).unwrap();
    qoder
        .execute_batch("CREATE TABLE IF NOT EXISTS t (x INTEGER);")
        .unwrap();

    let mut probe = SourceProbe::new(&paths);
    assert!(probe.poll().is_empty(), "initial baseline must be quiet");

    sleep(Duration::from_millis(20));
    let mut handle = fs::OpenOptions::new().append(true).open(&session).unwrap();
    writeln!(handle, "{{\"type\":\"assistant\"}}").unwrap();
    drop(handle);
    qoder.execute_batch("INSERT INTO t VALUES (1);").unwrap();

    let mut changed = probe.poll();
    changed.sort_unstable();
    assert_eq!(changed, vec!["claude", "qoder"]);

    assert!(probe.poll().is_empty(), "no further changes expected");
}

#[test]
fn probe_detects_claude_custom_title_sidecar_changes_only() {
    use std::time::Instant;

    let temp = TempDir::new().unwrap();
    let paths = paths_under(temp.path());
    let session_dir = paths.claude_dir.join("projects/-tmp-alpha/session-a");
    fs::create_dir_all(&session_dir).unwrap();
    let sidecar = session_dir.join("custom-title.json");
    fs::write(&sidecar, "{\"customTitle\":\"旧标题\"}").unwrap();
    let other_json = session_dir.join("other.json");
    fs::write(&other_json, "{\"x\":1}").unwrap();

    let base = Instant::now();
    let mut probe = SourceProbe::new(&paths);
    probe.reset_baseline(base);

    sleep(Duration::from_millis(20));
    fs::write(&other_json, "{\"x\":2}").unwrap();
    assert!(probe.changed_sources(base).is_empty());

    fs::write(&sidecar, "{\"customTitle\":\"新标题\"}").unwrap();
    assert!(probe
        .changed_sources(base + Duration::from_secs(15))
        .is_empty());
    assert_eq!(
        probe.changed_sources(base + Duration::from_secs(21)),
        vec!["claude"]
    );

    probe.defer_jsonl("claude", base + Duration::from_secs(21));
    sleep(Duration::from_millis(20));
    fs::remove_file(&sidecar).unwrap();
    assert!(probe
        .changed_sources(base + Duration::from_secs(51))
        .is_empty());
    assert_eq!(
        probe.changed_sources(base + Duration::from_secs(66)),
        vec!["claude"]
    );
}

#[test]
fn probe_detects_claude_session_state_changes() {
    let temp = TempDir::new().unwrap();
    let paths = paths_under(temp.path());
    let sessions = paths.claude_dir.join("sessions");
    fs::create_dir_all(&sessions).unwrap();

    let mut probe = SourceProbe::new(&paths);
    assert!(probe.poll().is_empty(), "initial baseline must be quiet");

    fs::write(sessions.join("123.json"), "{\"status\":\"idle\"}").unwrap();
    assert_eq!(probe.poll(), vec!["claude"]);
    assert!(probe.poll().is_empty(), "no further changes expected");
}

#[test]
fn probe_detects_trae_cn_hook_events() {
    let temp = TempDir::new().unwrap();
    let paths = paths_under(temp.path());
    let events = paths.trae_cn_events_dir.clone();
    fs::create_dir_all(&events).unwrap();
    let file = events.join("events-2026-11-07.jsonl");
    fs::write(&file, "{\"session\":\"abc\"}\n").unwrap();

    let mut probe = SourceProbe::new(&paths);
    assert!(probe.poll().is_empty(), "initial baseline must be quiet");

    sleep(Duration::from_millis(20));
    let mut handle = fs::OpenOptions::new().append(true).open(&file).unwrap();
    writeln!(handle, "{{\"session\":\"abc\",\"event\":\"Stop\"}}").unwrap();
    drop(handle);

    assert_eq!(probe.poll(), vec!["trae-cn"]);
    assert!(probe.poll().is_empty(), "no further changes expected");

    let session_dir = paths
        .trae_cn_memory_dir
        .join("-Users-wayne-workspace-playground--p2-abc/20261104");
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(session_dir.join("session_memory_history.jsonl"), "").unwrap();

    assert_eq!(probe.poll(), vec!["trae-cn"]);
    assert!(probe.poll().is_empty(), "no further changes expected");
}

#[test]
fn probe_detects_cline_session_changes() {
    let temp = TempDir::new().unwrap();
    let paths = paths_under(temp.path());
    fs::create_dir_all(paths.cline_db.parent().unwrap()).unwrap();

    let cline = rusqlite::Connection::open(&paths.cline_db).unwrap();
    cline
        .execute_batch("CREATE TABLE sessions (session_id TEXT PRIMARY KEY);")
        .unwrap();

    let mut probe = SourceProbe::new(&paths);
    assert!(probe.poll().is_empty(), "initial baseline must be quiet");

    cline
        .execute("INSERT INTO sessions VALUES ('s1')", [])
        .unwrap();
    assert_eq!(probe.poll(), vec!["cline"]);
    assert!(probe.poll().is_empty(), "no further changes expected");
}

#[test]
fn probe_opens_sqlite_sources_that_appear_after_startup() {
    let temp = TempDir::new().unwrap();
    let paths = paths_under(temp.path());
    let mut probe = SourceProbe::new(&paths);
    assert!(probe.poll().is_empty(), "missing databases stay quiet");

    let qoder = rusqlite::Connection::open(&paths.qoder_db).unwrap();
    qoder.execute_batch("CREATE TABLE t (x INTEGER);").unwrap();

    assert_eq!(
        probe.poll(),
        vec!["qoder"],
        "a database appearing after startup must trigger a scan"
    );
    assert!(probe.poll().is_empty(), "no further changes expected");

    qoder.execute_batch("INSERT INTO t VALUES (1);").unwrap();
    assert_eq!(probe.poll(), vec!["qoder"]);
}

#[test]
fn jsonl_probe_throttles_signature_walks_and_defers_after_scan() {
    use std::time::{Duration, Instant};

    let temp = TempDir::new().unwrap();
    let paths = paths_under(temp.path());
    let projects = paths.claude_dir.join("projects/-tmp-alpha");
    fs::create_dir_all(&projects).unwrap();
    let session = projects.join("s.jsonl");
    fs::write(&session, "{\"type\":\"user\"}\n").unwrap();

    let base = Instant::now();
    let mut probe = SourceProbe::new(&paths);
    probe.reset_baseline(base);
    assert!(probe.changed_sources(base).is_empty());

    // A file changes before the next 15s directory-signature window.
    sleep(Duration::from_millis(20));
    let mut handle = fs::OpenOptions::new().append(true).open(&session).unwrap();
    writeln!(handle, "{{\"type\":\"assistant\"}}").unwrap();
    drop(handle);
    assert!(probe
        .changed_sources(base + Duration::from_secs(3))
        .is_empty());

    // The next signature window records the change; live scan waits for debounce.
    assert!(probe
        .changed_sources(base + Duration::from_secs(15))
        .is_empty());
    let changed = probe.changed_sources(base + Duration::from_secs(21));
    assert_eq!(changed, vec!["claude"]);

    // After a scan, directory signature checks are deferred for the cooldown.
    probe.defer_jsonl("claude", base + Duration::from_secs(21));
    let mut later = fs::OpenOptions::new().append(true).open(&session).unwrap();
    writeln!(later, "{{\"type\":\"assistant\"}}").unwrap();
    drop(later);
    assert!(probe
        .changed_sources(base + Duration::from_secs(36))
        .is_empty());
    assert!(probe
        .changed_sources(base + Duration::from_secs(57))
        .is_empty());
    assert_eq!(
        probe.changed_sources(base + Duration::from_secs(66)),
        vec!["claude"]
    );
}

#[test]
fn live_scan_scheduler_coalesces_and_respects_cooldown() {
    use perch_core::probe::{LiveScanScheduler, LIVE_COOLDOWN, LIVE_DEBOUNCE, LIVE_MAX_WAIT};
    use std::time::{Duration, Instant};

    let base = Instant::now();
    let mut scheduler = LiveScanScheduler::new();
    scheduler.note_changes(&["codebuddy"], base);
    assert!(scheduler.due_sources(base).is_empty());
    scheduler.note_changes(&["codebuddy"], base + Duration::from_secs(3));
    assert!(scheduler
        .due_sources(base + Duration::from_secs(3))
        .is_empty());
    assert_eq!(
        scheduler.due_sources(base + LIVE_DEBOUNCE),
        vec!["codebuddy"]
    );

    scheduler.note_changes(
        &["codebuddy"],
        base + LIVE_DEBOUNCE + Duration::from_secs(1),
    );
    assert!(scheduler.due_sources(base + LIVE_COOLDOWN).is_empty());
    assert_eq!(
        scheduler.due_sources(base + LIVE_DEBOUNCE + LIVE_MAX_WAIT),
        vec!["codebuddy"]
    );
}
