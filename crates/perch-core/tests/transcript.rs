use std::fs;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use perch_core::models::{
    TaskSnapshot, TranscriptOutcome, TranscriptRole, SOURCE_CLAUDE, SOURCE_CODEARTS, SOURCE_KILO,
    SOURCE_OPENCODE, SOURCE_QODER,
};
use perch_core::transcript::read_transcript;
use tempfile::TempDir;

fn claude_task(source_path: &Path) -> TaskSnapshot {
    let mut task = TaskSnapshot::new(SOURCE_CLAUDE, "session-transcript");
    task.title = "合成会话".to_owned();
    task.source_path = Some(source_path.to_string_lossy().into_owned());
    task
}

fn assert_source_unchanged(path: &Path, before: &[u8]) {
    assert_eq!(
        before,
        fs::read(path).unwrap(),
        "读取前后源文件字节必须一致"
    );
}

#[test]
fn claude_transcript_folds_tool_rounds_into_one_count_line() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("session-tools.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"先读这个文件\"}}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[",
            "{\"type\":\"text\",\"text\":\"先看一下\"},",
            "{\"type\":\"tool_use\",\"name\":\"Read\",\"input\":{\"path\":\"/tmp/a\"}},",
            "{\"type\":\"tool_use\",\"name\":\"Grep\",\"input\":{\"pattern\":\"x\"}}]}}\n",
            "{\"type\":\"user\",\"toolUseResult\":{\"stdout\":\"工具输出\"},",
            "\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",",
            "\"content\":\"文件内容不渲染\"}]}}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[",
            "{\"type\":\"tool_use\",\"name\":\"Edit\",\"input\":{}}]}}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[",
            "{\"type\":\"text\",\"text\":\"结论如下\"}]}}\n",
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&claude_task(&session));

    let TranscriptOutcome::Ready { messages, .. } = outcome else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    let rendered: Vec<(TranscriptRole, &str)> = messages
        .iter()
        .map(|message| (message.role, message.text.as_str()))
        .collect();
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "先读这个文件"),
            (TranscriptRole::Assistant, "先看一下"),
            (TranscriptRole::ToolCall, "工具调用 3 次"),
            (TranscriptRole::Assistant, "结论如下"),
        ]
    );
    assert_source_unchanged(&session, &before);
}

#[test]
fn claude_transcript_tool_only_session_keeps_the_count_line() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("session-tool-only.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[",
            "{\"type\":\"tool_use\",\"name\":\"Read\",\"input\":{}}]}}\n",
            "{\"type\":\"user\",\"toolUseResult\":{\"stdout\":\"工具输出\"},",
            "\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",",
            "\"content\":\"输出\"}]}}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[",
            "{\"type\":\"tool_use\",\"name\":\"Edit\",\"input\":{}}]}}\n",
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&claude_task(&session));

    let TranscriptOutcome::Ready { messages, .. } = outcome else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, TranscriptRole::ToolCall);
    assert_eq!(messages[0].text, "工具调用 2 次");
    assert_source_unchanged(&session, &before);
}

#[test]
fn claude_transcript_session_without_displayable_content_reports_empty() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("session-empty.jsonl");
    fs::write(&session, "\n\n").unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&claude_task(&session));

    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("没有可显示的对话内容"), "{reason}");
    assert_source_unchanged(&session, &before);
}

#[test]
fn claude_transcript_clips_single_message_to_eight_thousand_chars() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("session-long-message.jsonl");
    // 9,000 个 CJK 字符：若按字节截断会得到约 2,666 字符，测试据此钉住「按字符」口径。
    let long_text = "甲".repeat(9_000);
    let record = format!(
        "{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"{long_text}\"}}}}\n"
    );
    fs::write(&session, record).unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&claude_task(&session));

    let TranscriptOutcome::Ready { messages, .. } = outcome else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text.chars().count(), 8_000);
    assert!(messages[0].truncated);
    assert!(messages[0].text.starts_with("甲甲甲甲"));
    assert_source_unchanged(&session, &before);
}

#[test]
fn claude_transcript_keeps_the_recent_part_of_oversized_sessions() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("session-oversize.jsonl");
    // 30 条 × 9,000 字符（截后 8,000）：总 240,000 超预算，保留最近 25 条（200,000 恰不超限）。
    let mut body = String::new();
    for index in 1..=30 {
        let text = format!("第{index:02}条{}", "甲".repeat(9_000));
        body.push_str(&format!(
            "{{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\
             \"content\":[{{\"type\":\"text\",\"text\":\"{text}\"}}]}}}}\n"
        ));
    }
    fs::write(&session, body).unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&claude_task(&session));

    let TranscriptOutcome::Ready {
        messages,
        session_truncated,
    } = outcome
    else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    assert!(session_truncated, "超预算会话必须标记丢弃");
    assert_eq!(messages.len(), 25);
    assert_eq!(messages[0].text.chars().count(), 8_000);
    assert!(
        messages[0].text.starts_with("第06条"),
        "{}",
        &messages[0].text[..12]
    );
    assert!(messages[24].text.starts_with("第30条"));
    let total: usize = messages
        .iter()
        .map(|message| message.text.chars().count())
        .sum();
    assert!(total <= 200_000, "保留部分不得超过会话预算：{total}");
    assert_source_unchanged(&session, &before);
}

#[test]
fn claude_transcript_skips_corrupt_and_non_utf8_lines() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("session-corrupt.jsonl");
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"损坏行之前\"}}\n"
            .as_bytes(),
    );
    body.extend_from_slice(b"{\"type\":\"user\",\"message\":{\"role\":\"user\", \"content\":\n");
    body.extend_from_slice(&[0xFF, 0xFE, b'\n']);
    body.extend_from_slice("不是 JSON 的一行\n".as_bytes());
    body.extend_from_slice(
        "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[\
          {\"type\":\"text\",\"text\":\"损坏行之后\"}]}}\n"
            .as_bytes(),
    );
    fs::write(&session, &body).unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&claude_task(&session));

    let TranscriptOutcome::Ready { messages, .. } = outcome else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    let rendered: Vec<(TranscriptRole, &str)> = messages
        .iter()
        .map(|message| (message.role, message.text.as_str()))
        .collect();
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "损坏行之前"),
            (TranscriptRole::Assistant, "损坏行之后"),
        ]
    );
    assert_source_unchanged(&session, &before);
}

#[test]
fn claude_transcript_reports_missing_source_file() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("session-missing.jsonl");

    let outcome = read_transcript(&claude_task(&session));

    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("源文件不存在"), "{reason}");
}

#[test]
fn claude_transcript_reports_missing_cache_pointer() {
    let mut task = TaskSnapshot::new(SOURCE_CLAUDE, "session-no-pointer");
    task.source_path = None;

    let outcome = read_transcript(&task);

    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("源文件指针"), "{reason}");
}

#[test]
fn claude_transcript_marks_unsupported_and_pending_sources() {
    // Trae CN / CodeBuddy 明确不提供正文。
    let trae = TaskSnapshot::new("trae-cn", "t-1");
    let outcome = read_transcript(&trae);
    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("来源不支持"), "{reason}");

    // 其余 4 个正文来源已声明支持，但在各自适配票落地前返回未接入。
    let codex = TaskSnapshot::new("codex", "c-1");
    let outcome = read_transcript(&codex);
    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("尚未接入"), "{reason}");
}

#[test]
fn claude_transcript_reads_user_and_assistant_turns_in_order() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("session-transcript.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"type\":\"user\",\"cwd\":\"/tmp/work\",\"message\":{\"role\":\"user\",",
            "\"content\":\"帮我看看这个函数\"}}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[",
            "{\"type\":\"text\",\"text\":\"函数缺少错误处理\"}]}}\n",
            "{\"type\":\"user\",\"isSidechain\":true,\"message\":{\"role\":\"user\",",
            "\"content\":[{\"type\":\"text\",\"text\":\"侧链输入\"}]}}\n",
            "{\"type\":\"user\",\"isMeta\":true,\"message\":{\"role\":\"user\",",
            "\"content\":[{\"type\":\"text\",\"text\":\"元信息\"}]}}\n",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[",
            "{\"type\":\"text\",\"text\":\"<system-reminder>上下文提醒</system-reminder>",
            "继续看下一处\"}]}}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[",
            "{\"type\":\"text\",\"text\":\"已修复\"}]}}\n",
            "{\"type\":\"summary\",\"summary\":\"不属于消息\"}\n",
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&claude_task(&session));

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
            (TranscriptRole::User, "帮我看看这个函数"),
            (TranscriptRole::Assistant, "函数缺少错误处理"),
            (TranscriptRole::User, "继续看下一处"),
            (TranscriptRole::Assistant, "已修复"),
        ]
    );
    assert_source_unchanged(&session, &before);
}

// ===== SQLite 系来源（Qoder / OpenCode / Kilo / CodeArts）=====

/// SQLite 读取器按 SourcePaths::detect() 解析路径（环境变量覆盖），测试通过
/// 设置环境变量指向临时 fixture；env 是进程全局的，用互斥锁把本节用例串行化。
static SQLITE_FIXTURE_LOCK: Mutex<()> = Mutex::new(());

fn lock_sqlite_fixtures() -> MutexGuard<'static, ()> {
    SQLITE_FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn dir_file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// 断言读取前后：目录里没有任何文件新增或删除（含 -shm / -wal 侧车），
/// 且数据库文件字节一致。
fn assert_db_untouched(db: &Path, before_files: &[String], before_bytes: &[u8]) {
    let dir = db.parent().unwrap();
    assert_eq!(
        before_files,
        dir_file_names(dir),
        "读取前后目录文件集合必须一致（不得产生任何写入或侧车文件）"
    );
    assert_source_unchanged(db, before_bytes);
}

fn create_qoder_fixture(path: &Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(path).unwrap();
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
    conn
}

fn insert_qoder_message(
    conn: &rusqlite::Connection,
    session_id: &str,
    message_id: &str,
    sequence: i64,
    payload_json: &str,
) {
    conn.execute(
        "INSERT INTO chat_session_messages (session_id, message_id, sequence, payload_json) \
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![session_id, message_id, sequence, payload_json],
    )
    .unwrap();
}

fn insert_qoder_session(conn: &rusqlite::Connection, session_id: &str) {
    conn.execute(
        "INSERT INTO chat_sessions (session_id, title) VALUES (?1, ?2)",
        rusqlite::params![session_id, "合成会话"],
    )
    .unwrap();
}

fn qoder_task(source_id: &str) -> TaskSnapshot {
    let mut task = TaskSnapshot::new(SOURCE_QODER, source_id);
    task.title = "合成会话".to_owned();
    task
}

/// OpenCode / Kilo / CodeArts 共用的 message + part schema（CodeArts 复用 OpenCode 表结构）。
fn create_opencode_fixture(path: &Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT NOT NULL);
         CREATE TABLE message (
           id TEXT PRIMARY KEY,
           session_id TEXT NOT NULL,
           time_created INTEGER NOT NULL,
           time_updated INTEGER NOT NULL,
           data TEXT NOT NULL);
         CREATE TABLE part (
           id TEXT PRIMARY KEY,
           message_id TEXT NOT NULL,
           session_id TEXT NOT NULL,
           time_created INTEGER NOT NULL,
           time_updated INTEGER NOT NULL,
           data TEXT NOT NULL);",
    )
    .unwrap();
    conn
}

fn insert_oc_session(conn: &rusqlite::Connection, session_id: &str) {
    conn.execute(
        "INSERT INTO session (id, title) VALUES (?1, ?2)",
        rusqlite::params![session_id, "合成会话"],
    )
    .unwrap();
}

fn insert_oc_message(
    conn: &rusqlite::Connection,
    session_id: &str,
    message_id: &str,
    time_created: i64,
    role: &str,
) {
    conn.execute(
        "INSERT INTO message (id, session_id, time_created, time_updated, data) \
         VALUES (?1, ?2, ?3, ?3, ?4)",
        rusqlite::params![
            message_id,
            session_id,
            time_created,
            format!(r#"{{"id":"{message_id}","role":"{role}"}}"#)
        ],
    )
    .unwrap();
}

fn insert_oc_part(
    conn: &rusqlite::Connection,
    message_id: &str,
    part_id: &str,
    time_created: i64,
    data: &str,
) {
    conn.execute(
        "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) \
         VALUES (?1, ?2, 'ses-shared', ?3, ?3, ?4)",
        rusqlite::params![part_id, message_id, time_created, data],
    )
    .unwrap();
}

#[test]
fn qoder_transcript_reads_turns_in_sequence_order() {
    let _guard = lock_sqlite_fixtures();
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("main.sqlite");
    let conn = create_qoder_fixture(&db);
    insert_qoder_session(&conn, "q-order");
    // 插入顺序打乱，断言按 sequence 排序。
    insert_qoder_message(
        &conn,
        "q-order",
        "m3",
        3,
        r#"{"id":"m3","role":"user","turnId":"t2","text":"第二个问题","timestamp":3}"#,
    );
    insert_qoder_message(
        &conn,
        "q-order",
        "m2",
        2,
        concat!(
            r#"{"id":"m2","role":"assistant","turnId":"t1","text":"看完了，结论如下","#,
            r#""timestamp":2,"parts":[{"type":"thinking","text":"思考不渲染"},"#,
            r#"{"type":"text","text":"看完了块1"},{"type":"text","text":"块2"}]}"#
        ),
    );
    insert_qoder_message(
        &conn,
        "q-order",
        "m1",
        1,
        r#"{"id":"m1","role":"user","turnId":"t1","text":"先看这个文件","timestamp":1}"#,
    );
    insert_qoder_message(
        &conn,
        "q-order",
        "m4",
        4,
        r#"{"id":"m4","role":"assistant","turnId":"t2","text":"修好了","timestamp":4}"#,
    );
    drop(conn);
    let before_files = dir_file_names(temp.path());
    let before_bytes = fs::read(&db).unwrap();
    std::env::set_var("PERCH_QODER_DB", &db);

    let outcome = read_transcript(&qoder_task("q-order"));

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
            (TranscriptRole::User, "先看这个文件"),
            (TranscriptRole::Assistant, "看完了，结论如下"),
            (TranscriptRole::User, "第二个问题"),
            (TranscriptRole::Assistant, "修好了"),
        ]
    );
    assert!(
        messages.iter().all(|message| !message.truncated),
        "正常消息不应标记截断"
    );
    assert_db_untouched(&db, &before_files, &before_bytes);
    std::env::remove_var("PERCH_QODER_DB");
}

#[test]
fn qoder_transcript_maps_legacy_rows_and_skips_system_records() {
    let _guard = lock_sqlite_fixtures();
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("main.sqlite");
    let conn = create_qoder_fixture(&db);
    insert_qoder_session(&conn, "q-legacy");
    // 旧格式：user 回合正文在 parts；system 角色整条跳过；assistant 带 finalTextId。
    insert_qoder_message(
        &conn,
        "q-legacy",
        "m1",
        1,
        concat!(
            r#"{"id":"m1","turnId":"t1","timestamp":1,"attachments":[],"#,
            r#""parts":[{"type":"text","text":"旧格式提问"}]}"#
        ),
    );
    insert_qoder_message(
        &conn,
        "q-legacy",
        "m2",
        2,
        r#"{"id":"m2","role":"system","subtype":"notice","text":"系统提示不渲染","timestamp":2}"#,
    );
    insert_qoder_message(
        &conn,
        "q-legacy",
        "m3",
        3,
        concat!(
            r#"{"id":"m3","turnId":"t1","timestamp":3,"finalTextId":"pt1","#,
            r#""parts":[{"type":"tool","id":"p1"},{"type":"text","text":"旧格式回答"}]}"#
        ),
    );
    // 纯 hook 记录（无角色、无正文、无工具）不产出。
    insert_qoder_message(
        &conn,
        "q-legacy",
        "m4",
        4,
        r#"{"id":"m4","parts":[{"type":"hook","id":"p2"}],"timestamp":4}"#,
    );
    // 现行格式：assistant 顶层 text 优先，parts 里的流式分块不重复产出。
    insert_qoder_message(
        &conn,
        "q-legacy",
        "m5",
        5,
        concat!(
            r#"{"id":"m5","role":"assistant","text":"顶层完整正文","timestamp":5,"#,
            r#""parts":[{"type":"text","text":"分块一"},{"type":"text","text":"分块二"}]}"#
        ),
    );
    drop(conn);
    let before_files = dir_file_names(temp.path());
    let before_bytes = fs::read(&db).unwrap();
    std::env::set_var("PERCH_QODER_DB", &db);

    let outcome = read_transcript(&qoder_task("q-legacy"));

    let TranscriptOutcome::Ready { messages, .. } = outcome else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    let rendered: Vec<(TranscriptRole, &str)> = messages
        .iter()
        .map(|message| (message.role, message.text.as_str()))
        .collect();
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "旧格式提问"),
            (TranscriptRole::ToolCall, "工具调用 1 次"),
            (TranscriptRole::Assistant, "旧格式回答"),
            (TranscriptRole::Assistant, "顶层完整正文"),
        ]
    );
    assert_db_untouched(&db, &before_files, &before_bytes);
    std::env::remove_var("PERCH_QODER_DB");
}

#[test]
fn qoder_transcript_distinguishes_empty_session_from_missing_one() {
    let _guard = lock_sqlite_fixtures();
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("main.sqlite");
    let conn = create_qoder_fixture(&db);
    // 注册了会话但没有任何消息行 → 空会话；未注册的会话 id → 不存在。
    insert_qoder_session(&conn, "q-empty");
    drop(conn);
    let before_files = dir_file_names(temp.path());
    let before_bytes = fs::read(&db).unwrap();
    std::env::set_var("PERCH_QODER_DB", &db);

    let empty = read_transcript(&qoder_task("q-empty"));
    let TranscriptOutcome::Error { reason } = empty else {
        panic!("期望 Error，实际：{empty:?}");
    };
    assert!(reason.contains("没有可显示的对话内容"), "{reason}");

    let missing = read_transcript(&qoder_task("q-ghost"));
    let TranscriptOutcome::Error { reason } = missing else {
        panic!("期望 Error，实际：{missing:?}");
    };
    assert!(reason.contains("会话不存在"), "{reason}");
    assert_db_untouched(&db, &before_files, &before_bytes);
    std::env::remove_var("PERCH_QODER_DB");
}

#[test]
fn qoder_transcript_clips_single_message_and_keeps_recent_session_tail() {
    let _guard = lock_sqlite_fixtures();
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("main.sqlite");
    let conn = create_qoder_fixture(&db);
    insert_qoder_session(&conn, "q-caps");
    // 30 条 × 9,000 字符（截后 8,000）：总 240,000 超预算，保留最近 25 条。
    for index in 1..=30i64 {
        let text = format!("第{index:02}条{}", "甲".repeat(9_000));
        insert_qoder_message(
            &conn,
            "q-caps",
            &format!("m{index}"),
            index,
            &format!(
                r#"{{"id":"m{index}","role":"assistant","text":"{text}","timestamp":{index}}}"#
            ),
        );
    }
    // 单条超限：12,000 个 CJK 字符（按字符截到 8,000，而非按字节）。
    insert_qoder_message(
        &conn,
        "q-caps",
        "m-long",
        31,
        &format!(
            r#"{{"id":"m-long","role":"user","text":"{}","timestamp":31}}"#,
            "乙".repeat(12_000)
        ),
    );
    drop(conn);
    let before_files = dir_file_names(temp.path());
    let before_bytes = fs::read(&db).unwrap();
    std::env::set_var("PERCH_QODER_DB", &db);

    let outcome = read_transcript(&qoder_task("q-caps"));

    let TranscriptOutcome::Ready {
        messages,
        session_truncated,
    } = outcome
    else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    assert!(session_truncated, "超预算会话必须标记丢弃");
    assert_eq!(messages.len(), 25, "31 条 × 8,000 = 248k，保留最近 25 条");
    assert_eq!(messages[0].text.chars().count(), 8_000);
    assert!(messages[0].text.starts_with("第07条"));
    assert_eq!(messages[24].role, TranscriptRole::User);
    assert_eq!(messages[24].text.chars().count(), 8_000);
    assert!(messages[24].truncated);
    assert!(messages[24].text.starts_with("乙乙乙乙"));
    let total: usize = messages
        .iter()
        .map(|message| message.text.chars().count())
        .sum();
    assert!(total <= 200_000, "保留部分不得超过会话预算：{total}");
    assert_db_untouched(&db, &before_files, &before_bytes);
    std::env::remove_var("PERCH_QODER_DB");
}

/// OpenCode / Kilo / CodeArts 共用同一适配器：一个 fixture、三条来源读出一致结果，
/// CodeArts 不需要单独的表查询实现。
#[test]
fn opencode_family_reads_shared_schema_identically_for_all_three_sources() {
    let _guard = lock_sqlite_fixtures();
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("opencode.db");
    let conn = create_opencode_fixture(&db);
    insert_oc_session(&conn, "ses-shared");
    // 用户消息：synthetic 注入块与 file 附件块不渲染。
    insert_oc_message(&conn, "ses-shared", "m1", 100, "user");
    insert_oc_part(
        &conn,
        "m1",
        "p1",
        100,
        r#"{"type":"text","synthetic":true,"text":"环境上下文"}"#,
    );
    insert_oc_part(
        &conn,
        "m1",
        "p1b",
        101,
        r#"{"type":"file","mime":"image/png","filename":"a.png"}"#,
    );
    insert_oc_part(
        &conn,
        "m1",
        "p1c",
        102,
        r#"{"type":"text","text":"第一个问题"}"#,
    );
    // 助手消息：step-start/reasoning 跳过；text → 工具 ×2 → text。
    insert_oc_message(&conn, "ses-shared", "m2", 110, "assistant");
    insert_oc_part(&conn, "m2", "p2a", 110, r#"{"type":"step-start"}"#);
    insert_oc_part(
        &conn,
        "m2",
        "p2b",
        111,
        r#"{"type":"reasoning","text":"思考不渲染"}"#,
    );
    insert_oc_part(
        &conn,
        "m2",
        "p2c",
        112,
        r#"{"type":"text","text":"开始排查"}"#,
    );
    insert_oc_part(&conn, "m2", "p2d", 113, r#"{"type":"tool","tool":"read"}"#);
    insert_oc_part(&conn, "m2", "p2e", 114, r#"{"type":"tool","tool":"grep"}"#);
    insert_oc_part(
        &conn,
        "m2",
        "p2f",
        115,
        r#"{"type":"text","text":"定位到了"}"#,
    );
    insert_oc_message(&conn, "ses-shared", "m3", 120, "user");
    insert_oc_part(
        &conn,
        "m3",
        "p3a",
        120,
        r#"{"type":"text","text":"修一下"}"#,
    );
    // 纯工具回合：三个工具块，无文本。
    insert_oc_message(&conn, "ses-shared", "m4", 130, "assistant");
    insert_oc_part(&conn, "m4", "p4a", 130, r#"{"type":"tool","tool":"edit"}"#);
    insert_oc_part(&conn, "m4", "p4b", 131, r#"{"type":"tool","tool":"write"}"#);
    insert_oc_part(&conn, "m4", "p4c", 132, r#"{"type":"tool","tool":"patch"}"#);
    drop(conn);
    let before_files = dir_file_names(temp.path());
    let before_bytes = fs::read(&db).unwrap();

    for (source, env_key, source_id) in [
        (SOURCE_OPENCODE, "PERCH_OPENCODE_DB", "ses-shared"),
        (SOURCE_KILO, "PERCH_KILO_DB", "ses-shared"),
        (SOURCE_CODEARTS, "PERCH_CODEARTS_DB", "ses-shared"),
    ] {
        std::env::set_var(env_key, &db);
        let mut task = TaskSnapshot::new(source, source_id);
        task.title = "合成会话".to_owned();
        let outcome = read_transcript(&task);
        let TranscriptOutcome::Ready { messages, .. } = outcome else {
            panic!("{source} 期望 Ready，实际：{outcome:?}");
        };
        let rendered: Vec<(TranscriptRole, &str)> = messages
            .iter()
            .map(|message| (message.role, message.text.as_str()))
            .collect();
        assert_eq!(
            rendered,
            vec![
                (TranscriptRole::User, "第一个问题"),
                (TranscriptRole::Assistant, "开始排查"),
                (TranscriptRole::ToolCall, "工具调用 2 次"),
                (TranscriptRole::Assistant, "定位到了"),
                (TranscriptRole::User, "修一下"),
                (TranscriptRole::ToolCall, "工具调用 3 次"),
            ],
            "{source} 消息序列不一致"
        );
        std::env::remove_var(env_key);
    }
    assert_db_untouched(&db, &before_files, &before_bytes);
}

#[test]
fn opencode_family_clips_message_and_keeps_recent_session_tail() {
    let _guard = lock_sqlite_fixtures();
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("opencode.db");
    let conn = create_opencode_fixture(&db);
    insert_oc_session(&conn, "ses-caps");
    // 30 条 × 9,000 字符（截后 8,000）：总 240,000 超预算，保留最近 25 条。
    for index in 1..=30i64 {
        let time = 1_000 + index;
        let text = format!("第{index:02}条{}", "甲".repeat(9_000));
        insert_oc_message(&conn, "ses-caps", &format!("m{index}"), time, "assistant");
        insert_oc_part(
            &conn,
            &format!("m{index}"),
            &format!("p{index}"),
            time,
            &format!(r#"{{"type":"text","text":"{text}"}}"#),
        );
    }
    // 单条超限：12,000 个 CJK 字符按字符截到 8,000。
    insert_oc_message(&conn, "ses-caps", "m-long", 2_000, "user");
    insert_oc_part(
        &conn,
        "m-long",
        "p-long",
        2_000,
        &format!(r#"{{"type":"text","text":"{}"}}"#, "乙".repeat(12_000)),
    );
    drop(conn);
    let before_files = dir_file_names(temp.path());
    let before_bytes = fs::read(&db).unwrap();
    std::env::set_var("PERCH_KILO_DB", &db);

    let mut task = TaskSnapshot::new(SOURCE_KILO, "ses-caps");
    task.title = "合成会话".to_owned();
    let outcome = read_transcript(&task);

    let TranscriptOutcome::Ready {
        messages,
        session_truncated,
    } = outcome
    else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    assert!(session_truncated, "超预算会话必须标记丢弃");
    assert_eq!(messages.len(), 25, "31 条 × 8,000 = 248k，保留最近 25 条");
    assert_eq!(messages[0].text.chars().count(), 8_000);
    assert!(messages[0].text.starts_with("第07条"));
    assert_eq!(messages[24].role, TranscriptRole::User);
    assert_eq!(messages[24].text.chars().count(), 8_000);
    assert!(messages[24].truncated);
    let total: usize = messages
        .iter()
        .map(|message| message.text.chars().count())
        .sum();
    assert!(total <= 200_000, "保留部分不得超过会话预算：{total}");
    assert_db_untouched(&db, &before_files, &before_bytes);
    std::env::remove_var("PERCH_KILO_DB");
}

#[test]
fn opencode_family_distinguishes_empty_missing_and_db_absent() {
    let _guard = lock_sqlite_fixtures();
    // 空会话（注册了 session 但无消息）与不存在会话共用一个库；缺库单独一个目录。
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("opencode.db");
    let conn = create_opencode_fixture(&db);
    insert_oc_session(&conn, "ses-empty");
    drop(conn);
    let before_files = dir_file_names(temp.path());
    let before_bytes = fs::read(&db).unwrap();
    std::env::set_var("PERCH_CODEARTS_DB", &db);

    let mut empty_task = TaskSnapshot::new(SOURCE_CODEARTS, "ses-empty");
    empty_task.title = "合成会话".to_owned();
    let empty = read_transcript(&empty_task);
    let TranscriptOutcome::Error { reason } = empty else {
        panic!("期望 Error，实际：{empty:?}");
    };
    assert!(reason.contains("没有可显示的对话内容"), "{reason}");

    let mut missing_task = TaskSnapshot::new(SOURCE_CODEARTS, "ses-ghost");
    missing_task.title = "合成会话".to_owned();
    let missing = read_transcript(&missing_task);
    let TranscriptOutcome::Error { reason } = missing else {
        panic!("期望 Error，实际：{missing:?}");
    };
    assert!(reason.contains("会话不存在"), "{reason}");
    assert_db_untouched(&db, &before_files, &before_bytes);
    std::env::remove_var("PERCH_CODEARTS_DB");

    // 数据库文件缺失：错误指向配置路径。
    let absent_temp = TempDir::new().unwrap();
    let absent_db = absent_temp.path().join("absent.db");
    std::env::set_var("PERCH_KILO_DB", &absent_db);
    let mut absent_task = TaskSnapshot::new(SOURCE_KILO, "ses-absent");
    absent_task.title = "合成会话".to_owned();
    let absent = read_transcript(&absent_task);
    let TranscriptOutcome::Error { reason } = absent else {
        panic!("期望 Error，实际：{absent:?}");
    };
    assert!(reason.contains("未找到Kilo数据库"), "{reason}");
    assert!(
        reason.contains(absent_db.to_string_lossy().as_ref()),
        "{reason}"
    );
    std::env::remove_var("PERCH_KILO_DB");
}

/// WAL 库 + 存活写连接（模拟 agent 应用正开着）：只读打开不得产生任何新文件，
/// 数据库文件字节一致，且能经 -wal 读到未 checkpoint 的已提交数据。
#[test]
fn opencode_family_wal_db_stays_byte_identical_with_live_writer() {
    let _guard = lock_sqlite_fixtures();
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("opencode.db");
    let conn = create_opencode_fixture(&db);
    conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
    insert_oc_session(&conn, "ses-wal");
    insert_oc_message(&conn, "ses-wal", "m1", 100, "user");
    insert_oc_part(
        &conn,
        "m1",
        "p1",
        100,
        r#"{"type":"text","text":"WAL 会话提问"}"#,
    );
    insert_oc_message(&conn, "ses-wal", "m2", 110, "assistant");
    insert_oc_part(
        &conn,
        "m2",
        "p2",
        110,
        r#"{"type":"text","text":"WAL 会话回答"}"#,
    );
    // 写连接保持打开（-shm/-wal 存在，数据可能未 checkpoint），模拟应用运行中。
    let before_files = dir_file_names(temp.path());
    assert!(
        before_files.iter().any(|name| name.ends_with("-wal")),
        "fixture 必须处于 WAL 模式"
    );
    let before_bytes = fs::read(&db).unwrap();
    std::env::set_var("PERCH_OPENCODE_DB", &db);

    let mut task = TaskSnapshot::new(SOURCE_OPENCODE, "ses-wal");
    task.title = "合成会话".to_owned();
    let outcome = read_transcript(&task);

    let TranscriptOutcome::Ready { messages, .. } = outcome else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    let rendered: Vec<(TranscriptRole, &str)> = messages
        .iter()
        .map(|message| (message.role, message.text.as_str()))
        .collect();
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "WAL 会话提问"),
            (TranscriptRole::Assistant, "WAL 会话回答"),
        ]
    );
    assert_db_untouched(&db, &before_files, &before_bytes);
    std::env::remove_var("PERCH_OPENCODE_DB");
    drop(conn);
}

#[test]
fn opencode_family_skips_broken_payloads_and_keeps_the_rest() {
    let _guard = lock_sqlite_fixtures();
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("opencode.db");
    let conn = create_opencode_fixture(&db);
    insert_oc_session(&conn, "ses-broken");
    insert_oc_message(&conn, "ses-broken", "m1", 100, "user");
    insert_oc_part(
        &conn,
        "m1",
        "p1",
        100,
        r#"{"type":"text","text":"损坏行之前"}"#,
    );
    // 消息 data 损坏：整条消息无法判角色，其 part 一并跳过。
    conn.execute(
        "INSERT INTO message (id, session_id, time_created, time_updated, data) \
         VALUES ('m2', 'ses-broken', 110, 110, '{\"role\":')",
        [],
    )
    .unwrap();
    insert_oc_part(
        &conn,
        "m2",
        "p2",
        110,
        r#"{"type":"text","text":"不该出现"}"#,
    );
    // part data 损坏：跳过该块，其余照常。
    insert_oc_part(&conn, "m3", "p3-broken", 120, "{\"type\":");
    insert_oc_message(&conn, "ses-broken", "m3", 120, "assistant");
    insert_oc_part(
        &conn,
        "m3",
        "p3",
        121,
        r#"{"type":"text","text":"损坏行之后"}"#,
    );
    drop(conn);
    let before_files = dir_file_names(temp.path());
    let before_bytes = fs::read(&db).unwrap();
    std::env::set_var("PERCH_KILO_DB", &db);

    let mut task = TaskSnapshot::new(SOURCE_KILO, "ses-broken");
    task.title = "合成会话".to_owned();
    let outcome = read_transcript(&task);

    let TranscriptOutcome::Ready { messages, .. } = outcome else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    let rendered: Vec<(TranscriptRole, &str)> = messages
        .iter()
        .map(|message| (message.role, message.text.as_str()))
        .collect();
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "损坏行之前"),
            (TranscriptRole::Assistant, "损坏行之后"),
        ]
    );
    assert_db_untouched(&db, &before_files, &before_bytes);
    std::env::remove_var("PERCH_KILO_DB");
}

#[test]
fn sqlite_sources_are_transcript_supported_and_claude_stays_supported() {
    use perch_core::models::transcript_supported;

    // 本票接入的四个 SQLite 系来源在能力集合内（不断言集合总数，其他来源由各自票据负责）。
    for source in ["qoder", "opencode", "kilo", "codearts"] {
        assert!(transcript_supported(source), "{source} 应当支持正文读取");
    }
    assert!(transcript_supported("claude"), "既有来源不应被移出能力集合");
}

#[test]
fn sqlite_sources_report_missing_message_tables() {
    let _guard = lock_sqlite_fixtures();
    // Qoder：只有会话表没有消息表。
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("main.sqlite");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch(
        "CREATE TABLE chat_sessions (session_id TEXT PRIMARY KEY, title TEXT NOT NULL);",
    )
    .unwrap();
    insert_qoder_session(&conn, "q-notable");
    drop(conn);
    std::env::set_var("PERCH_QODER_DB", &db);
    let outcome = read_transcript(&qoder_task("q-notable"));
    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("缺少消息表"), "{reason}");
    std::env::remove_var("PERCH_QODER_DB");

    // OpenCode 系：只有 session 表，message/part/session_message 全缺。
    let temp2 = TempDir::new().unwrap();
    let db2 = temp2.path().join("opencode.db");
    let conn2 = rusqlite::Connection::open(&db2).unwrap();
    conn2
        .execute_batch("CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT NOT NULL);")
        .unwrap();
    insert_oc_session(&conn2, "ses-notable");
    drop(conn2);
    std::env::set_var("PERCH_CODEARTS_DB", &db2);
    let mut task = TaskSnapshot::new(SOURCE_CODEARTS, "ses-notable");
    task.title = "合成会话".to_owned();
    let outcome = read_transcript(&task);
    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("CodeArts 数据库缺少消息表"), "{reason}");
    std::env::remove_var("PERCH_CODEARTS_DB");
}

/// 旧库没有 message/part 表时回退 session_message（data 内嵌 parts）。
#[test]
fn opencode_family_falls_back_to_legacy_session_message_table() {
    let _guard = lock_sqlite_fixtures();
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("opencode.db");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch(
        "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT NOT NULL);
         CREATE TABLE session_message (
           id TEXT PRIMARY KEY,
           session_id TEXT NOT NULL,
           type TEXT NOT NULL,
           time_created INTEGER NOT NULL,
           time_updated INTEGER NOT NULL,
           data TEXT NOT NULL);",
    )
    .unwrap();
    insert_oc_session(&conn, "ses-legacy");
    for (id, row_type, time, data) in [
        (
            "s1",
            "user",
            100,
            r#"{"role":"user","parts":[{"type":"text","text":"旧库提问"}]}"#,
        ),
        (
            "s2",
            "assistant",
            110,
            r#"{"role":"assistant","parts":[{"type":"tool","tool":"read"},{"type":"text","text":"旧库回答"}]}"#,
        ),
        (
            "s3",
            "assistant",
            120,
            r#"{"role":"assistant","text":"纯文本旧格式"}"#,
        ),
    ] {
        conn.execute(
            "INSERT INTO session_message (id, session_id, type, time_created, time_updated, data) \
             VALUES (?1, 'ses-legacy', ?2, ?3, ?3, ?4)",
            rusqlite::params![id, row_type, time, data],
        )
        .unwrap();
    }
    drop(conn);
    let before_files = dir_file_names(temp.path());
    let before_bytes = fs::read(&db).unwrap();
    std::env::set_var("PERCH_CODEARTS_DB", &db);

    let mut task = TaskSnapshot::new(SOURCE_CODEARTS, "ses-legacy");
    task.title = "合成会话".to_owned();
    let outcome = read_transcript(&task);

    let TranscriptOutcome::Ready { messages, .. } = outcome else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    let rendered: Vec<(TranscriptRole, &str)> = messages
        .iter()
        .map(|message| (message.role, message.text.as_str()))
        .collect();
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "旧库提问"),
            (TranscriptRole::ToolCall, "工具调用 1 次"),
            (TranscriptRole::Assistant, "旧库回答"),
            (TranscriptRole::Assistant, "纯文本旧格式"),
        ]
    );
    assert_db_untouched(&db, &before_files, &before_bytes);
    std::env::remove_var("PERCH_CODEARTS_DB");
}

#[test]
fn qoder_transcript_folds_tool_parts_into_one_count_line() {
    let _guard = lock_sqlite_fixtures();
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("main.sqlite");
    let conn = create_qoder_fixture(&db);
    insert_qoder_session(&conn, "q-tools");
    insert_qoder_message(
        &conn,
        "q-tools",
        "m1",
        1,
        r#"{"id":"m1","role":"user","text":"先改这个文件","timestamp":1}"#,
    );
    // 助手回合：文本 + 两个工具块（thinking/hook 不计），随后一条纯工具回合。
    insert_qoder_message(
        &conn,
        "q-tools",
        "m2",
        2,
        concat!(
            r#"{"id":"m2","role":"assistant","text":"先读一下","#,
            r#""parts":[{"type":"thinking","text":"思考"},{"type":"tool","tool":"read","#,
            r#""id":"p1"},{"type":"tool","tool":"grep","id":"p2"},{"type":"hook","id":"p3"}]}"#
        ),
    );
    insert_qoder_message(
        &conn,
        "q-tools",
        "m3",
        3,
        r#"{"id":"m3","role":"assistant","text":"","parts":[{"type":"tool","tool":"edit","id":"p4"}],"timestamp":3}"#,
    );
    insert_qoder_message(
        &conn,
        "q-tools",
        "m4",
        4,
        r#"{"id":"m4","role":"assistant","text":"结论如下","timestamp":4}"#,
    );
    drop(conn);
    let before_files = dir_file_names(temp.path());
    let before_bytes = fs::read(&db).unwrap();
    std::env::set_var("PERCH_QODER_DB", &db);

    let outcome = read_transcript(&qoder_task("q-tools"));

    let TranscriptOutcome::Ready { messages, .. } = outcome else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    let rendered: Vec<(TranscriptRole, &str)> = messages
        .iter()
        .map(|message| (message.role, message.text.as_str()))
        .collect();
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "先改这个文件"),
            (TranscriptRole::Assistant, "先读一下"),
            (TranscriptRole::ToolCall, "工具调用 3 次"),
            (TranscriptRole::Assistant, "结论如下"),
        ]
    );
    assert_db_untouched(&db, &before_files, &before_bytes);
    std::env::remove_var("PERCH_QODER_DB");
}

#[test]
fn claude_collector_pointer_feeds_the_reader_end_to_end() {
    use perch_core::collectors::claude::ClaudeCollector;
    use perch_core::collectors::Collector;
    use perch_core::store::CacheStore;

    let temp = TempDir::new().unwrap();
    let projects = temp.path().join("projects/-tmp-work");
    fs::create_dir_all(&projects).unwrap();
    let session = projects.join("session-transcript.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"type\":\"user\",\"cwd\":\"/tmp/work\",\"message\":{\"role\":\"user\",",
            "\"content\":\"端到端问题\"}}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[",
            "{\"type\":\"tool_use\",\"name\":\"Read\",\"input\":{}}]}}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[",
            "{\"type\":\"text\",\"text\":\"端到端回答\"}]}}\n",
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let result = ClaudeCollector::new(temp.path().to_path_buf()).scan();
    let task = result
        .tasks
        .iter()
        .find(|task| task.source_id == "session-transcript")
        .expect("collector 应产出该会话任务")
        .clone();
    assert_eq!(
        task.source_path.as_deref(),
        Some(session.to_str().unwrap()),
        "扫描时必须写入可重建的源文件指针"
    );

    let store = CacheStore::open_in_memory().unwrap();
    let scan_id = store
        .begin_scan("claude", "full", "source", "test")
        .unwrap();
    store
        .save_result(
            &perch_core::models::SourceResult::ok("claude", vec![task], Vec::new()),
            scan_id,
        )
        .unwrap();
    let loaded = store
        .load_task("claude:session-transcript")
        .unwrap()
        .unwrap();

    let outcome = read_transcript(&loaded);
    let TranscriptOutcome::Ready { messages, .. } = outcome else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    let rendered: Vec<(TranscriptRole, &str)> = messages
        .iter()
        .map(|message| (message.role, message.text.as_str()))
        .collect();
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "端到端问题"),
            (TranscriptRole::ToolCall, "工具调用 1 次"),
            (TranscriptRole::Assistant, "端到端回答"),
        ]
    );
    assert_source_unchanged(&session, &before);
}
