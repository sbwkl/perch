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

fn codex_task(source_path: &Path) -> TaskSnapshot {
    let mut task = TaskSnapshot::new("codex", "sess-transcript");
    task.title = "合成会话".to_owned();
    task.source_path = Some(source_path.to_string_lossy().into_owned());
    task
}

fn rendered_roles(outcome: TranscriptOutcome) -> Vec<(TranscriptRole, String)> {
    let TranscriptOutcome::Ready { messages, .. } = outcome else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    messages
        .into_iter()
        .map(|message| (message.role, message.text))
        .collect()
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
fn codex_collector_pointer_feeds_the_reader_end_to_end() {
    use perch_core::collectors::codex::CodexCollector;
    use perch_core::collectors::Collector;
    use perch_core::store::CacheStore;

    let temp = TempDir::new().unwrap();
    let sessions = temp.path().join("sessions/2026/11");
    fs::create_dir_all(&sessions).unwrap();
    let session = sessions.join("rollout-transcript.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"timestamp\":\"2026-11-01T08:00:00Z\",\"type\":\"session_meta\",\"payload\":\
             {\"id\":\"sess-transcript\",\"cwd\":\"/tmp/work\"}}\n",
            "{\"timestamp\":\"2026-11-01T08:01:00Z\",\"type\":\"response_item\",\"payload\":\
             {\"type\":\"message\",\"role\":\"user\",\"content\":[\
             {\"type\":\"input_text\",\"text\":\"端到端问题\"}]}}\n",
            "{\"timestamp\":\"2026-11-01T08:02:00Z\",\"type\":\"response_item\",\"payload\":\
             {\"type\":\"function_call\",\"name\":\"shell\",\"arguments\":\"{}\"}}\n",
            "{\"timestamp\":\"2026-11-01T08:03:00Z\",\"type\":\"response_item\",\"payload\":\
             {\"type\":\"message\",\"role\":\"assistant\",\"content\":[\
             {\"type\":\"output_text\",\"text\":\"端到端回答\"}]}}\n",
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let result = CodexCollector::new(temp.path().to_path_buf()).scan();
    let task = result
        .tasks
        .iter()
        .find(|task| task.source_id == "sess-transcript")
        .expect("collector 应产出该会话任务")
        .clone();
    assert_eq!(
        task.source_path.as_deref(),
        Some(session.to_str().unwrap()),
        "扫描时必须写入 rollout 源文件指针"
    );

    let store = CacheStore::open_in_memory().unwrap();
    let scan_id = store.begin_scan("codex", "full", "source", "test").unwrap();
    store
        .save_result(
            &perch_core::models::SourceResult::ok("codex", vec![task], Vec::new()),
            scan_id,
        )
        .unwrap();
    let loaded = store.load_task("codex:sess-transcript").unwrap().unwrap();

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

#[test]
fn codex_transcript_maps_roles_and_skips_non_message_records() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("rollout-roles.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"timestamp\":\"2026-11-01T08:00:00Z\",\"type\":\"session_meta\",\"payload\":\
             {\"id\":\"sess-transcript\",\"cwd\":\"/tmp/work\"}}\n",
            "{\"timestamp\":\"2026-11-01T08:01:00Z\",\"type\":\"turn_context\",\"payload\":\
             {\"cwd\":\"/tmp/work\"}}\n",
            "{\"timestamp\":\"2026-11-01T08:02:00Z\",\"type\":\"response_item\",\"payload\":\
             {\"type\":\"message\",\"role\":\"user\",\"content\":[\
             {\"type\":\"input_text\",\"text\":\"<environment_context>\\n  <cwd>/tmp/work</cwd>\\n</environment_context>\"},\
             {\"type\":\"input_text\",\"text\":\"先看这个函数\"}]}}\n",
            "{\"timestamp\":\"2026-11-01T08:03:00Z\",\"type\":\"response_item\",\"payload\":\
             {\"type\":\"message\",\"role\":\"developer\",\"content\":[\
             {\"type\":\"input_text\",\"text\":\"项目说明注入\"}]}}\n",
            "{\"timestamp\":\"2026-11-01T08:04:00Z\",\"type\":\"event_msg\",\"payload\":\
             {\"type\":\"agent_message\",\"message\":\"不该出现的事件副本\"}}\n",
            "{\"timestamp\":\"2026-11-01T08:05:00Z\",\"type\":\"response_item\",\"payload\":\
             {\"type\":\"reasoning\",\"summary\":[]}}\n",
            "{\"timestamp\":\"2026-11-01T08:06:00Z\",\"type\":\"response_item\",\"payload\":\
             {\"type\":\"message\",\"role\":\"assistant\",\"content\":[\
             {\"type\":\"output_text\",\"text\":\"函数缺少错误处理\"}]}}\n",
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&codex_task(&session));

    let rendered = rendered_roles(outcome);
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "先看这个函数".to_owned()),
            (TranscriptRole::Assistant, "函数缺少错误处理".to_owned()),
        ]
    );
    assert_source_unchanged(&session, &before);
}

#[test]
fn codex_transcript_folds_tool_calls_between_texts() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("rollout-tools.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",",
            "\"content\":[{\"type\":\"input_text\",\"text\":\"修一下\"}]}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",",
            "\"content\":[{\"type\":\"output_text\",\"text\":\"我先看\"}]}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",",
            "\"name\":\"shell\",\"arguments\":\"ls\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",",
            "\"output\":\"文件列表不渲染\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"custom_tool_call\",",
            "\"name\":\"apply_patch\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",",
            "\"content\":[{\"type\":\"output_text\",\"text\":\"修好了\"}]}}\n",
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&codex_task(&session));

    let rendered = rendered_roles(outcome);
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "修一下".to_owned()),
            (TranscriptRole::Assistant, "我先看".to_owned()),
            (TranscriptRole::ToolCall, "工具调用 2 次".to_owned()),
            (TranscriptRole::Assistant, "修好了".to_owned()),
        ]
    );
    assert_source_unchanged(&session, &before);
}

#[test]
fn codex_transcript_clips_single_message_to_eight_thousand_chars() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("rollout-long-message.jsonl");
    let long_text = "甲".repeat(9_000);
    let record = format!(
        concat!(
            "{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"assistant\",",
            "\"content\":[{{\"type\":\"output_text\",\"text\":\"{long_text}\"}}]}}}}\n"
        ),
        long_text = long_text
    );
    fs::write(&session, record).unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&codex_task(&session));

    let TranscriptOutcome::Ready { messages, .. } = outcome else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text.chars().count(), 8_000);
    assert!(messages[0].truncated);
    assert_source_unchanged(&session, &before);
}

#[test]
fn codex_transcript_keeps_the_recent_part_of_oversized_sessions() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("rollout-oversize.jsonl");
    // 30 条 × 9,000 字符（截后 8,000）：总 240,000 超预算，保留最近 25 条。
    let mut body = String::new();
    for index in 1..=30 {
        let text = format!("第{index:02}条{}", "甲".repeat(9_000));
        body.push_str(&format!(
            concat!(
                "{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"assistant\",",
                "\"content\":[{{\"type\":\"output_text\",\"text\":\"{text}\"}}]}}}}\n"
            ),
            text = text
        ));
    }
    fs::write(&session, body).unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&codex_task(&session));

    let TranscriptOutcome::Ready {
        messages,
        session_truncated,
    } = outcome
    else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    assert!(session_truncated, "超预算会话必须标记丢弃");
    assert_eq!(messages.len(), 25);
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
fn codex_transcript_skips_corrupt_and_non_utf8_lines() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("rollout-corrupt.jsonl");
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(
        "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\
         \"content\":[{\"type\":\"input_text\",\"text\":\"损坏行之前\"}]}}\n"
            .as_bytes(),
    );
    body.extend_from_slice(b"{\"type\":\"response_item\",\"payload\":{ broken\n");
    body.extend_from_slice(&[0xFF, 0xFE, b'\n']);
    body.extend_from_slice(
        "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\
         \"content\":[{\"type\":\"output_text\",\"text\":\"损坏行之后\"}]}}\n"
            .as_bytes(),
    );
    fs::write(&session, &body).unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&codex_task(&session));

    let rendered = rendered_roles(outcome);
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "损坏行之前".to_owned()),
            (TranscriptRole::Assistant, "损坏行之后".to_owned()),
        ]
    );
    assert_source_unchanged(&session, &before);
}

#[test]
fn codex_transcript_reports_empty_sessions() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("rollout-empty.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"sess-transcript\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\",\"summary\":[]}}\n",
            "\n\n",
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&codex_task(&session));

    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("没有可显示的对话内容"), "{reason}");
    assert_source_unchanged(&session, &before);
}

#[test]
fn codex_transcript_reports_missing_file_and_pointer() {
    let temp = TempDir::new().unwrap();
    let outcome = read_transcript(&codex_task(&temp.path().join("rollout-missing.jsonl")));
    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("源文件不存在"), "{reason}");

    let mut task = TaskSnapshot::new("codex", "sess-no-pointer");
    task.source_path = None;
    let outcome = read_transcript(&task);
    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("源文件指针"), "{reason}");
}

#[test]
fn antigravity_collector_pointer_feeds_the_reader_end_to_end() {
    use perch_core::collectors::antigravity::AntigravityCollector;
    use perch_core::collectors::Collector;
    use perch_core::store::CacheStore;

    let temp = TempDir::new().unwrap();
    let logs = temp
        .path()
        .join("brain/conv-transcript/.system_generated/logs");
    fs::create_dir_all(&logs).unwrap();
    let session = logs.join("transcript_full.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"step_index\":0,\"source\":\"USER_EXPLICIT\",\"type\":\"USER_INPUT\",\
             \"status\":\"DONE\",\"created_at\":\"2026-11-01T08:00:00Z\",\
             \"content\":\"<USER_REQUEST>\\n端到端问题\\n</USER_REQUEST>\"}\n",
            "{\"step_index\":1,\"source\":\"MODEL\",\"type\":\"RUN_COMMAND\",\
             \"status\":\"DONE\",\"created_at\":\"2026-11-01T08:01:00Z\",\"content\":null}\n",
            "{\"step_index\":2,\"source\":\"MODEL\",\"type\":\"PLANNER_RESPONSE\",\
             \"status\":\"DONE\",\"created_at\":\"2026-11-01T08:02:00Z\",\
             \"model\":\"gemini-3-pro\",\"content\":\"端到端回答\"}\n"
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let result =
        AntigravityCollector::new(temp.path().join("ide-missing"), temp.path().to_path_buf())
            .scan();
    let task = result
        .tasks
        .iter()
        .find(|task| task.source_id == "conv-transcript")
        .expect("collector 应产出该会话任务")
        .clone();
    assert_eq!(
        task.source_path.as_deref(),
        Some(session.to_str().unwrap()),
        "扫描时必须写入转录源文件指针"
    );

    let store = CacheStore::open_in_memory().unwrap();
    let scan_id = store
        .begin_scan("antigravity", "full", "source", "test")
        .unwrap();
    store
        .save_result(
            &perch_core::models::SourceResult::ok("antigravity", vec![task], Vec::new()),
            scan_id,
        )
        .unwrap();
    let loaded = store
        .load_task("antigravity:conv-transcript")
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

fn antigravity_task(source_path: &Path) -> TaskSnapshot {
    let mut task = TaskSnapshot::new("antigravity", "conv-transcript");
    task.title = "合成会话".to_owned();
    task.source_path = Some(source_path.to_string_lossy().into_owned());
    task
}

#[test]
fn antigravity_transcript_maps_roles_and_skips_system_steps() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("transcript-roles.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"source\":\"SYSTEM\",\"type\":\"CONVERSATION_HISTORY\",\
             \"content\":\"历史注入不渲染\"}\n",
            "{\"source\":\"USER_EXPLICIT\",\"type\":\"USER_INPUT\",\
             \"content\":\"<USER_REQUEST>\\n多行第一行\\n第二行\\n</USER_REQUEST>\"}\n",
            "{\"source\":\"MODEL\",\"type\":\"GENERIC\",\"content\":\"Created At: 元数据\"}\n",
            "{\"source\":\"SYSTEM\",\"type\":\"SYSTEM_MESSAGE\",\"content\":\"系统消息\"}\n",
            "{\"source\":\"USER_EXPLICIT\",\"type\":\"USER_INPUT\",\"content\":\"裸用户输入\"}\n",
            "{\"source\":\"MODEL\",\"type\":\"PLANNER_RESPONSE\",\
             \"content\":\"第一段回答\"}\n",
            "{\"source\":\"UNKNOWN_SOURCE\",\"type\":\"OTHER\",\"content\":\"未知来源\"}\n"
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&antigravity_task(&session));

    let rendered = rendered_roles(outcome);
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "多行第一行\n第二行".to_owned()),
            (TranscriptRole::ToolCall, "工具调用 1 次".to_owned()),
            (TranscriptRole::User, "裸用户输入".to_owned()),
            (TranscriptRole::Assistant, "第一段回答".to_owned()),
        ]
    );
    assert_source_unchanged(&session, &before);
}

#[test]
fn antigravity_transcript_folds_tool_activity_between_answers() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("transcript-tools.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"source\":\"USER_EXPLICIT\",\"type\":\"USER_INPUT\",\"content\":\"查一下\"}\n",
            "{\"source\":\"MODEL\",\"type\":\"GREP_SEARCH\",\"content\":null}\n",
            "{\"source\":\"MODEL\",\"type\":\"VIEW_FILE\",\"content\":null}\n",
            "{\"source\":\"MODEL\",\"type\":\"RUN_COMMAND\",\"content\":null}\n",
            "{\"source\":\"MODEL\",\"type\":\"PLANNER_RESPONSE\",\"content\":\"找到了\"}\n",
            "{\"source\":\"MODEL\",\"type\":\"CODE_ACTION\",\"content\":null}\n",
            "{\"source\":\"MODEL\",\"type\":\"PLANNER_RESPONSE\",\"content\":\"已修改\"}\n"
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&antigravity_task(&session));

    let rendered = rendered_roles(outcome);
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "查一下".to_owned()),
            (TranscriptRole::ToolCall, "工具调用 3 次".to_owned()),
            (TranscriptRole::Assistant, "找到了".to_owned()),
            (TranscriptRole::ToolCall, "工具调用 1 次".to_owned()),
            (TranscriptRole::Assistant, "已修改".to_owned()),
        ]
    );
    assert_source_unchanged(&session, &before);
}

#[test]
fn antigravity_transcript_clips_single_message_to_eight_thousand_chars() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("transcript-long.jsonl");
    let long_text = "甲".repeat(9_000);
    let record = format!(
        "{{\"source\":\"MODEL\",\"type\":\"PLANNER_RESPONSE\",\"content\":\"{long_text}\"}}\n"
    );
    fs::write(&session, record).unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&antigravity_task(&session));

    let TranscriptOutcome::Ready { messages, .. } = outcome else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, TranscriptRole::Assistant);
    assert_eq!(messages[0].text.chars().count(), 8_000);
    assert!(messages[0].truncated);
    assert_source_unchanged(&session, &before);
}

#[test]
fn antigravity_transcript_keeps_the_recent_part_of_oversized_sessions() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("transcript-oversize.jsonl");
    let mut body = String::new();
    for index in 1..=30 {
        let text = format!("第{index:02}条{}", "甲".repeat(9_000));
        body.push_str(&format!(
            "{{\"source\":\"MODEL\",\"type\":\"PLANNER_RESPONSE\",\"content\":\"{text}\"}}\n"
        ));
    }
    fs::write(&session, body).unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&antigravity_task(&session));

    let TranscriptOutcome::Ready {
        messages,
        session_truncated,
    } = outcome
    else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    assert!(session_truncated);
    assert_eq!(messages.len(), 25);
    assert!(messages[0].text.starts_with("第06条"));
    assert!(messages[24].text.starts_with("第30条"));
    assert_source_unchanged(&session, &before);
}

#[test]
fn antigravity_transcript_skips_corrupt_and_non_utf8_lines() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("transcript-corrupt.jsonl");
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(
        "{\"source\":\"USER_EXPLICIT\",\"type\":\"USER_INPUT\",\"content\":\"损坏行之前\"}\n"
            .as_bytes(),
    );
    // 写入中断留下的游离片段与坏行。
    body.extend_from_slice(b"}]}\n");
    body.extend_from_slice(&[0xFF, 0xFE, b'\n']);
    body.extend_from_slice(b"{\"source\":\"MODEL\",\"type\":\"\"\n");
    body.extend_from_slice(
        "{\"source\":\"MODEL\",\"type\":\"PLANNER_RESPONSE\",\"content\":\"损坏行之后\"}\n"
            .as_bytes(),
    );
    fs::write(&session, &body).unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&antigravity_task(&session));

    let rendered = rendered_roles(outcome);
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "损坏行之前".to_owned()),
            (TranscriptRole::Assistant, "损坏行之后".to_owned()),
        ]
    );
    assert_source_unchanged(&session, &before);
}

#[test]
fn antigravity_transcript_reports_empty_sessions() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("transcript-empty.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"source\":\"SYSTEM\",\"type\":\"CHECKPOINT\",\"content\":null}\n",
            "\n\n",
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&antigravity_task(&session));

    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("没有可显示的对话内容"), "{reason}");
    assert_source_unchanged(&session, &before);
}

#[test]
fn antigravity_transcript_reports_missing_file_and_pointer() {
    let temp = TempDir::new().unwrap();
    let outcome = read_transcript(&antigravity_task(
        &temp.path().join("transcript-missing.jsonl"),
    ));
    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("源文件不存在"), "{reason}");

    let mut task = TaskSnapshot::new("antigravity", "conv-no-pointer");
    task.source_path = None;
    let outcome = read_transcript(&task);
    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("源文件指针"), "{reason}");
}

fn minimax_task(source_path: &Path) -> TaskSnapshot {
    let mut task = TaskSnapshot::new("minimax", "sess-mm-transcript");
    task.title = "合成会话".to_owned();
    task.source_path = Some(source_path.to_string_lossy().into_owned());
    task
}

#[test]
fn minimax_collector_pointer_feeds_the_reader_end_to_end() {
    use perch_core::collectors::minimax::MiniMaxCollector;
    use perch_core::collectors::Collector;
    use perch_core::store::CacheStore;

    let temp = TempDir::new().unwrap();
    let session_dir = temp
        .path()
        .join("v2/sessions/2026/11/01/05-47-46-session_mm");
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join("manifest.json"),
        concat!(
            "{\"sessionId\":\"sess-mm-transcript\",\"createdAtMs\":1730427600000,\
             \"updatedAtMs\":1730427700000}"
        ),
    )
    .unwrap();
    let session = session_dir.join("messages.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"message_id\":\"m1\",\"turn_id\":\"t1\",\"message\":\
             {\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"端到端问题\"}]}}\n",
            "{\"message_id\":\"m2\",\"turn_id\":\"t1\",\"message\":\
             {\"role\":\"assistant\",\"content\":[\
             {\"type\":\"thinking\",\"thinking\":\"思考过程不渲染\"},\
             {\"type\":\"toolCall\",\"name\":\"shell\"},\
             {\"type\":\"text\",\"text\":\"端到端回答\"}]}}\n",
            "{\"message_id\":\"m3\",\"turn_id\":\"t1\",\"message\":\
             {\"role\":\"toolResult\",\"content\":[{\"type\":\"text\",\"text\":\"工具输出\"}]}}\n"
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let result = MiniMaxCollector::new(temp.path().to_path_buf()).scan();
    let task = result
        .tasks
        .iter()
        .find(|task| task.source_id == "sess-mm-transcript")
        .expect("collector 应产出该会话任务")
        .clone();
    assert_eq!(
        task.source_path.as_deref(),
        Some(session.to_str().unwrap()),
        "扫描时必须写入 messages.jsonl 源文件指针"
    );

    let store = CacheStore::open_in_memory().unwrap();
    let scan_id = store
        .begin_scan("minimax", "full", "source", "test")
        .unwrap();
    store
        .save_result(
            &perch_core::models::SourceResult::ok("minimax", vec![task], Vec::new()),
            scan_id,
        )
        .unwrap();
    let loaded = store
        .load_task("minimax:sess-mm-transcript")
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
            (TranscriptRole::Assistant, "端到端回答"),
            (TranscriptRole::ToolCall, "工具调用 1 次"),
        ]
    );
    assert_source_unchanged(&session, &before);
}

#[test]
fn minimax_transcript_maps_roles_and_strips_injected_blocks() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("messages-roles.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"message\":{\"role\":\"custom\",\"content\":null}}\n",
            "{\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":",
            "\"<system-reminder>\\n注入提醒\\n</system-reminder>\\n真实的问题\"}]}}\n",
            "{\"message\":{\"role\":\"user\",\"content\":\"裸字符串输入\"}}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":[\
             {\"type\":\"thinking\",\"thinking\":\"思考\"}]}}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":\"字符串回答\"}}\n",
            "{\"message\":{\"role\":\"toolResult\",\"content\":[{\"type\":\"text\",\"text\":\"输出\"}]}}\n"
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&minimax_task(&session));

    let rendered = rendered_roles(outcome);
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "真实的问题".to_owned()),
            (TranscriptRole::User, "裸字符串输入".to_owned()),
            (TranscriptRole::Assistant, "字符串回答".to_owned()),
        ]
    );
    assert_source_unchanged(&session, &before);
}

#[test]
fn minimax_transcript_folds_tool_calls_between_texts() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("messages-tools.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"继续\"}]}}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":[\
             {\"type\":\"text\",\"text\":\"先执行\"},\
             {\"type\":\"toolCall\",\"name\":\"shell\"}]}}\n",
            "{\"message\":{\"role\":\"toolResult\",\"content\":[{\"type\":\"text\",\"text\":\"输出\"}]}}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":[\
             {\"type\":\"toolCall\",\"name\":\"edit\"},\
             {\"type\":\"toolCall\",\"name\":\"shell\"}]}}\n",
            "{\"message\":{\"role\":\"toolResult\",\"content\":[{\"type\":\"text\",\"text\":\"输出\"}]}}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":[\
             {\"type\":\"text\",\"text\":\"完成了\"}]}}\n"
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&minimax_task(&session));

    let rendered = rendered_roles(outcome);
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "继续".to_owned()),
            (TranscriptRole::Assistant, "先执行".to_owned()),
            (TranscriptRole::ToolCall, "工具调用 3 次".to_owned()),
            (TranscriptRole::Assistant, "完成了".to_owned()),
        ]
    );
    assert_source_unchanged(&session, &before);
}

#[test]
fn minimax_transcript_clips_single_message_to_eight_thousand_chars() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("messages-long.jsonl");
    let long_text = "甲".repeat(9_000);
    let record = format!(
        "{{\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\
         \"text\":\"{long_text}\"}}]}}}}\n"
    );
    fs::write(&session, record).unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&minimax_task(&session));

    let TranscriptOutcome::Ready { messages, .. } = outcome else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text.chars().count(), 8_000);
    assert!(messages[0].truncated);
    assert_source_unchanged(&session, &before);
}

#[test]
fn minimax_transcript_keeps_the_recent_part_of_oversized_sessions() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("messages-oversize.jsonl");
    let mut body = String::new();
    for index in 1..=30 {
        let text = format!("第{index:02}条{}", "甲".repeat(9_000));
        body.push_str(&format!(
            "{{\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\
             \"text\":\"{text}\"}}]}}}}\n"
        ));
    }
    fs::write(&session, body).unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&minimax_task(&session));

    let TranscriptOutcome::Ready {
        messages,
        session_truncated,
    } = outcome
    else {
        panic!("期望 Ready，实际：{outcome:?}");
    };
    assert!(session_truncated);
    assert_eq!(messages.len(), 25);
    assert!(messages[0].text.starts_with("第06条"));
    assert!(messages[24].text.starts_with("第30条"));
    assert_source_unchanged(&session, &before);
}

#[test]
fn minimax_transcript_skips_corrupt_and_non_utf8_lines() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("messages-corrupt.jsonl");
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(
        "{\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"损坏行之前\"}]}}\n"
            .as_bytes(),
    );
    body.extend_from_slice(b"{\"message\":{\"role\":\"user\", broken\n");
    body.extend_from_slice(&[0xFF, 0xFE, b'\n']);
    body.extend_from_slice("不是 JSON 的一行\n".as_bytes());
    body.extend_from_slice(
        "{\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"损坏行之后\"}]}}\n"
            .as_bytes(),
    );
    fs::write(&session, &body).unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&minimax_task(&session));

    let rendered = rendered_roles(outcome);
    assert_eq!(
        rendered,
        vec![
            (TranscriptRole::User, "损坏行之前".to_owned()),
            (TranscriptRole::Assistant, "损坏行之后".to_owned()),
        ]
    );
    assert_source_unchanged(&session, &before);
}

#[test]
fn minimax_transcript_reports_empty_sessions() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("messages-empty.jsonl");
    fs::write(
        &session,
        concat!(
            "{\"message\":{\"role\":\"toolResult\",\"content\":[{\"type\":\"text\",\"text\":\"输出\"}]}}\n",
            "\n\n",
        ),
    )
    .unwrap();
    let before = fs::read(&session).unwrap();

    let outcome = read_transcript(&minimax_task(&session));

    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("没有可显示的对话内容"), "{reason}");
    assert_source_unchanged(&session, &before);
}

#[test]
fn minimax_transcript_reports_missing_file_and_pointer() {
    let temp = TempDir::new().unwrap();
    let outcome = read_transcript(&minimax_task(&temp.path().join("messages-missing.jsonl")));
    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("源文件不存在"), "{reason}");

    let mut task = TaskSnapshot::new("minimax", "sess-no-pointer");
    task.source_path = None;
    let outcome = read_transcript(&task);
    let TranscriptOutcome::Error { reason } = outcome else {
        panic!("期望 Error，实际：{outcome:?}");
    };
    assert!(reason.contains("源文件指针"), "{reason}");
}

#[test]
fn transcript_supported_set_gains_the_three_jsonl_sources() {
    use perch_core::models::transcript_supported;

    // 能力集合里的三个 JSONL 系来源：读取已接入并返回真实对话。
    assert!(transcript_supported("codex"));
    assert!(transcript_supported("antigravity"));
    assert!(transcript_supported("minimax"));
    // 原有首源仍受支持。
    assert!(transcript_supported("claude"));
    // 明确不提供正文的来源保持不支持。
    assert!(!transcript_supported("trae-cn"));
    assert!(!transcript_supported("codebuddy"));
    // 能力集合里已声明、但尚未接入的来源继续返回「尚未接入」，不整屏失败。
    for source in ["qoder", "opencode", "kilo", "cline", "codearts"] {
        let task = TaskSnapshot::new(source, "pending-1");
        let outcome = read_transcript(&task);
        let TranscriptOutcome::Error { reason } = outcome else {
            panic!("期望 Error，实际：{outcome:?}");
        };
        assert!(reason.contains("尚未接入"), "{source}: {reason}");
    }
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
