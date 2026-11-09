use std::fs;
use std::path::Path;

use perch_core::collectors::claude::ClaudeCollector;
use perch_core::collectors::Collector;
use perch_core::models::{SourceScanStatus, TaskSnapshot};
use tempfile::TempDir;

const SESSION_ID: &str = "session-live";

fn write_transcript(root: &Path) {
    let project = root.join("projects/-tmp-perch-live");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join(format!("{SESSION_ID}.jsonl")),
        concat!(
            "{\"type\":\"user\",\"cwd\":\"/tmp/perch-live\",\"timestamp\":\"2026-10-01T10:00:00Z\"}\n",
            "{\"type\":\"assistant\",\"cwd\":\"/tmp/perch-live\",\"timestamp\":\"2026-10-01T10:01:00Z\",",
            "\"message\":{\"model\":\"claude-sonnet-4-5\"}}\n"
        ),
    )
    .unwrap();
}

fn write_live_state(root: &Path, pid: u32, status: &str) {
    let dir = root.join("sessions");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(format!("{pid}.json")),
        format!("{{\"pid\":{pid},\"sessionId\":\"{SESSION_ID}\",\"status\":\"{status}\"}}"),
    )
    .unwrap();
}

fn scan_task(root: &Path) -> TaskSnapshot {
    let result = ClaudeCollector::new(root.to_path_buf()).scan();
    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    result
        .tasks
        .into_iter()
        .find(|task| task.source_id == SESSION_ID)
        .unwrap()
}

#[test]
fn live_busy_session_maps_to_running() {
    let temp = TempDir::new().unwrap();
    write_transcript(temp.path());
    write_live_state(temp.path(), std::process::id(), "busy");

    let task = scan_task(temp.path());

    assert_eq!(task.status_source.as_deref(), Some("running"));
    assert_eq!(task.status_raw.as_deref(), Some("busy"));
}

#[test]
fn live_waiting_session_maps_to_waiting() {
    let temp = TempDir::new().unwrap();
    write_transcript(temp.path());
    write_live_state(temp.path(), std::process::id(), "waiting");

    let task = scan_task(temp.path());

    assert_eq!(task.status_source.as_deref(), Some("waiting"));
    assert_eq!(task.status_raw.as_deref(), Some("waiting"));
}

#[test]
fn live_idle_session_is_explicit_idle() {
    let temp = TempDir::new().unwrap();
    write_transcript(temp.path());
    write_live_state(temp.path(), std::process::id(), "idle");

    let task = scan_task(temp.path());

    assert_eq!(task.status_source.as_deref(), Some("idle"));
    assert_eq!(task.status_raw.as_deref(), Some("idle"));
}

#[test]
fn missing_live_state_falls_back_to_idle() {
    let temp = TempDir::new().unwrap();
    write_transcript(temp.path());

    let task = scan_task(temp.path());

    assert_eq!(task.status_source.as_deref(), Some("idle"));
    assert_eq!(task.status_raw, None);
}

#[test]
fn dead_live_process_is_ignored() {
    let temp = TempDir::new().unwrap();
    write_transcript(temp.path());
    write_live_state(temp.path(), 2_000_000_000, "busy");

    let task = scan_task(temp.path());

    assert_eq!(task.status_source.as_deref(), Some("idle"));
    assert_eq!(task.status_raw, None);
}
