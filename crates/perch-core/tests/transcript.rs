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
            &format!(r#"{{"id":"m{index}","role":"assistant","text":"{text}","timestamp":{index}}}"#),
        );
    }
    // 单条超限：12,000 个 CJK 字符（按字符截到 8,000，而非按字节）。
    insert_qoder_message(
        &conn,
        "q-caps",
        "m-long",
        31,
        &format!(r#"{{"id":"m-long","role":"user","text":"{}","timestamp":31}}"#, "乙".repeat(12_000)),
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
