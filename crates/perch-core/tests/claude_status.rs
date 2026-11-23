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

fn write_entrypoint_transcript(root: &Path, session_id: &str, entrypoint: &str) {
    let project = root.join("projects/-tmp-perch-live");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join(format!("{session_id}.jsonl")),
        format!(
            "{{\"type\":\"user\",\"cwd\":\"/tmp/perch-live\",\"timestamp\":\"2026-10-01T10:00:00Z\",\"entrypoint\":\"{entrypoint}\"}}\n"
        ),
    )
    .unwrap();
}

fn write_desktop_session_index(root: &Path, cli_session_id: &str, desktop_session_id: &str) {
    let dir = root.join("account/org/00000000-0000-4000-8000-000000000001");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(format!("{desktop_session_id}.json")),
        format!(
            "{{\"sessionId\":\"{desktop_session_id}\",\"cliSessionId\":\"{cli_session_id}\",\"title\":\"must not be read\"}}"
        ),
    )
    .unwrap();
}

fn scan_task_with_desktop_index(root: &Path, index_root: &Path) -> TaskSnapshot {
    let result = ClaudeCollector::new(root.to_path_buf())
        .with_desktop_sessions_root(index_root.to_path_buf())
        .scan();
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
fn desktop_entrypoint_with_valid_mapping_yields_session_deeplink() {
    let temp = TempDir::new().unwrap();
    let index = TempDir::new().unwrap();
    write_entrypoint_transcript(temp.path(), SESSION_ID, "claude-desktop-3p");
    write_desktop_session_index(index.path(), SESSION_ID, "local_abc123");

    let task = scan_task_with_desktop_index(temp.path(), index.path());

    assert_eq!(task.agent_kind.as_deref(), Some("desktop"));
    assert_eq!(
        task.deeplink.as_deref(),
        Some("claude://code/continue?session=local_abc123")
    );
    assert_eq!(
        task.resume_cmd.as_deref(),
        Some("cd '/tmp/perch-live' && claude --resume session-live")
    );
}

#[test]
fn empty_entrypoint_does_not_override_desktop_source() {
    let temp = TempDir::new().unwrap();
    let index = TempDir::new().unwrap();
    write_entrypoint_transcript(temp.path(), SESSION_ID, "claude-desktop-3p");
    let path = temp
        .path()
        .join("projects/-tmp-perch-live")
        .join(format!("{SESSION_ID}.jsonl"));
    let mut content = fs::read_to_string(&path).unwrap();
    content.push_str("{\"type\":\"assistant\",\"entrypoint\":\"\"}\n");
    fs::write(&path, content).unwrap();
    write_desktop_session_index(index.path(), SESSION_ID, "local_abc123");

    let task = scan_task_with_desktop_index(temp.path(), index.path());

    assert_eq!(task.agent_kind.as_deref(), Some("desktop"));
    assert_eq!(
        task.deeplink.as_deref(),
        Some("claude://code/continue?session=local_abc123")
    );
}

#[test]
fn desktop_entrypoint_with_archived_mapping_falls_back_to_cli() {
    let temp = TempDir::new().unwrap();
    let index = TempDir::new().unwrap();
    write_entrypoint_transcript(temp.path(), SESSION_ID, "claude-desktop-3p");
    let dir = index
        .path()
        .join("account/org/00000000-0000-4000-8000-000000000001");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("local_abc123.json"),
        format!(
            "{{\"sessionId\":\"local_abc123\",\"cliSessionId\":\"{SESSION_ID}\",\"isArchived\":true}}"
        ),
    )
    .unwrap();

    let task = scan_task_with_desktop_index(temp.path(), index.path());

    assert_eq!(task.agent_kind.as_deref(), Some("cli"));
    assert_eq!(task.deeplink, None);
}

#[test]
fn desktop_entrypoint_without_mapping_falls_back_to_cli() {
    let temp = TempDir::new().unwrap();
    let index = TempDir::new().unwrap();
    write_entrypoint_transcript(temp.path(), SESSION_ID, "claude-desktop-3p");

    let task = scan_task_with_desktop_index(temp.path(), index.path());

    assert_eq!(task.agent_kind.as_deref(), Some("cli"));
    assert_eq!(task.deeplink, None);
}

#[test]
fn mixed_entrypoint_uses_last_record() {
    let temp = TempDir::new().unwrap();
    let index = TempDir::new().unwrap();
    write_entrypoint_transcript(temp.path(), SESSION_ID, "claude-desktop-3p");
    let project = temp.path().join("projects/-tmp-perch-live");
    let path = project.join(format!("{SESSION_ID}.jsonl"));
    let mut content = fs::read_to_string(&path).unwrap();
    content.push_str(
        "{\"type\":\"assistant\",\"timestamp\":\"2026-10-01T10:01:00Z\",\"entrypoint\":\"cli\"}\n",
    );
    fs::write(&path, content).unwrap();
    write_desktop_session_index(index.path(), SESSION_ID, "local_abc123");

    let task = scan_task_with_desktop_index(temp.path(), index.path());

    assert_eq!(task.agent_kind.as_deref(), Some("cli"));
    assert_eq!(task.deeplink, None);
}

#[test]
fn desktop_entrypoint_with_invalid_mapping_falls_back_to_cli() {
    let temp = TempDir::new().unwrap();
    let index = TempDir::new().unwrap();
    write_entrypoint_transcript(temp.path(), SESSION_ID, "claude-desktop-3p");
    write_desktop_session_index(index.path(), SESSION_ID, "last");

    let task = scan_task_with_desktop_index(temp.path(), index.path());

    assert_eq!(task.agent_kind.as_deref(), Some("cli"));
    assert_eq!(task.deeplink, None);
    assert_ne!(task.title, "must not be read");
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
