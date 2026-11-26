use std::fs;
use std::path::Path;

use perch_core::models::{TaskSnapshot, TranscriptOutcome, TranscriptRole, SOURCE_CLAUDE};
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

    // 其余 7 个正文来源已声明支持，但在各自适配票落地前返回未接入。
    let qoder = TaskSnapshot::new("qoder", "q-1");
    let outcome = read_transcript(&qoder);
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
    assert!(
        messages.iter().all(|message| !message.truncated),
        "正常消息不应标记截断"
    );
    assert_source_unchanged(&session, &before);
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
