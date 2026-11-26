use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind};
use std::path::PathBuf;

use serde_json::Value;

use crate::collectors::claude::strip_injected_wrappers;
use crate::models::{
    TaskSnapshot, TranscriptMessage, TranscriptOutcome, TranscriptRole, SOURCE_ANTIGRAVITY,
    SOURCE_CLAUDE, SOURCE_CODEX,
};

/// 单一公共读取入口：输入任务快照，输出对话记录（角色、文本、是否截断）或错误。
/// 后续来源通过在本模块增加读取器接入同一入口；读取是内存内单次快照，
/// 不缓存正文、不做后台重读，源文件一律只读打开。
pub fn read_transcript(task: &TaskSnapshot) -> TranscriptOutcome {
    match task.source.as_str() {
        SOURCE_CLAUDE => read_claude_transcript(task),
        SOURCE_CODEX => read_codex_transcript(task),
        SOURCE_ANTIGRAVITY => read_antigravity_transcript(task),
        other if crate::models::transcript_supported(other) => {
            TranscriptOutcome::error("该来源的对话正文读取尚未接入")
        }
        _ => TranscriptOutcome::error("来源不支持读取对话正文"),
    }
}

/// 各文件来源共用的开场：解析指针列并只读打开源文件。
/// 指针缺失、文件不存在、打开失败各自有一行原因，不整屏失败。
fn open_session_file(task: &TaskSnapshot) -> Result<BufReader<File>, TranscriptOutcome> {
    let Some(path_text) = task.source_path.as_deref().filter(|path| !path.is_empty()) else {
        return Err(TranscriptOutcome::error(
            "缓存中缺少会话源文件指针，请刷新后重试",
        ));
    };
    let path = PathBuf::from(path_text);
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(TranscriptOutcome::error(format!(
                "源文件不存在：{}",
                path.display()
            )));
        }
        Err(error) => {
            return Err(TranscriptOutcome::error(format!("读取源文件失败：{error}")));
        }
    };
    Ok(BufReader::new(file))
}

/// 单会话累计器：按时间正序追加消息，工具轮折叠为计数行，
/// 超出会话预算时从最早的消息开始整条丢弃（保最近）。
struct SessionBuilder {
    messages: VecDeque<TranscriptMessage>,
    session_chars: usize,
    session_truncated: bool,
    pending_tools: usize,
}

impl SessionBuilder {
    fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            session_chars: 0,
            session_truncated: false,
            pending_tools: 0,
        }
    }

    fn add_tools(&mut self, count: usize) {
        self.pending_tools += count;
    }

    /// 追加一条消息；其前若有未折叠的工具轮，先输出折叠行。
    fn push_text(&mut self, role: TranscriptRole, text: String) {
        self.flush_tools();
        let (text, truncated) = clip_message_text(text);
        if text.is_empty() {
            return;
        }
        push_message(
            &mut self.messages,
            &mut self.session_chars,
            &mut self.session_truncated,
            TranscriptMessage {
                role,
                text,
                truncated,
            },
        );
    }

    /// 把累积的纯工具轮折叠成一行「工具调用 N 次」。
    fn flush_tools(&mut self) {
        if self.pending_tools == 0 {
            return;
        }
        let count = std::mem::take(&mut self.pending_tools);
        push_message(
            &mut self.messages,
            &mut self.session_chars,
            &mut self.session_truncated,
            tool_call_message(count),
        );
    }

    /// 会话结束：折叠剩余工具轮；没有任何可显示内容时报错而不是空 Ready。
    fn finish(mut self) -> TranscriptOutcome {
        self.flush_tools();
        if self.messages.is_empty() {
            return TranscriptOutcome::error("会话没有可显示的对话内容");
        }
        TranscriptOutcome::Ready {
            messages: self.messages.into_iter().collect(),
            session_truncated: self.session_truncated,
        }
    }
}

fn read_claude_transcript(task: &TaskSnapshot) -> TranscriptOutcome {
    let mut reader = match open_session_file(task) {
        Ok(reader) => reader,
        Err(outcome) => return outcome,
    };
    let mut builder = SessionBuilder::new();
    let mut buffer = Vec::new();

    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => return TranscriptOutcome::error(format!("读取源文件失败：{error}")),
        }
        // 非 UTF-8 行与损坏 JSON 行跳过并继续，不整屏失败。
        let Ok(line) = std::str::from_utf8(&buffer) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };

        match claude_record(&value) {
            Some(ClaudeRecord::Tools(count)) => builder.add_tools(count),
            Some(ClaudeRecord::Text {
                role,
                text,
                tool_uses,
            }) => {
                builder.push_text(role, text);
                builder.add_tools(tool_uses);
            }
            None => {}
        }
    }

    builder.finish()
}

/// Codex rollout：只取 response_item 的 message（user input_text / assistant
/// output_text）；function_call / custom_tool_call 只计数。event_msg 的
/// user_message / agent_message 与 response_item 重复，一律跳过避免双计。
fn read_codex_transcript(task: &TaskSnapshot) -> TranscriptOutcome {
    let mut reader = match open_session_file(task) {
        Ok(reader) => reader,
        Err(outcome) => return outcome,
    };
    let mut builder = SessionBuilder::new();
    let mut buffer = Vec::new();

    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => return TranscriptOutcome::error(format!("读取源文件失败：{error}")),
        }
        let Ok(line) = std::str::from_utf8(&buffer) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("response_item") {
            continue;
        }
        let payload = match value.get("payload") {
            Some(payload) if payload.is_object() => payload,
            _ => continue,
        };
        match payload.get("type").and_then(Value::as_str) {
            Some("message") => match payload.get("role").and_then(Value::as_str) {
                Some("user") => {
                    let texts = codex_message_texts(payload, "input_text");
                    if let Some(text) = joined_visible_text(&texts) {
                        builder.push_text(TranscriptRole::User, text);
                    }
                }
                Some("assistant") => {
                    let texts = codex_message_texts(payload, "output_text");
                    if let Some(text) = joined_visible_text(&texts) {
                        builder.push_text(TranscriptRole::Assistant, text);
                    }
                }
                // developer / system 角色是注入的项目说明，不属于对话。
                _ => {}
            },
            Some("function_call" | "custom_tool_call") => builder.add_tools(1),
            // reasoning / function_call_output / custom_tool_call_output /
            // local_shell_call / web_search_call / other 等一律跳过。
            _ => {}
        }
    }

    builder.finish()
}

/// Codex message payload 的 content 数组 → 文本块列表；每块剥离注入包装后
/// 为空即跳过（environment_context 等常与真实输入同条共存）。
fn codex_message_texts(payload: &Value, content_type: &str) -> Vec<String> {
    let mut texts = Vec::new();
    if let Some(Value::Array(items)) = payload.get("content") {
        for item in items {
            let Some(kind) = item.get("type").and_then(Value::as_str) else {
                continue;
            };
            if kind != content_type {
                continue;
            }
            if let Some(raw) = item.get("text").and_then(Value::as_str) {
                let stripped = strip_codex_wrappers(raw);
                if !stripped.trim().is_empty() {
                    texts.push(stripped);
                }
            }
        }
    }
    texts
}

/// 剥离 Codex 注入的系统包装块（可带属性）：<environment_context>、
/// <user_instructions>、<codex_internal_context …>、<heartbeat>。
fn strip_codex_wrappers(text: &str) -> String {
    const WRAPPERS: &[&str] = &[
        "environment_context",
        "user_instructions",
        "codex_internal_context",
        "heartbeat",
    ];
    let mut output = text.to_owned();
    for tag in WRAPPERS {
        let open = format!("<{tag}");
        let close = format!("</{tag}>");
        while let Some(start) = output.find(&open) {
            let Some(open_end) = output[start..].find('>') else {
                break;
            };
            let Some(relative) = output[start + open_end..].find(&close) else {
                break;
            };
            let end = start + open_end + relative + close.len();
            output.replace_range(start..end, "");
        }
    }
    output
}

/// Antigravity 转录：USER_EXPLICIT（USER_INPUT）是用户输入（常包在
/// <USER_REQUEST> 里）；MODEL 的 PLANNER_RESPONSE 是助手文本；
/// 其余 MODEL 步骤（VIEW_FILE / RUN_COMMAND / GENERIC 等）是工具活动，只计数；
/// SYSTEM 全部跳过。转录不是严格 JSONL（写入中断留下坏行），坏行整行跳过。
fn read_antigravity_transcript(task: &TaskSnapshot) -> TranscriptOutcome {
    let mut reader = match open_session_file(task) {
        Ok(reader) => reader,
        Err(outcome) => return outcome,
    };
    let mut builder = SessionBuilder::new();
    let mut buffer = Vec::new();

    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => return TranscriptOutcome::error(format!("读取源文件失败：{error}")),
        }
        let Ok(line) = std::str::from_utf8(&buffer) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        match value.get("source").and_then(Value::as_str) {
            Some("USER_EXPLICIT") => {
                if let Some(content) = value.get("content").and_then(Value::as_str) {
                    let text = unwrap_user_request(content);
                    if !text.trim().is_empty() {
                        builder.push_text(TranscriptRole::User, text);
                    }
                }
            }
            Some("MODEL") => match value.get("type").and_then(Value::as_str) {
                Some("PLANNER_RESPONSE") => {
                    if let Some(content) = value.get("content").and_then(Value::as_str) {
                        if !content.trim().is_empty() {
                            builder.push_text(TranscriptRole::Assistant, content.to_owned());
                        }
                    }
                }
                // 非对话的模型活动步骤折叠为工具计数。
                Some(_) => builder.add_tools(1),
                None => {}
            },
            // SYSTEM / 缺失来源：系统消息、检查点、对话历史等，不属于对话。
            _ => {}
        }
    }

    builder.finish()
}

/// Antigravity 用户输入被 <USER_REQUEST>…</USER_REQUEST> 包裹时取内部文本。
fn unwrap_user_request(content: &str) -> String {
    let inner = match content
        .find("<USER_REQUEST>")
        .zip(content.find("</USER_REQUEST>"))
        .filter(|(start, end)| end > start)
    {
        Some((start, end)) => &content[start + "<USER_REQUEST>".len()..end],
        None => content,
    };
    inner.trim().to_owned()
}

fn tool_call_message(count: usize) -> TranscriptMessage {
    TranscriptMessage {
        role: TranscriptRole::ToolCall,
        text: format!("工具调用 {count} 次"),
        truncated: false,
    }
}

enum ClaudeRecord {
    Text {
        role: TranscriptRole,
        text: String,
        /// 该记录内 assistant 发起的 tool_use 数量，记入其后的折叠计数。
        tool_uses: usize,
    },
    Tools(usize),
}

/// 只提取 user / assistant 的可见文本：跳过 sidechain、meta、非消息记录，
/// 剥离系统注入包装；工具内容只计数不产出文本。
fn claude_record(value: &Value) -> Option<ClaudeRecord> {
    match value.get("type").and_then(Value::as_str)? {
        "user" => {
            if value
                .get("isSidechain")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || value
                    .get("isMeta")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            {
                return None;
            }
            // 携带 toolUseResult 的 user 记录是工具结果载体，不产出文本。
            if value.get("toolUseResult").is_some() {
                return None;
            }
            let (texts, _) = claude_content_parts(value);
            let text = joined_visible_text(&texts)?;
            Some(ClaudeRecord::Text {
                role: TranscriptRole::User,
                text,
                tool_uses: 0,
            })
        }
        "assistant" => {
            if value
                .get("isSidechain")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return None;
            }
            let (texts, tool_uses) = claude_content_parts(value);
            match joined_visible_text(&texts) {
                Some(text) => Some(ClaudeRecord::Text {
                    role: TranscriptRole::Assistant,
                    text,
                    tool_uses,
                }),
                None if tool_uses > 0 => Some(ClaudeRecord::Tools(tool_uses)),
                None => None,
            }
        }
        _ => None,
    }
}

/// message.content 归一为文本块列表；字符串内容视作单个文本块。
fn claude_content_parts(value: &Value) -> (Vec<String>, usize) {
    let mut texts = Vec::new();
    let mut tool_uses = 0usize;
    match value.pointer("/message/content") {
        Some(Value::String(raw)) => texts.push(strip_injected_wrappers(raw)),
        Some(Value::Array(blocks)) => {
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(raw) = block.get("text").and_then(Value::as_str) {
                            texts.push(strip_injected_wrappers(raw));
                        }
                    }
                    Some("tool_use") => tool_uses += 1,
                    _ => {}
                }
            }
        }
        _ => {}
    }
    (texts, tool_uses)
}

fn joined_visible_text(texts: &[String]) -> Option<String> {
    let visible: Vec<&str> = texts
        .iter()
        .map(String::as_str)
        .filter(|text| !text.trim().is_empty())
        .collect();
    if visible.is_empty() {
        None
    } else {
        Some(visible.join("\n"))
    }
}

fn clip_message_text(text: String) -> (String, bool) {
    if text.chars().count() > crate::models::TRANSCRIPT_MESSAGE_CHAR_LIMIT {
        (
            crate::collectors::common::truncate_chars(
                &text,
                crate::models::TRANSCRIPT_MESSAGE_CHAR_LIMIT,
            ),
            true,
        )
    } else {
        (text, false)
    }
}

/// 追加消息并按单会话预算保留最近部分：超出时从最早的消息开始整条丢弃。
fn push_message(
    messages: &mut VecDeque<TranscriptMessage>,
    session_chars: &mut usize,
    session_truncated: &mut bool,
    message: TranscriptMessage,
) {
    *session_chars += message.text.chars().count();
    messages.push_back(message);
    while *session_chars > crate::models::TRANSCRIPT_SESSION_CHAR_LIMIT && !messages.is_empty() {
        if let Some(removed) = messages.pop_front() {
            *session_chars -= removed.text.chars().count();
            *session_truncated = true;
        }
    }
}
