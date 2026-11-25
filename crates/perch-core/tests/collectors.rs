use std::fs;
use std::path::Path;

use perch_core::collectors::antigravity::AntigravityCollector;
use perch_core::collectors::claude::ClaudeCollector;
use perch_core::collectors::cline::ClineCollector;
use perch_core::collectors::codearts::CodeArtsCollector;
use perch_core::collectors::codebuddy::CodeBuddyCollector;
use perch_core::collectors::codex::CodexCollector;
use perch_core::collectors::kilocode::KiloCollector;
use perch_core::collectors::minimax::MiniMaxCollector;
use perch_core::collectors::opencode::OpenCodeCollector;
use perch_core::collectors::qoder::QoderCollector;
use perch_core::collectors::trae_cn::TraeCnCollector;
use perch_core::collectors::Collector;
use perch_core::models::{
    SourceScanStatus, SOURCE_ANTIGRAVITY, SOURCE_CLINE, SOURCE_CODEARTS, SOURCE_CODEBUDDY,
    SOURCE_MINIMAX, SOURCE_TRAE_CN,
};
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
fn claude_collector_derives_title_from_first_candidate_user_record() {
    let temp = TempDir::new().unwrap();
    let projects = temp.path().join("projects/-tmp-alpha");
    fs::create_dir_all(&projects).unwrap();
    let source = projects.join("derived.jsonl");
    let original = concat!(
        "{\"type\":\"user\",\"isSidechain\":true,\"message\":{\"role\":\"user\",\
         \"content\":[{\"type\":\"text\",\"text\":\"侧链输入\"}]}}\n",
        "{\"type\":\"user\",\"isMeta\":true,\"message\":{\"role\":\"user\",\
         \"content\":[{\"type\":\"text\",\"text\":\"元信息\"}]}}\n",
        "{\"type\":\"user\",\"toolUseResult\":{\"stdout\":\"工具输出\"},\
         \"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"工具结果\"}]}}\n",
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\
         \"text\":\"<command-name>/review</command-name>\"},{\"type\":\"text\",\
         \"text\":\"  请   审查  这个   功能   并补充测试  \"}]}}\n",
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\
         \"text\":\"后续输入不应参与标题\"}]}}\n"
    );
    fs::write(&source, original).unwrap();

    let result = ClaudeCollector::new(temp.path().to_path_buf()).scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    let task = result
        .tasks
        .iter()
        .find(|task| task.source_id == "derived")
        .unwrap();
    assert_eq!(task.title, "请 审查 这个 功能 并补充测试");
    assert_eq!(task.title_source, "derived");
    assert_eq!(fs::read(&source).unwrap(), original.as_bytes());
}

#[test]
fn claude_collector_ignores_slash_only_first_candidate() {
    let temp = TempDir::new().unwrap();
    let projects = temp.path().join("projects/-tmp-alpha");
    fs::create_dir_all(&projects).unwrap();
    let source = projects.join("slash-only.jsonl");
    fs::write(
        &source,
        concat!(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\
             \"text\":\"<command-name>/model</command-name>\"},{\"type\":\"text\",\
             \"text\":\"<command-args></command-args>\"}]}}\n",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\
             \"text\":\"后续有效输入\"}]}}\n"
        ),
    )
    .unwrap();

    let result = ClaudeCollector::new(temp.path().to_path_buf()).scan();

    let task = result
        .tasks
        .iter()
        .find(|task| task.source_id == "slash-only")
        .unwrap();
    assert_eq!(task.title, "新会话");
    assert_eq!(task.title_source, "fallback");
}

#[test]
fn claude_collector_uses_fallback_when_first_candidate_is_empty() {
    let temp = TempDir::new().unwrap();
    let projects = temp.path().join("projects/-tmp-alpha");
    fs::create_dir_all(&projects).unwrap();
    fs::write(
        projects.join("empty-first.jsonl"),
        concat!(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\
             \"text\":\" <system-reminder>注入内容</system-reminder> \"}]}}\n",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\
             \"text\":\"后续有效输入\"}]}}\n"
        ),
    )
    .unwrap();

    let result = ClaudeCollector::new(temp.path().to_path_buf()).scan();

    let task = result
        .tasks
        .iter()
        .find(|task| task.source_id == "empty-first")
        .unwrap();
    assert_eq!(task.title, "新会话");
    assert_eq!(task.title_source, "fallback");
}

#[test]
fn claude_collector_truncates_derived_title_to_sixty_chars() {
    let temp = TempDir::new().unwrap();
    let projects = temp.path().join("projects/-tmp-alpha");
    fs::create_dir_all(&projects).unwrap();
    let text = format!("{}中后文", "a".repeat(59));
    let line = serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": [{"type": "text", "text": text}]}
    });
    fs::write(
        projects.join("long.jsonl"),
        format!("{}\n", serde_json::to_string(&line).unwrap()),
    )
    .unwrap();

    let result = ClaudeCollector::new(temp.path().to_path_buf()).scan();

    let task = result
        .tasks
        .iter()
        .find(|task| task.source_id == "long")
        .unwrap();
    assert_eq!(task.title, format!("{}中", "a".repeat(59)));
    assert_eq!(task.title.chars().count(), 60);
    assert_eq!(task.title_source, "derived");
}

#[test]
fn claude_collector_ignores_empty_native_titles() {
    let temp = TempDir::new().unwrap();
    let projects = temp.path().join("projects/-tmp-alpha");
    fs::create_dir_all(&projects).unwrap();
    let session_dir = projects.join("empty-native");
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        projects.join("empty-native.jsonl"),
        concat!(
            "{\"type\":\"custom-title\",\"customTitle\":\"   \"}\n",
            "{\"type\":\"custom-title\",\"customTitle\":\"有效人工标题\"}\n",
            "{\"type\":\"custom-title\",\"customTitle\":\"\"}\n",
            "{\"type\":\"ai-title\",\"aiTitle\":\"有效 AI 标题\"}\n",
            "{\"type\":\"ai-title\",\"aiTitle\":\"\"}\n"
        ),
    )
    .unwrap();
    fs::write(
        session_dir.join("custom-title.json"),
        "{\"customTitle\":\"Sidecar 标题\"}",
    )
    .unwrap();

    let result = ClaudeCollector::new(temp.path().to_path_buf()).scan();

    let task = result
        .tasks
        .iter()
        .find(|task| task.source_id == "empty-native")
        .unwrap();
    assert_eq!(task.title, "有效人工标题");
    assert_eq!(task.title_source, "custom");
}

#[test]
fn claude_collector_uses_sidecar_title_between_native_titles() {
    let temp = TempDir::new().unwrap();
    let projects = temp.path().join("projects/-tmp-alpha");
    fs::create_dir_all(&projects).unwrap();
    let session_dir = projects.join("sidecar");
    fs::create_dir_all(&session_dir).unwrap();
    let source = projects.join("sidecar.jsonl");
    fs::write(
        &source,
        concat!(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\
             \"text\":\"派生标题\"}]}}\n",
            "{\"type\":\"ai-title\",\"aiTitle\":\"AI 标题\"}\n",
            "{\"type\":\"custom-title\",\"customTitle\":\"JSONL 人工标题\"}\n"
        ),
    )
    .unwrap();
    let sidecar = session_dir.join("custom-title.json");
    fs::write(&sidecar, "{\"customTitle\":\"Sidecar 标题\"}").unwrap();
    let collector = ClaudeCollector::new(temp.path().to_path_buf());
    let with_native = collector.scan();
    let task = with_native
        .tasks
        .iter()
        .find(|task| task.source_id == "sidecar")
        .unwrap();
    assert_eq!(task.title, "JSONL 人工标题");
    assert_eq!(task.title_source, "custom");

    fs::write(&source, "{\"type\":\"ai-title\",\"aiTitle\":\"AI 标题\"}\n").unwrap();
    let with_sidecar = collector.scan();
    let task = with_sidecar
        .tasks
        .iter()
        .find(|task| task.source_id == "sidecar")
        .unwrap();
    assert_eq!(task.title, "Sidecar 标题");
    assert_eq!(task.title_source, "custom");

    fs::write(&sidecar, "{\"customTitle\":\"   \"}").unwrap();
    let with_blank = collector.scan();
    let task = with_blank
        .tasks
        .iter()
        .find(|task| task.source_id == "sidecar")
        .unwrap();
    assert_eq!(task.title, "AI 标题");
    assert_eq!(task.title_source, "ai");

    fs::write(&sidecar, "{broken").unwrap();
    let with_broken = collector.scan();
    let task = with_broken
        .tasks
        .iter()
        .find(|task| task.source_id == "sidecar")
        .unwrap();
    assert_eq!(task.title, "AI 标题");
    assert_eq!(task.title_source, "ai");

    fs::write(&sidecar, "{\"customTitle\":\"临时标题\"}").unwrap();
    let final_sidecar_before = fs::read(&sidecar).unwrap();
    fs::remove_file(&sidecar).unwrap();
    let after_delete = collector.scan();
    let task = after_delete
        .tasks
        .iter()
        .find(|task| task.source_id == "sidecar")
        .unwrap();
    assert_eq!(task.title, "AI 标题");
    assert_eq!(task.title_source, "ai");
    assert_eq!(
        final_sidecar_before,
        "{\"customTitle\":\"临时标题\"}".as_bytes()
    );
}

#[test]
fn claude_collector_native_title_takes_over_derived_and_reverts_when_removed() {
    let temp = TempDir::new().unwrap();
    let projects = temp.path().join("projects/-tmp-alpha");
    fs::create_dir_all(&projects).unwrap();
    let source = projects.join("takeover.jsonl");
    fs::write(
        &source,
        concat!(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\
             \"text\":\"派生标题\"}]}}\n",
            "{\"type\":\"ai-title\",\"aiTitle\":\"AI 标题\"}\n"
        ),
    )
    .unwrap();

    let collector = ClaudeCollector::new(temp.path().to_path_buf());
    let with_ai = collector.scan();
    let task = with_ai
        .tasks
        .iter()
        .find(|task| task.source_id == "takeover")
        .unwrap();
    assert_eq!(task.title, "AI 标题");
    assert_eq!(task.title_source, "ai");

    fs::write(
        &source,
        concat!(
            "{\"type\":\"ai-title\",\"aiTitle\":\"AI 标题\"}\n",
            "{\"type\":\"custom-title\",\"customTitle\":\"人工标题\"}\n"
        ),
    )
    .unwrap();
    let with_custom = collector.scan();
    let task = with_custom
        .tasks
        .iter()
        .find(|task| task.source_id == "takeover")
        .unwrap();
    assert_eq!(task.title, "人工标题");
    assert_eq!(task.title_source, "custom");

    fs::write(
        &source,
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"派生标题\"}]}}\n",
    )
    .unwrap();
    let reverted = collector.scan();
    let task = reverted
        .tasks
        .iter()
        .find(|task| task.source_id == "takeover")
        .unwrap();
    assert_eq!(task.title, "派生标题");
    assert_eq!(task.title_source, "derived");
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

/// 写入一条合成 Codex rollout 首行（session_meta），payload 为紧凑 JSON 字符串。
fn write_codex_rollout(root: &Path, relative: &str, payload: &str) {
    let file = root.join(relative);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(
        &file,
        format!(
            "{{\"timestamp\":\"2026-11-01T08:00:00Z\",\"type\":\"session_meta\",\
             \"payload\":{payload}}}\n"
        ),
    )
    .unwrap();
}

#[test]
fn codex_subagent_sessions_link_to_parent_thread() {
    let temp = TempDir::new().unwrap();
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-main.jsonl",
        "{\"id\":\"main-a\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"Codex Desktop\",\
         \"thread_source\":\"user\"}",
    );
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-sub.jsonl",
        "{\"id\":\"sub-a\",\"session_id\":\"main-a\",\"cwd\":\"/Users/x/work/beta\",\
         \"originator\":\"codex-tui\",\"thread_source\":\"subagent\",\
         \"parent_thread_id\":\"main-a\",\"source\":{\"subagent\":{}}}",
    );

    let result = CodexCollector::new(temp.path().to_path_buf()).scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    assert_eq!(result.tasks.len(), 2, "子会话仍保留在快照中");
    let child = result
        .tasks
        .iter()
        .find(|task| task.source_id == "sub-a")
        .unwrap();
    assert_eq!(child.parent_key.as_deref(), Some("codex:main-a"));
    let parent = result
        .tasks
        .iter()
        .find(|task| task.source_id == "main-a")
        .unwrap();
    assert_eq!(parent.parent_key, None, "主任务不得带父子关系");
}

#[test]
fn codex_side_chat_sessions_link_to_parent_thread() {
    let temp = TempDir::new().unwrap();
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-main.jsonl",
        "{\"id\":\"main-b\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"Codex Desktop\",\
         \"thread_source\":\"user\"}",
    );
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-side-forked.jsonl",
        "{\"id\":\"side-b\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"codex-tui\",\
         \"thread_source\":\"tui_side_chat\",\"forked_from_id\":\"main-b\"}",
    );
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-side-history.jsonl",
        "{\"id\":\"side-b2\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"codex-tui\",\
         \"thread_source\":\"tui_side_chat\",\"history_base\":{\"thread_id\":\"main-b\"}}",
    );

    let result = CodexCollector::new(temp.path().to_path_buf()).scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    assert_eq!(result.tasks.len(), 3);
    let forked = result
        .tasks
        .iter()
        .find(|task| task.source_id == "side-b")
        .unwrap();
    assert_eq!(forked.parent_key.as_deref(), Some("codex:main-b"));
    let history = result
        .tasks
        .iter()
        .find(|task| task.source_id == "side-b2")
        .unwrap();
    assert_eq!(history.parent_key.as_deref(), Some("codex:main-b"));
}

#[test]
fn codex_subagent_falls_back_to_forked_from_id() {
    let temp = TempDir::new().unwrap();
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-main.jsonl",
        "{\"id\":\"main-c\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"Codex Desktop\",\
         \"thread_source\":\"user\"}",
    );
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-sub-forked.jsonl",
        "{\"id\":\"sub-c\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"codex-tui\",\
         \"thread_source\":\"subagent\",\"forked_from_id\":\"main-c\"}",
    );
    // thread_source 缺失但 source 显式声明 subagent 的 rollout，仍按子会话兜底。
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-sub-source.jsonl",
        "{\"id\":\"sub-c2\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"codex-tui\",\
         \"forked_from_id\":\"main-c\",\"source\":{\"subagent\":{}}}",
    );

    let result = CodexCollector::new(temp.path().to_path_buf()).scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    assert_eq!(result.tasks.len(), 3);
    let forked = result
        .tasks
        .iter()
        .find(|task| task.source_id == "sub-c")
        .unwrap();
    assert_eq!(forked.parent_key.as_deref(), Some("codex:main-c"));
    let source = result
        .tasks
        .iter()
        .find(|task| task.source_id == "sub-c2")
        .unwrap();
    assert_eq!(source.parent_key.as_deref(), Some("codex:main-c"));
}

#[test]
fn codex_user_automation_and_unsourced_forks_stay_top_level() {
    let temp = TempDir::new().unwrap();
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-user-forked.jsonl",
        "{\"id\":\"user-d\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"Codex Desktop\",\
         \"thread_source\":\"user\",\"forked_from_id\":\"main-d\"}",
    );
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-automation-forked.jsonl",
        "{\"id\":\"auto-d\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"codex_exec\",\
         \"thread_source\":\"automation\",\"forked_from_id\":\"main-d\"}",
    );
    // 真实数据里存在 thread_source 缺失却带 forked_from_id 的普通会话，不得误判为子会话。
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-unsourced-forked.jsonl",
        "{\"id\":\"plain-d\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"codex-tui\",\
         \"forked_from_id\":\"main-d\"}",
    );

    let result = CodexCollector::new(temp.path().to_path_buf()).scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    assert_eq!(result.tasks.len(), 3);
    for source_id in ["user-d", "auto-d", "plain-d"] {
        let task = result
            .tasks
            .iter()
            .find(|task| task.source_id == source_id)
            .unwrap();
        assert_eq!(task.parent_key, None, "{source_id} 必须仍是主任务");
    }
}

#[test]
fn codex_self_or_blank_parent_ids_do_not_link() {
    let temp = TempDir::new().unwrap();
    // parent_thread_id 等于自身 id（以 payload.id 判定自身，session_id 指向父不算）。
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-self.jsonl",
        "{\"id\":\"sub-e\",\"session_id\":\"sub-e\",\"cwd\":\"/Users/x/work/beta\",\
         \"originator\":\"codex-tui\",\"thread_source\":\"subagent\",\
         \"parent_thread_id\":\"sub-e\"}",
    );
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-blank.jsonl",
        "{\"id\":\"sub-e2\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"codex-tui\",\
         \"thread_source\":\"subagent\",\"parent_thread_id\":\"   \"}",
    );

    let result = CodexCollector::new(temp.path().to_path_buf()).scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    assert_eq!(result.tasks.len(), 2);
    for source_id in ["sub-e", "sub-e2"] {
        let task = result
            .tasks
            .iter()
            .find(|task| task.source_id == source_id)
            .unwrap();
        assert_eq!(task.parent_key, None, "{source_id} 不应建立父子关系");
    }
}

#[test]
fn codex_children_survive_missing_parents_depth_and_archives() {
    let temp = TempDir::new().unwrap();
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-main.jsonl",
        "{\"id\":\"main-f\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"Codex Desktop\",\
         \"thread_source\":\"user\"}",
    );
    // 二级子代理以即时父为准，而不是根主会话。
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-sub.jsonl",
        "{\"id\":\"sub-f\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"codex-tui\",\
         \"thread_source\":\"subagent\",\"parent_thread_id\":\"main-f\"}",
    );
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-grandchild.jsonl",
        "{\"id\":\"sub-f2\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"codex-tui\",\
         \"thread_source\":\"subagent\",\"parent_thread_id\":\"sub-f\"}",
    );
    // 父会话不在本次扫描结果中，父子关系照常写入（与 Qoder/Cline/OpenCode 一致）。
    write_codex_rollout(
        temp.path(),
        "sessions/2026/11/rollout-orphan.jsonl",
        "{\"id\":\"sub-ghost\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"codex-tui\",\
         \"thread_source\":\"subagent\",\"parent_thread_id\":\"ghost-parent\"}",
    );
    // 归档目录下的子代理会话同样归类。
    write_codex_rollout(
        temp.path(),
        "archived_sessions/rollout-sub-archived.jsonl",
        "{\"id\":\"sub-arch\",\"cwd\":\"/Users/x/work/beta\",\"originator\":\"codex-tui\",\
         \"thread_source\":\"subagent\",\"parent_thread_id\":\"main-f\"}",
    );

    let result = CodexCollector::new(temp.path().to_path_buf()).scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    assert_eq!(result.tasks.len(), 5);
    let grandchild = result
        .tasks
        .iter()
        .find(|task| task.source_id == "sub-f2")
        .unwrap();
    assert_eq!(grandchild.parent_key.as_deref(), Some("codex:sub-f"));
    let orphan = result
        .tasks
        .iter()
        .find(|task| task.source_id == "sub-ghost")
        .unwrap();
    assert_eq!(orphan.parent_key.as_deref(), Some("codex:ghost-parent"));
    let archived = result
        .tasks
        .iter()
        .find(|task| task.source_id == "sub-arch")
        .unwrap();
    assert_eq!(archived.parent_key.as_deref(), Some("codex:main-f"));
    assert!(archived.source_archived);
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

    let result = TraeCnCollector::new(events.clone(), temp.path().join("memory")).scan();

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

    let result = TraeCnCollector::new(events, temp.path().join("memory")).scan();
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
fn trae_cn_collector_includes_memory_history_sessions() {
    let temp = TempDir::new().unwrap();
    let events = temp.path().join("trae-cn");
    fs::create_dir_all(&events).unwrap();
    fs::write(
        events.join("events-2026-11-07.jsonl"),
        "{\"v\":1,\"ts\":1700000000.0,\"event\":\"Stop\",\"session\":\"sid_hook\"}\n",
    )
    .unwrap();

    let memory = temp.path().join("memory/projects");
    let playground =
        memory.join("-Users-wayne-workspace-playground--p2-da242afa7f737d3199f2/20261104");
    fs::create_dir_all(&playground).unwrap();
    fs::write(
        playground.join("session_memory_sid_history.jsonl"),
        "{\"intent\":\"历史会话同步实现\",\"message_summary_time\":\"2026-11-04 10:00:00\"}\n",
    )
    .unwrap();
    let hook_day =
        memory.join("-Users-wayne-workspace-playground--p2-da242afa7f737d3199f2/20261107");
    fs::create_dir_all(&hook_day).unwrap();
    fs::write(
        hook_day.join("session_memory_sid_hook.jsonl"),
        "{\"intent\":\"Hook 会话标题\",\"message_summary_time\":\"2026-11-07 20:09:00\"}\n",
    )
    .unwrap();
    let temp_project = memory
        .join("-Users-wayne-Documents-Trae-CN-temp-20261102-04--p2-3a81734beb95e3c3c9d7/20261008");
    fs::create_dir_all(&temp_project).unwrap();
    fs::write(
        temp_project.join("session_memory_sid_temp.jsonl"),
        "{\"intent\":\"临时目录实验\",\"message_summary_time\":\"2026-10-08 09:00:00\"}\n",
    )
    .unwrap();

    let result = TraeCnCollector::new(events, memory).scan();

    assert_eq!(result.status, SourceScanStatus::Ok);
    assert_eq!(
        result.tasks.len(),
        3,
        "memory sessions are added exactly once"
    );
    assert_eq!(
        result.files_seen, 4,
        "one event file plus three memory files"
    );

    let history = result
        .tasks
        .iter()
        .find(|task| task.source_id == "sid_history")
        .unwrap();
    assert_eq!(history.title, "历史会话同步实现");
    assert_eq!(history.title_source, "ai");
    assert_eq!(history.project_name.as_deref(), Some("playground"));
    assert_eq!(history.status_source.as_deref(), Some("idle"));
    assert_eq!(history.agent_kind.as_deref(), Some("desktop"));
    assert!(history.created_at.is_some());
    assert!(history.updated_at.is_some());
    assert!(history.created_at.unwrap() < history.updated_at.unwrap());

    let temp_session = result
        .tasks
        .iter()
        .find(|task| task.source_id == "sid_temp")
        .unwrap();
    assert_eq!(
        temp_session.project_name.as_deref(),
        Some("temp-20261102-04")
    );
    assert_eq!(temp_session.title, "临时目录实验");

    let hook = result
        .tasks
        .iter()
        .find(|task| task.source_id == "sid_hook")
        .unwrap();
    assert_eq!(
        hook.project_name.as_deref(),
        Some("playground"),
        "hook sessions reuse the memory project name"
    );
    assert_eq!(
        hook.title, "Hook 会话标题",
        "hook sessions reuse the memory intent title"
    );
}

#[test]
fn trae_cn_collector_reports_missing_hook_directory() {
    let temp = TempDir::new().unwrap();
    let result = TraeCnCollector::new(
        temp.path().join("missing"),
        temp.path().join("missing-memory"),
    )
    .scan();

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

    let result = TraeCnCollector::new(events, temp.path().join("memory")).scan();

    assert_eq!(result.status, SourceScanStatus::Partial);
    assert_eq!(result.tasks.len(), 1);
    assert!(result.error.is_some());
}

fn create_cline_db(path: &Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE sessions (
          session_id TEXT PRIMARY KEY,
          source TEXT NOT NULL,
          status TEXT NOT NULL,
          started_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          cwd TEXT NOT NULL,
          workspace_root TEXT NOT NULL,
          parent_session_id TEXT,
          model TEXT,
          metadata_json TEXT
        );
        "#,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sessions VALUES ('desk1', 'desktop', 'completed', \
         '2026-11-01T02:00:00.000Z', '2026-11-01T03:00:00.000Z', \
         '/Users/x/work/alpha', '/Users/x/work/alpha', NULL, 'cline-free/mimo-v2.6-flash', \
         '{\"title\":\"Alpha 任务\",\"systemPrompt\":\"must not be read\"}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sessions VALUES ('cli1', 'cli', 'failed', \
         '2026-11-02T02:00:00.000Z', '2026-11-02T03:00:00.000Z', \
         '/Users/x/work/beta', '/Users/x/work/beta', NULL, 'deepseek/deepseek-v4-flash', \
         '{\"title\":\"Beta 任务\"}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sessions VALUES ('chat1', 'desktop', 'idle', \
         '2026-11-03T02:00:00.000Z', '2026-11-03T03:00:00.000Z', \
         '/Users/wayne/.cline/data/workspaces/chat', '/Users/wayne/.cline/data/workspaces/chat', \
         NULL, 'cline-free/step-5-preview', '{\"title\":\"聊天任务\"}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sessions VALUES ('sub1', 'cli', 'running', \
         '2026-11-03T04:00:00.000Z', '2026-11-03T04:30:00.000Z', \
         '/Users/x/work/beta', '/Users/x/work/beta', 'cli1', NULL, '{\"title\":\"子任务\"}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sessions VALUES ('broken1', 'cli', 'idle', \
         '2026-11-04T04:00:00.000Z', '2026-11-04T04:30:00.000Z', \
         '/Users/x/work/beta', '/Users/x/work/beta', NULL, NULL, 'not-json')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sessions VALUES ('cwd1', 'cli', 'cancelled', \
         '2026-11-05T04:00:00.000Z', '2026-11-05T04:30:00.000Z', \
         '/Users/x/work/gamma', '', NULL, NULL, '{}')",
        [],
    )
    .unwrap();
}

#[test]
fn cline_collector_maps_sessions_and_keeps_source_read_only() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("sessions.db");
    create_cline_db(&db_path);
    let before = fs::read(&db_path).unwrap();

    let result = ClineCollector::new(db_path.clone()).scan();

    assert_eq!(result.status, SourceScanStatus::Ok);
    assert_eq!(result.source, SOURCE_CLINE);
    assert_eq!(result.tasks.len(), 6);
    assert_eq!(result.files_seen, 1);
    assert_eq!(result.probe_kind, "sqlite");

    let desktop = result
        .tasks
        .iter()
        .find(|task| task.source_id == "desk1")
        .unwrap();
    assert_eq!(desktop.title, "Alpha 任务");
    assert_eq!(desktop.title_source, "source");
    assert_eq!(desktop.project_path.as_deref(), Some("/Users/x/work/alpha"));
    assert_eq!(desktop.project_name.as_deref(), Some("alpha"));
    assert_eq!(desktop.status_source.as_deref(), Some("complete"));
    assert_eq!(desktop.status_raw.as_deref(), Some("completed"));
    assert_eq!(desktop.agent_kind.as_deref(), Some("desktop"));
    assert_eq!(desktop.model.as_deref(), Some("cline-free/mimo-v2.6-flash"));
    assert_eq!(desktop.created_at, Some(1793498400.0));
    assert_eq!(desktop.updated_at, Some(1793502000.0));
    assert_eq!(
        desktop.resume_cmd.as_deref(),
        Some("cd '/Users/x/work/alpha' && cline --id desk1")
    );

    let cli = result
        .tasks
        .iter()
        .find(|task| task.source_id == "cli1")
        .unwrap();
    assert_eq!(cli.status_source.as_deref(), Some("failed"));
    assert_eq!(cli.agent_kind.as_deref(), Some("cli"));

    let chat = result
        .tasks
        .iter()
        .find(|task| task.source_id == "chat1")
        .unwrap();
    assert_eq!(chat.project_path, None, "Cline chat workspace is internal");
    assert_eq!(chat.project_name, None);
    assert_eq!(chat.status_source.as_deref(), Some("idle"));
    assert_eq!(
        chat.resume_cmd.as_deref(),
        Some("cline --id chat1"),
        "internal chat workspaces must not appear as a cd target"
    );

    let sub = result
        .tasks
        .iter()
        .find(|task| task.source_id == "sub1")
        .unwrap();
    assert_eq!(sub.parent_key.as_deref(), Some("cline:cli1"));
    assert_eq!(sub.status_source.as_deref(), Some("running"));

    let broken = result
        .tasks
        .iter()
        .find(|task| task.source_id == "broken1")
        .unwrap();
    assert_eq!(broken.title, "新会话");
    assert_eq!(broken.title_source, "fallback");

    let cwd = result
        .tasks
        .iter()
        .find(|task| task.source_id == "cwd1")
        .unwrap();
    assert_eq!(cwd.project_path.as_deref(), Some("/Users/x/work/gamma"));
    assert_eq!(cwd.status_source.as_deref(), Some("idle"));
    assert_eq!(cwd.status_raw.as_deref(), Some("cancelled"));

    let after = fs::read(&db_path).unwrap();
    assert_eq!(
        before, after,
        "collector must not write the source database"
    );
}

#[test]
fn cline_collector_reports_missing_database() {
    let temp = TempDir::new().unwrap();
    let result = ClineCollector::new(temp.path().join("missing.db")).scan();

    assert_eq!(result.status, SourceScanStatus::Unavailable);
    assert!(result.tasks.is_empty());
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("未找到 Cline 数据库")));
}
#[test]
fn codearts_collector_shares_session_schema_and_keeps_source_read_only() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("opencode.db");
    create_session_db(&db_path);
    let before = fs::read(&db_path).unwrap();

    let result = CodeArtsCollector::new(db_path.clone()).scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    assert_eq!(result.source, SOURCE_CODEARTS);
    assert_eq!(result.tasks.len(), 2);
    let top = result
        .tasks
        .iter()
        .find(|task| task.source_id == "s1")
        .unwrap();
    assert_eq!(top.title, "Gamma 任务");
    assert_eq!(top.project_name.as_deref(), Some("gamma"));
    assert_eq!(top.todo_total, Some(3));
    assert_eq!(top.todo_done, Some(1));
    assert_eq!(
        top.resume_cmd, None,
        "CodeArts resume goes through app activation, not a CLI command"
    );

    let after = fs::read(&db_path).unwrap();
    assert_eq!(
        before, after,
        "collector must not write the source database"
    );
}

#[test]
fn codearts_collector_reports_missing_database() {
    let temp = TempDir::new().unwrap();
    let result = CodeArtsCollector::new(temp.path().join("missing.db")).scan();
    assert_eq!(result.status, SourceScanStatus::Unavailable);
    assert!(result.tasks.is_empty());
}

#[test]
fn codearts_collector_tolerates_session_schema_drift() {
    // CodeArts 的 session 表没有 model/parent_id 列（PR #6 修复前整库报
    // "no such column: model"），缺失列应补 NULL 而不是让整个来源报错。
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("opencode.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE session (
          id TEXT PRIMARY KEY,
          directory TEXT,
          title TEXT,
          time_created INTEGER,
          time_updated INTEGER,
          time_archived INTEGER
        );
        INSERT INTO session VALUES
          ('s1', '/tmp/x/drift-proj', '漂移会话', 1700000000000, 1700000100000, NULL);
        CREATE TABLE todo (session_id TEXT, content TEXT, status TEXT, position INTEGER);
        INSERT INTO todo VALUES ('s1', '修复列漂移', 'completed', 0);
        "#,
    )
    .unwrap();
    drop(conn);

    let result = CodeArtsCollector::new(db_path).scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    assert_eq!(result.tasks.len(), 1);
    let task = &result.tasks[0];
    assert_eq!(task.title, "漂移会话");
    assert_eq!(task.model, None, "missing model column maps to None");
    assert_eq!(task.project_name.as_deref(), Some("drift-proj"));
    assert_eq!(task.todo_total, Some(1));
    assert_eq!(task.todo_done, Some(1));
}

fn write_antigravity_fixture(root: &Path) {
    // IDE 会话：conversation_summaries 提供标题与项目。
    let ide_logs = root.join("ide").join("brain/conv-a/.system_generated/logs");
    fs::create_dir_all(&ide_logs).unwrap();
    fs::write(
        ide_logs.join("transcript_full.jsonl"),
        concat!(
            "{\"step_index\":0,\"source\":\"USER_EXPLICIT\",\"type\":\"USER_INPUT\",\
             \"status\":\"DONE\",\"created_at\":\"2026-10-01T06:11:34Z\",\
             \"content\":\"<USER_REQUEST>\\nIDE 首条输入\\n</USER_REQUEST>\"}\n",
            "{\"step_index\":2,\"source\":\"MODEL\",\"type\":\"RESPONSE\",\"status\":\"DONE\",\
             \"created_at\":\"2026-10-01T06:12:00Z\",\"model\":\"gemini-3-pro\",\"content\":\"ok\"}\n",
            "}]}\n",
            "{\"step_index\":3,\"source\":\"MODEL\",\"type\":\"RESPONSE\",\"status\":\"DONE\",\
             \"created_at\":\"2026-10-02T08:00:00Z\",\"model\":\"gemini-3-pro\",\"content\":\"done\"}\n"
        ),
    )
    .unwrap();

    let conn = Connection::open(root.join("ide/conversation_summaries.db")).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE conversation_summaries (
          conversation_id TEXT PRIMARY KEY,
          title TEXT,
          workspace_uris TEXT
        );
        "#,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO conversation_summaries VALUES ('conv-a', 'IDE 会话标题', \
         '[\"file:///Users/x/work/alpha\"]')",
        [],
    )
    .unwrap();
    drop(conn);

    // CLI 会话：无 summaries 库，标题回退转录首条用户输入。
    let cli_logs = root.join("cli").join("brain/conv-b/.system_generated/logs");
    fs::create_dir_all(&cli_logs).unwrap();
    fs::write(
        cli_logs.join("transcript_full.jsonl"),
        "{\"step_index\":0,\"source\":\"USER_EXPLICIT\",\"type\":\"USER_INPUT\",\
         \"status\":\"DONE\",\"created_at\":\"2026-10-03T09:00:00Z\",\
         \"content\":\"<USER_REQUEST>\\nCLI 首条输入\\n</USER_REQUEST>\"}\n",
    )
    .unwrap();
}

#[test]
fn antigravity_collector_merges_ide_and_cli_roots() {
    let temp = TempDir::new().unwrap();
    write_antigravity_fixture(temp.path());
    let ide_file = temp
        .path()
        .join("ide/brain/conv-a/.system_generated/logs/transcript_full.jsonl");
    let before = fs::read(&ide_file).unwrap();

    let result = AntigravityCollector::new(temp.path().join("ide"), temp.path().join("cli")).scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    assert_eq!(result.source, SOURCE_ANTIGRAVITY);
    assert_eq!(result.tasks.len(), 2);
    assert_eq!(result.files_seen, 2);
    assert_eq!(result.probe_kind, "file_tree");

    let ide = result
        .tasks
        .iter()
        .find(|task| task.source_id == "conv-a")
        .unwrap();
    assert_eq!(ide.title, "IDE 会话标题");
    assert_eq!(ide.title_source, "source");
    assert_eq!(ide.project_name.as_deref(), Some("alpha"));
    assert_eq!(ide.agent_kind.as_deref(), Some("ide"));
    assert_eq!(ide.model.as_deref(), Some("gemini-3-pro"));
    assert_eq!(
        ide.created_at,
        Some(
            chrono::DateTime::parse_from_rfc3339("2026-10-01T06:11:34Z")
                .unwrap()
                .timestamp() as f64
        )
    );
    assert_eq!(
        ide.updated_at,
        Some(
            chrono::DateTime::parse_from_rfc3339("2026-10-02T08:00:00Z")
                .unwrap()
                .timestamp() as f64
        ),
        "游离片段之后的时间戳仍要被读到"
    );

    let cli = result
        .tasks
        .iter()
        .find(|task| task.source_id == "conv-b")
        .unwrap();
    assert_eq!(cli.title, "CLI 首条输入");
    assert_eq!(cli.project_name.as_deref(), Some("default-cli-project"));
    assert_eq!(cli.agent_kind.as_deref(), Some("cli"));

    let after = fs::read(&ide_file).unwrap();
    assert_eq!(before, after, "collector must not modify the transcript");
}

#[test]
fn antigravity_collector_reports_missing_roots() {
    let temp = TempDir::new().unwrap();
    let result = AntigravityCollector::new(temp.path().join("ide"), temp.path().join("cli")).scan();
    assert_eq!(result.status, SourceScanStatus::Unavailable);
    assert!(result.tasks.is_empty());
}

const MINIMAX_SESSION_ID: &str = "mvs_312b6b55e9c143949b13cbdfa4a50f44";
const MINIMAX_SESSION_B64: &str = "bXZzXzMxMmI2YjU1ZTljMTQzOTQ5YjEzY2JkZmE0YTUwZjQ0";

fn write_minimax_fixture(root: &Path) {
    let session_dir = root.join(format!(
        "v2/sessions/2026/11/20/05-47-46-783-session_{MINIMAX_SESSION_B64}"
    ));
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join("manifest.json"),
        format!(
            "{{\"schemaVersion\":1,\"sessionId\":\"{MINIMAX_SESSION_ID}\",\
             \"createdAtMs\":1795153666783,\"updatedAtMs\":1795153670000,\
             \"source\":\"local-runtime\",\"layout\":\"v2-final-dated-session\"}}"
        ),
    )
    .unwrap();
    fs::write(
        session_dir.join("messages.jsonl"),
        concat!(
            "{\"message_id\":\"m1\",\"message\":{\"role\":\"user\",\"content\":[\
             {\"type\":\"text\",\"text\":\"<system-reminder>\\n<agent-context>\\n  ignored\\n</agent-context>\\n</system-reminder>\\n\\n修复构建脚本\\n\"}]}}\n",
            "{\"message_id\":\"m2\",\"message\":{\"role\":\"assistant\",\
             \"content\":[{\"type\":\"text\",\"text\":\"好的\"}],\
             \"model\":\"MiniMax-M2.5\",\"timestamp\":1795153670000,\
             \"usage\":{\"input\":100,\"output\":10}}}\n"
        ),
    )
    .unwrap();

    // 无 manifest 的会话：ID 回退目录名 base64url 解码。
    let legacy_dir = root.join("v2/sessions/2026/11/19/10-00-00-000-session_aGVsbG8");
    fs::create_dir_all(&legacy_dir).unwrap();
    fs::write(
        legacy_dir.join("messages.jsonl"),
        "{\"message_id\":\"m1\",\"message\":{\"role\":\"user\",\
         \"content\":[{\"type\":\"text\",\"text\":\"裸会话输入\"}]}}\n",
    )
    .unwrap();

    let sqlite_dir = root.join("v2/sqlite");
    fs::create_dir_all(&sqlite_dir).unwrap();
    let conn = Connection::open(sqlite_dir.join("runtime-state.sqlite")).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE local_runtime_sessions (
          session_id TEXT,
          workspace_dir TEXT,
          project_workspace_dir TEXT
        );
        "#,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO local_runtime_sessions VALUES (?, '/Users/x/work/minimax-proj/', NULL)",
        [MINIMAX_SESSION_ID],
    )
    .unwrap();
    drop(conn);
}

#[test]
fn minimax_collector_maps_manifest_sessions_and_projects() {
    let temp = TempDir::new().unwrap();
    write_minimax_fixture(temp.path());
    let messages = temp.path().join(format!(
        "v2/sessions/2026/11/20/05-47-46-783-session_{MINIMAX_SESSION_B64}/messages.jsonl"
    ));
    let before = fs::read(&messages).unwrap();

    let result = MiniMaxCollector::new(temp.path().to_path_buf()).scan();

    // 无 manifest 的会话目录名可成功解码出会话 ID（hello），属预期回退，不降级。
    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    assert_eq!(result.source, SOURCE_MINIMAX);
    assert_eq!(result.tasks.len(), 2);
    assert_eq!(result.files_seen, 2);
    assert_eq!(result.probe_kind, "file_tree");

    let session = result
        .tasks
        .iter()
        .find(|task| task.source_id == MINIMAX_SESSION_ID)
        .unwrap();
    assert_eq!(session.title, "修复构建脚本");
    assert_eq!(session.title_source, "source");
    assert_eq!(
        session.project_path.as_deref(),
        Some("/Users/x/work/minimax-proj")
    );
    assert_eq!(session.project_name.as_deref(), Some("minimax-proj"));
    assert_eq!(session.model.as_deref(), Some("MiniMax-M2.5"));
    assert_eq!(session.created_at, Some(1795153666.783));
    assert_eq!(session.updated_at, Some(1795153670.0));
    assert_eq!(session.agent_kind.as_deref(), Some("cli"));

    let legacy = result
        .tasks
        .iter()
        .find(|task| task.source_id == "hello")
        .unwrap();
    assert_eq!(legacy.title, "裸会话输入");
    assert_eq!(legacy.project_name, None);

    let after = fs::read(&messages).unwrap();
    assert_eq!(before, after, "collector must not modify the messages file");
}

#[test]
fn minimax_collector_reports_missing_sessions_root() {
    let temp = TempDir::new().unwrap();
    let result = MiniMaxCollector::new(temp.path().to_path_buf()).scan();
    assert_eq!(result.status, SourceScanStatus::Unavailable);
    assert!(result.tasks.is_empty());
}

const CODEBUDDY_CONVERSATION: &str = "2fc5374e3cfc49958bae3fc026b30d98";
const CODEBUDDY_GENIE_B64: &str = "L1VzZXJzL3dheW5lL0NvZGVCdWRkeS8yMDI2MTEyMDE1MDk0Nw__";

fn write_codebuddy_fixture(root: &Path) {
    let cn_log = root
        .join("cn/20261120T151025/window7/exthost/Tencent-Cloud.coding-copilot/腾讯云代码助手.log");
    fs::create_dir_all(cn_log.parent().unwrap()).unwrap();
    fs::write(
        &cn_log,
        concat!(
            "2026-11-20 15:28:04.652 [info] [CraftInvokableAgent] [a8e5acce] \
             Preparing model: Hy4 flash (hy4flash-free)\n",
            "2026-11-20 15:28:04.660 [info] [AgentReporter] [a8e5acce] onAgentStart: \
             userInput=修复登录页 agent=craft mode=craft \
             conversationId=2fc5374e3cfc49958bae3fc026b30d98 requestId=409\n",
            "2026-11-20 15:28:09.000 [info] [AgentReporter] [a8e5acce]  \
             Step execution metrics: step=1, duration=3997ms, \
             usage={\"inputTokens\":15142,\"outputTokens\":292}\n"
        ),
    )
    .unwrap();

    let ide_log = root.join("ide/2026-11-20/ide-session.log");
    fs::create_dir_all(ide_log.parent().unwrap()).unwrap();
    fs::write(
        &ide_log,
        "[11/21/2026, 10:00:00 AM.500] [Info] [AgentReporter] [a8e5acce]  \
         Step execution metrics: step=2, duration=10ms, usage={\"outputTokens\":10}\n",
    )
    .unwrap();

    // CN 日志根下不在 copilot 目录里的日志必须被排除。
    fs::write(root.join("cn/main.log"), "noise\n").unwrap();

    let genie = root.join(format!("genie/{CODEBUDDY_GENIE_B64}"));
    fs::create_dir_all(&genie).unwrap();
    fs::write(
        genie.join("current.json"),
        format!(
            "{{\"conversationId\":\"{CODEBUDDY_CONVERSATION}\",\
             \"lastUpdated\":\"2026-11-20T07:09:49.346Z\"}}"
        ),
    )
    .unwrap();
}

fn local_epoch_seconds(naive_text: &str, format: &str, micros: u32) -> f64 {
    use chrono::{TimeZone, Timelike};
    let naive = chrono::NaiveDateTime::parse_from_str(naive_text, format).unwrap();
    chrono::Local
        .from_local_datetime(&naive.with_nanosecond(micros * 1_000).unwrap())
        .single()
        .unwrap()
        .timestamp() as f64
        + micros as f64 / 1e6
}

#[test]
fn codebuddy_collector_scans_both_mirrors_and_genie_projects() {
    let temp = TempDir::new().unwrap();
    write_codebuddy_fixture(temp.path());
    let cn_log = temp
        .path()
        .join("cn/20261120T151025/window7/exthost/Tencent-Cloud.coding-copilot/腾讯云代码助手.log");
    let before = fs::read(&cn_log).unwrap();

    let result = CodeBuddyCollector::new(
        temp.path().join("cn"),
        temp.path().join("ide"),
        temp.path().join("genie"),
    )
    .scan();

    assert_eq!(result.status, SourceScanStatus::Ok, "{:?}", result.error);
    assert_eq!(result.source, SOURCE_CODEBUDDY);
    assert_eq!(result.tasks.len(), 1);
    assert_eq!(
        result.files_seen, 2,
        "logs outside the copilot dir are excluded"
    );
    assert_eq!(result.probe_kind, "file_tree");

    let task = &result.tasks[0];
    assert_eq!(task.source_id, CODEBUDDY_CONVERSATION);
    assert_eq!(task.title, "修复登录页");
    assert_eq!(task.title_source, "source");
    assert_eq!(
        task.project_path.as_deref(),
        Some("/Users/wayne/CodeBuddy/20261120150947")
    );
    assert_eq!(task.project_name.as_deref(), Some("20261120150947"));
    assert_eq!(task.model.as_deref(), Some("hy4flash-free"));
    assert_eq!(task.agent_kind.as_deref(), Some("desktop"));
    assert_eq!(
        task.created_at,
        Some(local_epoch_seconds(
            "2026-11-20 15:28:09.000",
            "%Y-%m-%d %H:%M:%S%.f",
            0
        ))
    );
    assert_eq!(
        task.updated_at,
        Some(local_epoch_seconds(
            "11/21/2026, 10:00:00 AM",
            "%m/%d/%Y, %I:%M:%S %p",
            500_000
        )),
        "IDE 镜像日志的时间戳也要计入"
    );

    let after = fs::read(&cn_log).unwrap();
    assert_eq!(before, after, "collector must not modify the logs");
}

#[test]
fn codebuddy_collector_reports_missing_log_dirs() {
    let temp = TempDir::new().unwrap();
    let result = CodeBuddyCollector::new(
        temp.path().join("cn"),
        temp.path().join("ide"),
        temp.path().join("genie"),
    )
    .scan();
    assert_eq!(result.status, SourceScanStatus::Unavailable);
    assert!(result.tasks.is_empty());
}
