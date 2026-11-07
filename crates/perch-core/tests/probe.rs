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
}
