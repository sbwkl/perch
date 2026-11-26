use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind};
use std::path::PathBuf;

use serde_json::Value;

use crate::collectors::claude::strip_injected_wrappers;
use crate::models::{
    TaskSnapshot, TranscriptMessage, TranscriptOutcome, TranscriptRole, SOURCE_CLAUDE, SOURCE_CLINE,
};

/// 单一公共读取入口：输入任务快照，输出对话记录（角色、文本、是否截断）或错误。
/// 后续来源通过在本模块增加读取器接入同一入口；读取是内存内单次快照，
/// 不缓存正文、不做后台重读，源文件一律只读打开。
pub fn read_transcript(task: &TaskSnapshot) -> TranscriptOutcome {
    match task.source.as_str() {
        SOURCE_CLAUDE => read_claude_transcript(task),
        SOURCE_CLINE => read_cline_transcript(task),
        other if crate::models::transcript_supported(other) => {
            TranscriptOutcome::error("该来源的对话正文读取尚未接入")
        }
        _ => TranscriptOutcome::error("来源不支持读取对话正文"),
    }
}

fn read_claude_transcript(task: &TaskSnapshot) -> TranscriptOutcome {
    let Some(path_text) = task.source_path.as_deref().filter(|path| !path.is_empty()) else {
        return TranscriptOutcome::error("缓存中缺少会话源文件指针，请刷新后重试");
    };
    let path = PathBuf::from(path_text);
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return TranscriptOutcome::error(format!("源文件不存在：{}", path.display()))
        }
        Err(error) => return TranscriptOutcome::error(format!("读取源文件失败：{error}")),
    };

    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    let mut messages: VecDeque<TranscriptMessage> = VecDeque::new();
    let mut session_chars = 0usize;
    let mut session_truncated = false;
    let mut pending_tools = 0usize;

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
            Some(ClaudeRecord::Tools(count)) => pending_tools += count,
            Some(ClaudeRecord::Text {
                role,
                text,
                tool_uses,
            }) => {
                if pending_tools > 0 {
                    push_message(
                        &mut messages,
                        &mut session_chars,
                        &mut session_truncated,
                        tool_call_message(pending_tools),
                    );
                    pending_tools = 0;
                }
                let (text, truncated) = clip_message_text(text);
                if !text.is_empty() {
                    push_message(
                        &mut messages,
                        &mut session_chars,
                        &mut session_truncated,
                        TranscriptMessage {
                            role,
                            text,
                            truncated,
                        },
                    );
                }
                pending_tools += tool_uses;
            }
            None => {}
        }
    }

    if pending_tools > 0 {
        push_message(
            &mut messages,
            &mut session_chars,
            &mut session_truncated,
            tool_call_message(pending_tools),
        );
    }

    if messages.is_empty() {
        return TranscriptOutcome::error("会话没有可显示的对话内容");
    }
    TranscriptOutcome::Ready {
        messages: messages.into_iter().collect(),
        session_truncated,
    }
}

fn tool_call_message(count: usize) -> TranscriptMessage {
    TranscriptMessage {
        role: TranscriptRole::ToolCall,
        text: format!("工具调用 {count} 次"),
        truncated: false,
    }
}

/// Cline：`<session-id>.messages.json` 是单个 JSON 文档，顶层 `messages` 数组即
/// 会话时间序（探针确认 ts 单调递增），按数组顺序产出。thinking 块跳过；
/// tool_use 只计数并在下一条可见文本前折叠为 tool_call 行；tool_result 与
/// tool_use 一一配对，吸收为空、不重复计数；无可见文本的消息不产出。
fn read_cline_transcript(task: &TaskSnapshot) -> TranscriptOutcome {
    let Some(path_text) = task.source_path.as_deref().filter(|path| !path.is_empty()) else {
        return TranscriptOutcome::error("缓存中缺少会话源文件指针，请刷新后重试");
    };
    let path = PathBuf::from(path_text);
    let raw = match std::fs::read(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return TranscriptOutcome::error(format!("源文件不存在：{}", path.display()))
        }
        Err(error) => return TranscriptOutcome::error(format!("读取源文件失败：{error}")),
    };
    let Ok(text) = std::str::from_utf8(&raw) else {
        return TranscriptOutcome::error("源文件不是有效的 UTF-8 文本");
    };
    let Ok(document) = serde_json::from_str::<Value>(text) else {
        return TranscriptOutcome::error("源文件不是合法的 JSON 文档");
    };
    let Some(records) = document.get("messages").and_then(Value::as_array) else {
        return TranscriptOutcome::error("源文件缺少 messages 数组");
    };

    let mut messages: VecDeque<TranscriptMessage> = VecDeque::new();
    let mut session_chars = 0usize;
    let mut session_truncated = false;
    let mut pending_tools = 0usize;

    for record in records {
        let role = match record.get("role").and_then(Value::as_str) {
            Some("user") => TranscriptRole::User,
            Some("assistant") => TranscriptRole::Assistant,
            _ => continue,
        };
        let (texts, tool_uses) = cline_content_parts(record.get("content"));
        if let Some(text) = joined_visible_text(&texts) {
            if pending_tools > 0 {
                push_message(
                    &mut messages,
                    &mut session_chars,
                    &mut session_truncated,
                    tool_call_message(pending_tools),
                );
                pending_tools = 0;
            }
            let (text, truncated) = clip_message_text(text);
            if !text.is_empty() {
                push_message(
                    &mut messages,
                    &mut session_chars,
                    &mut session_truncated,
                    TranscriptMessage {
                        role,
                        text,
                        truncated,
                    },
                );
            }
        }
        pending_tools += tool_uses;
    }

    if pending_tools > 0 {
        push_message(
            &mut messages,
            &mut session_chars,
            &mut session_truncated,
            tool_call_message(pending_tools),
        );
    }

    if messages.is_empty() {
        return TranscriptOutcome::error("会话没有可显示的对话内容");
    }
    TranscriptOutcome::Ready {
        messages: messages.into_iter().collect(),
        session_truncated,
    }
}

/// Cline content 归一：字符串内容视作单个文本块（防御历史格式）；数组中只取
/// text 块文本，tool_use 计数返回，thinking / tool_result / 未知块忽略。
fn cline_content_parts(content: Option<&Value>) -> (Vec<String>, usize) {
    let mut texts = Vec::new();
    let mut tool_uses = 0usize;
    match content {
        Some(Value::String(raw)) => texts.push(raw.clone()),
        Some(Value::Array(blocks)) => {
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(raw) = block.get("text").and_then(Value::as_str) {
                            texts.push(raw.to_owned());
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
