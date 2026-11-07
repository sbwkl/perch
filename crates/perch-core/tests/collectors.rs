use std::fs;
use std::path::Path;

use perch_core::collectors::claude::ClaudeCollector;
use perch_core::collectors::codex::CodexCollector;
use perch_core::collectors::kilocode::KiloCollector;
use perch_core::collectors::opencode::OpenCodeCollector;
use perch_core::collectors::qoder::QoderCollector;
use perch_core::collectors::trae_cn::TraeCnCollector;
use perch_core::collectors::Collector;
use perch_core::models::{SourceScanStatus, SOURCE_TRAE_CN};
use rusqlite::Connection;
use tempfile::TempDir;

fn create_qoder_db(path: &Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE chat_sessions (
          session_id TEXT PRIMARY KEY,
          origin_session_id TEXT,
          session_kind TEXT,
          owner_session_id TEXT,
          workspace_id TEXT,
          title TEXT,
          cwd TEXT,
          git_branch TEXT,
          model TEXT,
          created_at INTEGER,
          updated_at INTEGER,
          archived INTEGER NOT NULL DEFAULT 0,
          unread INTEGER NOT NULL DEFAULT 0,
          deleted_at INTEGER,
          extra_json TEXT
        );
        CREATE TABLE chat_session_messages (session_id TEXT, role TEXT);
        CREATE TABLE chat_session_turn_states (
          revision INTEGER NOT NULL DEFAULT 1,
          session_id TEXT NOT NULL,
          turn_id TEXT NOT NULL,
          status TEXT NOT NULL
        );
        CREATE TABLE chat_session_sidebar_placements (
          session_id TEXT,
          work_directory TEXT,
          sort_order INTEGER NOT NULL DEFAULT 0,
          pinned INTEGER NOT NULL DEFAULT 0,
          updated_at INTEGER NOT NULL
        );
        "#,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chat_sessions VALUES ('s1', NULL, 'chat', NULL, 'w1', 'Alpha 任务', '/Users/x/work/alpha', \
         'main', 'qwen3.5', 1700000000000, 1700003600000, 0, 0, NULL, \
         '{\"titleSource\":\"ai\",\"provisionalTitle\":\"临时\"}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chat_sessions VALUES ('s2', 's1', 'side_chat', 's1', 'w1', NULL, '/Users/x/work/alpha', \
         NULL, NULL, 1700000100000, 1700003700000, 1, 1, NULL, \
         '{\"provisionalTitle\":\"侧聊任务\"}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chat_sessions VALUES ('s3', NULL, 'chat', NULL, 'w1', '已删除', '/tmp', \
         NULL, NULL, 1700000000000, 1700000000000, 0, 0, 1700000001000, NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chat_sessions VALUES ('s4', NULL, 'chat', NULL, NULL, '沙箱任务', \
         '/Users/wayne/Documents/Qoder/2026-09-18/9f830de4', \
         NULL, NULL, 1700000200000, 1700003800000, 0, 0, NULL, NULL)",
        [],
    )
    .unwrap();
    let now_ms = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "INSERT INTO chat_sessions VALUES ('s5', NULL, 'chat', NULL, 'w1', '等待回复的任务', \
         '/Users/x/work/alpha', NULL, NULL, ?1, ?1, 0, 0, NULL, NULL)",
        [now_ms],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chat_session_messages VALUES ('s1', 'user'), ('s1', 'assistant'), ('s1', 'user'), \
         ('s2', 'user'), ('s5', 'user')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chat_session_turn_states (session_id, turn_id, status) VALUES ('s1', 't1', 'running')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chat_session_sidebar_placements VALUES ('s2', '/Users/x/work/alpha', 0, 1, 1700000002000)",
        [],
    )
    .unwrap();
    drop(conn);
}

#[test]
fn qoder_collector_maps_tasks_and_keeps_source_read_only() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("main.sqlite");
    create_qoder_db(&db_path);
    let before = fs::read(&db_path).unwrap();

    let result = QoderCollector::new(db_path.clone()).scan();

    assert_eq!(result.status, SourceScanStatus::Ok);
    assert_eq!(result.tasks.len(), 4);
    let alpha = result
        .tasks
        .iter()
        .find(|task| task.source_id == "s1")
        .unwrap();
    assert_eq!(alpha.title, "Alpha 任务");
    assert_eq!(alpha.title_source, "ai");
    assert_eq!(alpha.project_path.as_deref(), Some("/Users/x/work/alpha"));
    assert_eq!(alpha.project_name.as_deref(), Some("alpha"));
    assert_eq!(alpha.status_source.as_deref(), Some("running"));
    assert_eq!(alpha.message_count, Some(3));
    assert!(!alpha.source_archived);
    assert!(alpha.parent_key.is_none());

    let side = result
        .tasks
        .iter()
        .find(|task| task.source_id == "s2")
        .unwrap();
    assert_eq!(side.parent_key.as_deref(), Some("qoder:s1"));
    assert_eq!(side.title, "侧聊任务");
    assert_eq!(side.title_source, "provisional");
    assert_eq!(side.status_source.as_deref(), Some("archived"));
    assert!(side.source_archived);
    assert!(side.source_unread);
    assert!(side.source_pinned);

    let sandbox = result
        .tasks
        .iter()
        .find(|task| task.source_id == "s4")
        .unwrap();
    assert_eq!(sandbox.title, "沙箱任务");
    assert_eq!(sandbox.project_path, None, "sandbox cwd is not a project");
    assert_eq!(sandbox.project_name, None);
    assert_eq!(sandbox.status_source.as_deref(), Some("idle"));

    let waiting = result
        .tasks
        .iter()
        .find(|task| task.source_id == "s5")
        .unwrap();
    assert_eq!(waiting.status_source.as_deref(), Some("running"));
    assert_eq!(waiting.status_raw.as_deref(), Some("waiting_reply"));

    let after = fs::read(&db_path).unwrap();
    assert_eq!(
        before, after,
        "collector must not modify the source database"
    );
}

fn write_claude_fixture(root: &Path) {
    let projects = root.join("projects/-Users-x-work-alpha");
    fs::create_dir_all(&projects).unwrap();
    let session_a = projects.join("session-a.jsonl");
    fs::write(
        &session_a,
        concat!(
            "{\"type\":\"user\",\"cwd\":\"/Users/x/work/alpha\",\"timestamp\":\"2026-10-01T10:00:00Z\"}\n",
            "{\"type\":\"assistant\",\"cwd\":\"/Users/x/work/alpha\",\"gitBranch\":\"main\",\
             \"timestamp\":\"2026-10-01T10:01:00Z\",\"message\":{\"model\":\"claude-sonnet-4-5\"}}\n",
            "{\"type\":\"user\",\"isSidechain\":true,\"timestamp\":\"2026-10-01T10:02:00Z\"}\n",
            "{\"type\":\"ai-title\",\"aiTitle\":\"AI 标题\",\"sessionId\":\"session-a\"}\n",
            "{\"type\":\"custom-title\",\"customTitle\":\"人工标题\",\"sessionId\":\"session-a\"}\n"
        ),
    )
    .unwrap();

    let session_b = projects.join("session-b.jsonl");
    fs::write(
        &session_b,
        concat!(
            "{\"type\":\"user\",\"cwd\":\"/Users/x/work/alpha\",\"timestamp\":\"2026-10-03T09:00:00Z\"}\n",
            "{\"type\":\"ai-title\",\"aiTitle\":\"第二个 AI 标题\",\"sessionId\":\"session-b\"}\n"
        ),
    )
    .unwrap();

    let subagents = projects.join("session-a/subagents");
    fs::create_dir_all(&subagents).unwrap();
    fs::write(
        subagents.join("agent-1.jsonl"),
        "{\"type\":\"user\",\"cwd\":\"/tmp\",\"timestamp\":\"2026-10-01T11:00:00Z\"}\n",
    )
    .unwrap();

    let tasks = root.join("tasks/session-a");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(
        tasks.join("1.json"),
        "{\"id\":\"t1\",\"subject\":\"写测试\",\"description\":\"不要读取这段\",\"status\":\"completed\"}",
    )
    .unwrap();
    fs::write(
        tasks.join("2.json"),
        "{\"id\":\"t2\",\"subject\":\"验证只读\",\"status\":\"pending\"}",
    )
    .unwrap();
}

#[test]
fn claude_collector_reads_titles_todos_and_ignores_subagents() {
    let temp = TempDir::new().unwrap();
    write_claude_fixture(temp.path());
    let file_a = temp
        .path()
        .join("projects/-Users-x-work-alpha/session-a.jsonl");
    let before = fs::read(&file_a).unwrap();

    let result = ClaudeCollector::new(temp.path().to_path_buf()).scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    assert_eq!(result.tasks.len(), 2);
    assert_eq!(result.todos.len(), 2);
    let first = result
        .tasks
        .iter()
        .find(|task| task.source_id == "session-a")
        .unwrap();
    assert_eq!(first.title, "人工标题");
    assert_eq!(first.title_source, "custom");
    assert_eq!(first.message_count, Some(2));
    assert_eq!(first.model.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(first.git_branch.as_deref(), Some("main"));
    assert_eq!(first.todo_total, Some(2));
    assert_eq!(first.todo_done, Some(1));
    assert_eq!(
        first.resume_cmd.as_deref(),
        Some("cd '/Users/x/work/alpha' && claude --resume session-a")
    );

    let second = result
        .tasks
        .iter()
        .find(|task| task.source_id == "session-b")
        .unwrap();
    assert_eq!(second.title, "第二个 AI 标题");
    assert_eq!(second.title_source, "ai");

    let after = fs::read(&file_a).unwrap();
    assert_eq!(before, after, "collector must not modify Claude JSONL");
}

fn write_codex_fixture(root: &Path) {
    let active = root.join("sessions/2026/10/rollout-a.jsonl");
    fs::create_dir_all(active.parent().unwrap()).unwrap();
    fs::write(
        &active,
        "{\"timestamp\":\"2026-10-01T08:00:00Z\",\"type\":\"session_meta\",\"payload\":{\
         \"session_id\":\"a\",\"id\":\"a\",\"timestamp\":\"2026-10-01T08:00:00Z\",\
         \"cwd\":\"/Users/x/work/beta\",\"originator\":\"Codex Desktop\",\"thread_source\":\"user\",\
         \"base_instructions\":{\"provenance\":{\"model\":\"gpt-5.2\"}}}}\n",
    )
    .unwrap();

    let archived = root.join("archived_sessions/rollout-a.jsonl");
    fs::create_dir_all(archived.parent().unwrap()).unwrap();
    fs::write(
        &archived,
        "{\"timestamp\":\"2026-10-05T08:00:00Z\",\"type\":\"session_meta\",\"payload\":{\
         \"session_id\":\"a\",\"id\":\"a\",\"timestamp\":\"2026-10-05T08:00:00Z\",\
         \"cwd\":\"/Users/x/work/beta\",\"originator\":\"codex-tui\",\"thread_source\":\"user\"}}\n",
    )
    .unwrap();

    let automation = root.join("sessions/2026/10/rollout-b.jsonl");
    fs::write(
        &automation,
        "{\"timestamp\":\"2026-10-04T08:00:00Z\",\"type\":\"session_meta\",\"payload\":{\
         \"id\":\"b\",\"timestamp\":\"2026-10-04T08:00:00Z\",\"cwd\":\"/Users/x/work/beta\",\
         \"originator\":\"codex_exec\",\"thread_source\":\"automation\"}}\n",
    )
    .unwrap();

    fs::write(
        root.join("session_index.jsonl"),
        "{\"id\":\"a\",\"thread_name\":\"索引标题\",\"updated_at\":\"2026-10-02T00:00:00Z\"}\n",
    )
    .unwrap();

    let goals = Connection::open(root.join("goals_1.sqlite")).unwrap();
    goals
        .execute_batch(
            r#"
            CREATE TABLE thread_goals (
              thread_id TEXT PRIMARY KEY,
              goal_id TEXT NOT NULL,
              objective TEXT NOT NULL,
              status TEXT NOT NULL,
              token_budget INTEGER,
              tokens_used INTEGER,
              time_used_seconds INTEGER,
              created_at_ms INTEGER,
              updated_at_ms INTEGER
            );
            "#,
        )
        .unwrap();
    goals
        .execute(
            "INSERT INTO thread_goals VALUES ('b', 'g1', '自动化目标标题', 'blocked', NULL, 120, 5, 0, 0)",
            [],
        )
        .unwrap();
    drop(goals);
}

#[test]
fn codex_collector_dedupes_rollouts_and_applies_goal_status() {
    let temp = TempDir::new().unwrap();
    write_codex_fixture(temp.path());
    let before = fs::read(temp.path().join("session_index.jsonl")).unwrap();

    let result = CodexCollector::new(temp.path().to_path_buf()).scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    assert_eq!(result.tasks.len(), 2);
    let first = result
        .tasks
        .iter()
        .find(|task| task.source_id == "a")
        .unwrap();
    assert_eq!(first.title, "索引标题");
    assert!(!first.source_archived);
    assert_eq!(first.agent_kind.as_deref(), Some("desktop"));
    assert_eq!(first.model.as_deref(), Some("gpt-5.2"));
    assert_eq!(
        first.updated_at,
        Some(
            chrono::DateTime::parse_from_rfc3339("2026-10-02T00:00:00Z")
                .unwrap()
                .timestamp() as f64
        )
    );

    let second = result
        .tasks
        .iter()
        .find(|task| task.source_id == "b")
        .unwrap();
    assert_eq!(second.title, "自动化目标标题");
    assert_eq!(second.status_source.as_deref(), Some("blocked"));
    assert_eq!(second.status_raw.as_deref(), Some("blocked"));
    assert_eq!(second.agent_kind.as_deref(), Some("automation"));

    let after = fs::read(temp.path().join("session_index.jsonl")).unwrap();
    assert_eq!(before, after);
}

#[test]
fn codex_updated_at_uses_latest_rollout_activity() {
    fn epoch(value: &str) -> f64 {
        chrono::DateTime::parse_from_rfc3339(value)
            .unwrap()
            .timestamp() as f64
    }

    let temp = TempDir::new().unwrap();
    let root = temp.path();

    let resumed = root.join("sessions/2026/10/rollout-resumed.jsonl");
    fs::create_dir_all(resumed.parent().unwrap()).unwrap();
    fs::write(
        &resumed,
        concat!(
            "{\"timestamp\":\"2026-10-01T08:00:00Z\",\"type\":\"session_meta\",\"payload\":{\
             \"id\":\"resumed\",\"timestamp\":\"2026-10-01T08:00:00Z\",\"cwd\":\"/Users/x/work/beta\",\
             \"originator\":\"Codex Desktop\",\"thread_source\":\"user\"}}\n",
            "{\"timestamp\":\"2026-11-03T12:48:22Z\",\"type\":\"event_msg\",\
             \"payload\":{\"type\":\"task_complete\"}}\n"
        ),
    )
    .unwrap();

    let unindexed = root.join("sessions/2026/10/rollout-unindexed.jsonl");
    fs::write(
        &unindexed,
        concat!(
            "{\"timestamp\":\"2026-10-08T08:00:00Z\",\"type\":\"session_meta\",\"payload\":{\
             \"id\":\"unindexed\",\"timestamp\":\"2026-10-08T08:00:00Z\",\"cwd\":\"/Users/x/work/beta\",\
             \"originator\":\"codex-tui\",\"thread_source\":\"user\"}}\n",
            "{\"timestamp\":\"2026-10-09T09:30:00Z\",\"type\":\"event_msg\",\
             \"payload\":{\"type\":\"task_complete\"}}\n"
        ),
    )
    .unwrap();

    fs::write(
        root.join("session_index.jsonl"),
        "{\"id\":\"resumed\",\"thread_name\":\"恢复的会话\",\"updated_at\":\"2026-10-02T00:00:00Z\"}\n",
    )
    .unwrap();

    let result = CodexCollector::new(root.to_path_buf()).scan();
    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);

    let resumed = result
        .tasks
        .iter()
        .find(|task| task.source_id == "resumed")
        .unwrap();
    assert_eq!(resumed.created_at, Some(epoch("2026-10-01T08:00:00Z")));
    assert_eq!(
        resumed.updated_at,
        Some(epoch("2026-11-03T12:48:22Z")),
        "过期的 session_index 不能覆盖更新的 rollout 活动时间"
    );

    let unindexed = result
        .tasks
        .iter()
        .find(|task| task.source_id == "unindexed")
        .unwrap();
    assert_eq!(
        unindexed.updated_at,
        Some(epoch("2026-10-09T09:30:00Z")),
        "没有索引条目时应使用最后一条事件时间，而不是会话创建时间"
    );
}

fn source_home() -> String {
    std::env::var("PERCH_SOURCE_HOME")
        .or_else(|_| std::env::var("HOME"))
        .unwrap()
}

#[test]
fn codex_collector_ignores_generated_sandbox_cwd() {
    let temp = TempDir::new().unwrap();
    let cwd = format!("{}/Documents/Codex/2026-10-07/ni-s", source_home());
    let file = temp.path().join("sessions/2026/10/rollout-c.jsonl");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(
        &file,
        format!(
            "{{\"timestamp\":\"2026-10-07T08:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\
             \"id\":\"c\",\"timestamp\":\"2026-10-07T08:00:00Z\",\"cwd\":\"{cwd}\",\
             \"originator\":\"Codex Desktop\",\"thread_source\":\"user\"}}}}\n"
        ),
    )
    .unwrap();

    let result = CodexCollector::new(temp.path().to_path_buf()).scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    let task = result
        .tasks
        .iter()
        .find(|task| task.source_id == "c")
        .unwrap();
    assert_eq!(task.project_path, None, "sandbox cwd is not a project");
    assert_eq!(task.project_name, None);
}

#[test]
fn codex_running_status_follows_rollout_tail() {
    use std::io::Write;

    let temp = TempDir::new().unwrap();
    let file = temp.path().join("sessions/2026/10/rollout-live.jsonl");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(
        &file,
        concat!(
            "{\"timestamp\":\"2026-10-07T08:00:00Z\",\"type\":\"session_meta\",\"payload\":{\
             \"id\":\"live\",\"timestamp\":\"2026-10-07T08:00:00Z\",\"cwd\":\"/Users/x/work/beta\",\
             \"originator\":\"Codex Desktop\",\"thread_source\":\"user\"}}\n",
            "{\"timestamp\":\"2026-10-07T08:00:05Z\",\"type\":\"event_msg\",\
             \"payload\":{\"type\":\"task_started\"}}\n"
        ),
    )
    .unwrap();

    let collector = CodexCollector::new(temp.path().to_path_buf());
    let running = collector.scan();
    let task = running
        .tasks
        .iter()
        .find(|task| task.source_id == "live")
        .unwrap();
    assert_eq!(task.status_source.as_deref(), Some("running"));
    assert_eq!(task.status_raw.as_deref(), Some("task_started"));

    let mut handle = fs::OpenOptions::new().append(true).open(&file).unwrap();
    writeln!(
        handle,
        "{{\"timestamp\":\"2026-10-07T08:00:09Z\",\"type\":\"event_msg\",\
         \"payload\":{{\"type\":\"task_complete\"}}}}"
    )
    .unwrap();
    drop(handle);

    let idle = collector.scan();
    let task = idle
        .tasks
        .iter()
        .find(|task| task.source_id == "live")
        .unwrap();
    assert_eq!(
        task.status_source.as_deref(),
        Some("idle"),
        "task_complete must end the running state: {:?}",
        task.status_raw
    );
}

fn create_session_db(path: &Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE session (
          id TEXT PRIMARY KEY,
          project_id TEXT,
          parent_id TEXT,
          slug TEXT,
          directory TEXT,
          path TEXT,
          title TEXT,
          model TEXT,
          agent TEXT,
          time_created INTEGER,
          time_updated INTEGER,
          time_archived INTEGER
        );
        CREATE TABLE todo (
          session_id TEXT,
          content TEXT,
          status TEXT,
          priority TEXT,
          position INTEGER,
          time_created INTEGER,
          time_updated INTEGER
        );
        "#,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session VALUES ('s1', 'p1', NULL, 'slug', '/Users/x/work/gamma', '/', \
         'Gamma 任务', '{\"id\":\"kimi-k2\",\"providerID\":\"opencode\"}', 'build', \
         1785988035219, 1785994754248, NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session VALUES ('s2', 'p1', 's1', 'slug2', '/Users/x/work/gamma', '/', \
         '子会话', NULL, 'build', 1785988035219, 1785994754248, 1786090826879)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO todo VALUES ('s1', '完成第一项', 'completed', 'high', 0, 0, 0), \
         ('s1', '进行第二项', 'in_progress', 'medium', 1, 0, 0), \
         ('s1', '待办第三项', 'pending', 'low', 2, 0, 0)",
        [],
    )
    .unwrap();
    drop(conn);
}

#[test]
fn opencode_collector_ignores_default_project_placeholder() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("opencode.db");
    create_session_db(&db_path);
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO session VALUES ('s3', 'p2', NULL, NULL, ?1, '/', '默认项目任务', NULL, 'build', \
         1785988035219, 1785994754248, NULL)",
        [format!("{}/Documents/Default Project", source_home())],
    )
    .unwrap();
    drop(conn);

    let result = OpenCodeCollector::new(db_path.clone(), temp.path().join("missing.db")).scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    let task = result
        .tasks
        .iter()
        .find(|task| task.source_id == "s3")
        .unwrap();
    assert_eq!(
        task.project_path, None,
        "placeholder directory is not a project"
    );
    assert_eq!(task.project_name, None);
}

fn create_session_v2_db(path: &Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE session (
          id TEXT PRIMARY KEY,
          project_id TEXT,
          parent_id TEXT,
          slug TEXT,
          directory TEXT,
          path TEXT,
          title TEXT,
          model TEXT,
          agent TEXT,
          time_created INTEGER,
          time_updated INTEGER,
          time_archived INTEGER
        );
        CREATE TABLE session_v2 (
          id TEXT PRIMARY KEY,
          project_id TEXT NOT NULL,
          parent_id TEXT,
          slug TEXT NOT NULL,
          directory TEXT NOT NULL,
          title TEXT,
          model TEXT,
          time_created INTEGER NOT NULL,
          time_updated INTEGER NOT NULL,
          time_archived INTEGER,
          time_idle INTEGER,
          time_busy INTEGER
        );
        CREATE TABLE todo (
          session_id TEXT,
          content TEXT,
          status TEXT,
          priority TEXT,
          position INTEGER,
          time_created INTEGER,
          time_updated INTEGER
        );
        "#,
    )
    .unwrap();
    // 旧 session 表在 v2 时代已冻结，不应再出现在结果里。
    conn.execute(
        "INSERT INTO session VALUES ('old_stale', 'p1', NULL, 'slug', '/Users/x/work/gamma', '/', \
         '旧表任务', NULL, 'build', 1785988035219, 1785994754248, NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session_v2 VALUES \
         ('v2_running', 'p1', NULL, 'slug', '/Users/x/work/gamma', '运行中任务', NULL, \
          1785988035219, 1785994800000, NULL, 1785994000000, 1785994800000), \
         ('v2_idle', 'p1', NULL, 'slug', '/Users/x/work/gamma', '空闲任务', NULL, \
          1785988035219, 1785994754248, NULL, 1785994800000, 1785994000000), \
         ('v2_archived', 'p1', NULL, 'slug', '/Users/x/work/gamma', '已归档任务', NULL, \
          1785988035219, 1785994754248, 1786090826879, 1785994000000, 1785994800000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO todo VALUES ('old_stale', '旧待办', 'pending', 'low', 0, 0, 0)",
        [],
    )
    .unwrap();
    drop(conn);
}

#[test]
fn opencode_collector_prefers_session_v2_and_infers_busy_status() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("opencode.db");
    create_session_v2_db(&db_path);

    let result = OpenCodeCollector::new(db_path.clone(), temp.path().join("missing.db")).scan();
    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    assert_eq!(
        result.tasks.len(),
        3,
        "旧 session 行必须被忽略：{:?}",
        result
            .tasks
            .iter()
            .map(|task| &task.source_id)
            .collect::<Vec<_>>()
    );
    assert!(result
        .tasks
        .iter()
        .all(|task| task.source_id.starts_with("v2_")));
    assert!(result.todos.is_empty(), "旧表 todo 不应挂到 v2 任务上");

    let running = result
        .tasks
        .iter()
        .find(|task| task.source_id == "v2_running")
        .unwrap();
    assert_eq!(running.status_source.as_deref(), Some("running"));
    assert_eq!(running.status_raw.as_deref(), Some("busy"));

    let idle = result
        .tasks
        .iter()
        .find(|task| task.source_id == "v2_idle")
        .unwrap();
    assert_eq!(idle.status_source.as_deref(), Some("idle"));

    let archived = result
        .tasks
        .iter()
        .find(|task| task.source_id == "v2_archived")
        .unwrap();
    assert_eq!(archived.status_source.as_deref(), Some("archived"));
    assert!(archived.source_archived);
}

#[test]
fn opencode_and_kilo_collectors_share_session_schema() {
    let temp = TempDir::new().unwrap();
    let opencode_path = temp.path().join("opencode.db");
    create_session_db(&opencode_path);
    let before = fs::read(&opencode_path).unwrap();

    let opencode =
        OpenCodeCollector::new(opencode_path.clone(), temp.path().join("missing.db")).scan();
    assert_eq!(
        opencode.status,
        SourceScanStatus::Ok,
        "{:?}",
        opencode.error
    );
    assert_eq!(opencode.tasks.len(), 2);
    assert_eq!(opencode.todos.len(), 3);
    let top = opencode
        .tasks
        .iter()
        .find(|task| task.source_id == "s1")
        .unwrap();
    assert_eq!(top.title, "Gamma 任务");
    assert_eq!(top.model.as_deref(), Some("kimi-k2"));
    assert_eq!(top.todo_total, Some(3));
    assert_eq!(top.todo_done, Some(1));
    assert_eq!(
        top.resume_cmd.as_deref(),
        Some("cd '/Users/x/work/gamma' && opencode --session s1")
    );
    let child = opencode
        .tasks
        .iter()
        .find(|task| task.source_id == "s2")
        .unwrap();
    assert_eq!(child.parent_key.as_deref(), Some("opencode:s1"));
    assert_eq!(child.status_source.as_deref(), Some("archived"));

    let after = fs::read(&opencode_path).unwrap();
    assert_eq!(before, after);

    let kilo_path = temp.path().join("kilo.db");
    create_session_db(&kilo_path);
    let kilo = KiloCollector::new(kilo_path).scan();
    assert_eq!(kilo.status, SourceScanStatus::Ok);
    let kilo_top = kilo
        .tasks
        .iter()
        .find(|task| task.source_id == "s1")
        .unwrap();
    assert_eq!(
        kilo_top.resume_cmd.as_deref(),
        Some("cd '/Users/x/work/gamma' && kilo")
    );
}

#[test]
fn trae_cn_collector_aggregates_hook_events() {
    let temp = TempDir::new().unwrap();
    let events = temp.path().join("trae-cn");
    fs::create_dir_all(&events).unwrap();
    let first = events.join("events-2026-11-06.jsonl");
    fs::write(
        &first,
        concat!(
            "{\"v\":1,\"uid\":\"u1\",\"ts\":1700000000.0,\"event\":\"Stop\",\"session\":\"sid_a\",\"project\":\"alpha\",\"tool\":\"Edit\"}\n",
            "{\"v\":1,\"uid\":\"u2\",\"ts\":1700000100.0,\"event\":\"Stop\",\"session\":\"sid_a\",\"project\":\"alpha\",\"model\":\"glm-5\"}\n",
            "{\"v\":1,\"uid\":\"u3\",\"ts\":1700000200.0,\"event\":\"Stop\",\"session\":\"sid_b\",\"project\":\"beta\"}\n",
            "{\"v\":1,\"uid\":\"u4\",\"ts\":1700000300.0,\"event\":\"Stop\",\"session\":\"unknown\",\"project\":\"beta\"}\n",
        ),
    )
    .unwrap();
    let second = events.join("events-2026-11-07.jsonl");
    fs::write(
        &second,
        "{\"v\":1,\"uid\":\"u5\",\"ts\":1700000400.0,\"event\":\"Stop\",\"session\":\"sid_a\",\"project\":\"gamma\",\"model\":\"doubao-seed\"}\n",
    )
    .unwrap();
    let before = fs::read_to_string(&first).unwrap();

    let result = TraeCnCollector::new(events.clone()).scan();

    assert_eq!(result.status, SourceScanStatus::Ok);
    assert_eq!(result.source, SOURCE_TRAE_CN);
    assert_eq!(result.files_seen, 2);
    assert_eq!(result.probe_kind, "file_tree");
    assert!(result.error.is_none());
    assert_eq!(result.tasks.len(), 2, "unknown session is skipped");

    let alpha = result
        .tasks
        .iter()
        .find(|task| task.source_id == "sid_a")
        .unwrap();
    assert_eq!(alpha.title, "新会话");
    assert_eq!(alpha.title_source, "fallback");
    assert_eq!(alpha.project_name.as_deref(), Some("gamma"));
    assert_eq!(alpha.created_at, Some(1_700_000_000.0));
    assert_eq!(alpha.updated_at, Some(1_700_000_400.0));
    assert_eq!(alpha.model.as_deref(), Some("doubao-seed"));
    assert_eq!(alpha.status_source.as_deref(), Some("idle"));
    assert_eq!(alpha.agent_kind.as_deref(), Some("desktop"));
    assert!(alpha.project_path.is_none());

    let beta = result
        .tasks
        .iter()
        .find(|task| task.source_id == "sid_b")
        .unwrap();
    assert_eq!(beta.project_name.as_deref(), Some("beta"));
    assert_eq!(beta.created_at, Some(1_700_000_200.0));
    assert_eq!(beta.updated_at, Some(1_700_000_200.0));
    assert!(beta.model.is_none());

    assert_eq!(result.tasks[0].source_id, "sid_a", "newest session first");
    assert_eq!(fs::read_to_string(&first).unwrap(), before);
}

#[test]
fn trae_cn_collector_derives_status_from_latest_event() {
    let temp = TempDir::new().unwrap();
    let events = temp.path().join("trae-cn");
    fs::create_dir_all(&events).unwrap();
    fs::write(
        events.join("events-2026-11-07.jsonl"),
        concat!(
            "{\"v\":1,\"ts\":1700000100.0,\"event\":\"SessionStart\",\"session\":\"sid_run\"}\n",
            "{\"v\":1,\"ts\":1700000200.0,\"event\":\"UserPromptSubmit\",\"session\":\"sid_run\"}\n",
            "{\"v\":1,\"ts\":1700000300.0,\"event\":\"PreToolUse\",\"session\":\"sid_run\"}\n",
            "{\"v\":1,\"ts\":1700000400.0,\"event\":\"UserPromptSubmit\",\"session\":\"sid_stop\"}\n",
            "{\"v\":1,\"ts\":1700000500.0,\"event\":\"Stop\",\"session\":\"sid_stop\"}\n",
            "{\"v\":1,\"ts\":1700000600.0,\"event\":\"SessionStart\",\"session\":\"sid_start\"}\n",
            "{\"v\":1,\"ts\":1700000800.0,\"event\":\"UnknownEvent\",\"session\":\"sid_start\"}\n",
        ),
    )
    .unwrap();

    let result = TraeCnCollector::new(events).scan();

    assert_eq!(result.status, SourceScanStatus::Ok);
    let status = |id: &str| {
        result
            .tasks
            .iter()
            .find(|task| task.source_id == id)
            .and_then(|task| task.status_source.clone())
            .unwrap()
    };
    assert_eq!(status("sid_run"), "running", "tool activity keeps running");
    assert_eq!(status("sid_stop"), "idle", "Stop closes the turn");
    assert_eq!(
        status("sid_start"),
        "idle",
        "unknown events do not change status"
    );
}

#[test]
fn trae_cn_collector_reports_missing_hook_directory() {
    let temp = TempDir::new().unwrap();
    let result = TraeCnCollector::new(temp.path().join("missing")).scan();

    assert_eq!(result.status, SourceScanStatus::Unavailable);
    assert_eq!(result.probe_kind, "file_tree");
    assert!(result.tasks.is_empty());
    assert_eq!(result.files_seen, 0);
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("usage hooks install trae-cn")));
}

#[test]
fn trae_cn_collector_reports_partial_on_bad_lines() {
    let temp = TempDir::new().unwrap();
    let events = temp.path().join("trae-cn");
    fs::create_dir_all(&events).unwrap();
    fs::write(
        events.join("events-2026-11-07.jsonl"),
        concat!(
            "{\"ts\":1700000000.0,\"session\":\"sid_a\",\"project\":\"alpha\"}\n",
            "{\"ts\":1700000100.0,\"session\":\n",
        ),
    )
    .unwrap();

    let result = TraeCnCollector::new(events).scan();

    assert_eq!(result.status, SourceScanStatus::Partial);
    assert_eq!(result.tasks.len(), 1);
    assert!(result.error.is_some());
}
