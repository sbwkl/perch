use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::Value;

use crate::collectors::claude::strip_injected_wrappers;
use crate::collectors::common::{open_read_only, table_exists};
use crate::models::{
    TaskSnapshot, TranscriptMessage, TranscriptOutcome, TranscriptRole, SOURCE_ANTIGRAVITY,
    SOURCE_CLAUDE, SOURCE_CLINE, SOURCE_CODEARTS, SOURCE_CODEX, SOURCE_KILO, SOURCE_MINIMAX,
    SOURCE_OPENCODE, SOURCE_QODER,
};

/// 单一公共读取入口：输入任务快照，输出对话记录（角色、文本、是否截断）或错误。
/// 后续来源通过在本模块增加读取器接入同一入口；读取是内存内单次快照，
/// 不缓存正文、不做后台重读，源文件一律只读打开。
pub fn read_transcript(task: &TaskSnapshot) -> TranscriptOutcome {
    match task.source.as_str() {
        SOURCE_CLAUDE => read_claude_transcript(task),
        SOURCE_QODER => read_qoder_transcript(task),
        SOURCE_OPENCODE | SOURCE_KILO | SOURCE_CODEARTS => read_opencode_transcript(task),
        SOURCE_CODEX => read_codex_transcript(task),
        SOURCE_ANTIGRAVITY => read_antigravity_transcript(task),
        SOURCE_MINIMAX => read_minimax_transcript(task),
        SOURCE_CLINE => read_cline_transcript(task),
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

/// MiniMax messages.jsonl：message.role 区分 user / assistant / toolResult。
/// user 的 text 内容剥离注入的 reminder 块；assistant 的 thinking 不渲染、
/// toolCall 只计数、text 是可见文本；toolResult / custom 整条跳过。
fn read_minimax_transcript(task: &TaskSnapshot) -> TranscriptOutcome {
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
        let Some(message) = value.get("message").filter(|m| m.is_object()) else {
            continue;
        };
        match message.get("role").and_then(Value::as_str) {
            Some("user") => {
                let text = minimax_user_text(message);
                if !text.trim().is_empty() {
                    builder.push_text(TranscriptRole::User, text);
                }
            }
            Some("assistant") => {
                let (texts, tool_calls) = minimax_assistant_parts(message);
                if let Some(text) = joined_visible_text(&texts) {
                    builder.push_text(TranscriptRole::Assistant, text);
                }
                builder.add_tools(tool_calls);
            }
            // toolResult 是工具输出（工具调用已在 assistant 记录计数）；
            // custom 是内部消息，均不属于对话。
            _ => {}
        }
    }

    builder.finish()
}

/// MiniMax user message 的 content：字符串或 text 分块数组，剥离注入 reminder。
fn minimax_user_text(message: &Value) -> String {
    let joined = match message.get("content") {
        Some(Value::String(raw)) => raw.clone(),
        Some(Value::Array(parts)) => {
            let mut texts = Vec::new();
            for part in parts {
                if part.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(raw) = part.get("text").and_then(Value::as_str) {
                        texts.push(raw.to_owned());
                    }
                }
            }
            texts.join("\n")
        }
        _ => String::new(),
    };
    crate::collectors::minimax::strip_reminder_blocks(&joined)
        .trim()
        .to_owned()
}

/// MiniMax assistant message 的 content 分块 →（可见文本块, toolCall 计数）。
fn minimax_assistant_parts(message: &Value) -> (Vec<String>, usize) {
    let mut texts = Vec::new();
    let mut tool_calls = 0usize;
    match message.get("content") {
        Some(Value::String(raw)) => texts.push(raw.clone()),
        Some(Value::Array(parts)) => {
            for part in parts {
                match part.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(raw) = part.get("text").and_then(Value::as_str) {
                            texts.push(raw.to_owned());
                        }
                    }
                    Some("toolCall") => tool_calls += 1,
                    // thinking 是思考过程，不读取不渲染。
                    _ => {}
                }
            }
        }
        _ => {}
    }
    (texts, tool_calls)
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

/// SQLite 系读取器的公共入口：按配置路径（含环境变量覆盖）只读打开源库。
/// 打开失败或文件缺失统一转为 TranscriptOutcome::Error，不向上传播。
fn open_transcript_db(label: &str, path: &Path) -> Result<Connection, TranscriptOutcome> {
    if !path.is_file() {
        return Err(TranscriptOutcome::error(format!(
            "未找到{label}数据库：{}",
            path.display()
        )));
    }
    open_read_only(path)
        .map_err(|error| TranscriptOutcome::error(format!("打开{label}数据库失败：{error}")))
}

/// 会话级存在性检查：Some(false) 表示源库注册表里没有该会话；
/// 表缺失等查询错误返回 None（跳过检查，交由消息查询自然落到空会话）。
fn source_session_exists(conn: &Connection, exists_sql: &str, source_id: &str) -> Option<bool> {
    match conn.query_row(exists_sql, [source_id], |row| row.get::<_, i64>(0)) {
        Ok(_) => Some(true),
        Err(rusqlite::Error::QueryReturnedNoRows) => Some(false),
        Err(_) => None,
    }
}

/// Qoder：正文在 `chat_session_messages.payload_json`。现行格式 role 在 payload 里、
/// 完整正文在顶层 text（parts 里的 text 是流式分块）；旧格式（无 role 字段）正文
/// 只在 parts 的 text 块里，带 finalTextId 的是助手回合。system 等角色整条跳过。
fn read_qoder_transcript(task: &TaskSnapshot) -> TranscriptOutcome {
    let db_path = crate::paths::SourcePaths::detect().qoder_db;
    let conn = match open_transcript_db("Qoder", &db_path) {
        Ok(conn) => conn,
        Err(outcome) => return outcome,
    };
    if source_session_exists(
        &conn,
        "SELECT 1 FROM chat_sessions WHERE session_id = ?1",
        &task.source_id,
    ) == Some(false)
    {
        return TranscriptOutcome::error(format!("会话不存在：{}", task.source_id));
    }
    if !table_exists(&conn, "chat_session_messages").unwrap_or(false) {
        return TranscriptOutcome::error("Qoder 数据库缺少消息表");
    }

    let mut stmt = match conn.prepare(
        "SELECT payload_json FROM chat_session_messages \
         WHERE session_id = ?1 ORDER BY sequence, rowid",
    ) {
        Ok(stmt) => stmt,
        Err(error) => return TranscriptOutcome::error(format!("读取会话消息失败：{error}")),
    };
    let mut rows = match stmt.query([&task.source_id]) {
        Ok(rows) => rows,
        Err(error) => return TranscriptOutcome::error(format!("读取会话消息失败：{error}")),
    };

    let mut builder = SessionBuilder::new();
    while let Some(row) = match rows.next() {
        Ok(row) => row,
        Err(error) => return TranscriptOutcome::error(format!("读取会话消息失败：{error}")),
    } {
        let payload: String = match row.get(0) {
            Ok(payload) => payload,
            Err(error) => return TranscriptOutcome::error(format!("读取会话消息失败：{error}")),
        };
        // 损坏的 payload 跳过并继续，不整屏失败。
        let Ok(value) = serde_json::from_str::<Value>(&payload) else {
            continue;
        };
        for event in transcript_events(&value) {
            match event {
                TranscriptEvent::Text { role, text } => builder.push_text(role, text),
                TranscriptEvent::Tools(count) => builder.add_tools(count),
            }
        }
    }

    builder.finish()
}

enum TranscriptEvent {
    Text { role: TranscriptRole, text: String },
    Tools(usize),
}

/// payload_json 归一为事件序列：role 取 payload.role，缺失时按结构判定
/// （finalTextId → 助手，其余 → 用户）；system 等角色整条跳过。
/// 现行格式正文在顶层 text（parts 的 text 是流式分块，不重复产出），行内工具
/// 计到其后；旧格式正文只在 parts 的 text 块里，按 parts 顺序回放，文本前的
/// 工具块先冲刷成计数行。
fn transcript_events(value: &Value) -> Vec<TranscriptEvent> {
    let role_raw = value.get("role").and_then(Value::as_str);
    let role = match role_raw {
        Some("user") => TranscriptRole::User,
        Some("assistant") => TranscriptRole::Assistant,
        Some(_) => return Vec::new(),
        None if value.get("finalTextId").is_some() => TranscriptRole::Assistant,
        None => TranscriptRole::User,
    };

    let mut parts_texts = Vec::new();
    let mut tools = 0usize;
    if let Some(parts) = value.get("parts").and_then(Value::as_array) {
        for part in parts {
            match part.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(raw) = part.get("text").and_then(Value::as_str) {
                        parts_texts.push(raw.to_owned());
                    }
                }
                Some("tool") => tools += 1,
                _ => {}
            }
        }
    }

    match role_raw {
        Some(_) => {
            let top = value.get("text").and_then(Value::as_str).unwrap_or("");
            let text = if top.trim().is_empty() {
                joined_visible_text(&parts_texts)
            } else {
                Some(top.to_owned())
            };
            let mut events = Vec::new();
            if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
                events.push(TranscriptEvent::Text { role, text });
            }
            if tools > 0 {
                events.push(TranscriptEvent::Tools(tools));
            }
            events
        }
        None => {
            // 旧格式：按 parts 顺序回放；相邻 text 块合并为一条，文本前的 tool 块
            // 先冲刷成计数行，尾部 tool 块计到其后。
            let mut events = Vec::new();
            let mut tool_run = 0usize;
            let mut text_run: Vec<String> = Vec::new();
            if let Some(parts) = value.get("parts").and_then(Value::as_array) {
                for part in parts {
                    match part.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            if tool_run > 0 {
                                events.push(TranscriptEvent::Tools(tool_run));
                                tool_run = 0;
                            }
                            if let Some(raw) = part.get("text").and_then(Value::as_str) {
                                if !raw.trim().is_empty() {
                                    text_run.push(raw.to_owned());
                                }
                            }
                        }
                        Some("tool") => tool_run += 1,
                        _ => {}
                    }
                }
            }
            if let Some(text) = joined_visible_text(&text_run) {
                events.push(TranscriptEvent::Text { role, text });
            }
            if tool_run > 0 {
                events.push(TranscriptEvent::Tools(tool_run));
            }
            events
        }
    }
}

/// OpenCode / Kilo / CodeArts 共用适配器（CodeArts 复用 OpenCode 表结构，无需单独实现）：
/// 正文在 `message` + `part` 两表的 `data` JSON 里，part.type=text 为可见文本、
/// type=tool 只计数；synthetic 注入块、reasoning、step-start/finish、file 等不渲染。
/// 旧库无 `message` 表时回退 `session_message`（data JSON 内嵌 parts 或单条 text）。
fn read_opencode_transcript(task: &TaskSnapshot) -> TranscriptOutcome {
    let paths = crate::paths::SourcePaths::detect();
    let (label, candidates): (&str, Vec<PathBuf>) = match task.source.as_str() {
        SOURCE_OPENCODE => (
            "OpenCode",
            vec![paths.opencode_db.clone(), paths.opencode_stable_db.clone()],
        ),
        SOURCE_KILO => ("Kilo", vec![paths.kilo_db.clone()]),
        SOURCE_CODEARTS => ("CodeArts", vec![paths.codearts_db.clone()]),
        _ => return TranscriptOutcome::error("来源不支持读取对话正文"),
    };
    let Some(db_path) = crate::paths::SourcePaths::first_existing(&candidates) else {
        return TranscriptOutcome::error(format!(
            "未找到{label}数据库：{}",
            candidates[0].display()
        ));
    };
    let conn = match open_transcript_db(label, &db_path) {
        Ok(conn) => conn,
        Err(outcome) => return outcome,
    };

    // 会话存在性与空会话区分：session_v2 优先（新版 OpenCode），Kilo / CodeArts 用 session。
    if table_exists(&conn, "session_v2").unwrap_or(false) {
        if source_session_exists(
            &conn,
            "SELECT 1 FROM session_v2 WHERE id = ?1",
            &task.source_id,
        ) == Some(false)
        {
            return TranscriptOutcome::error(format!("会话不存在：{}", task.source_id));
        }
    } else if table_exists(&conn, "session").unwrap_or(false)
        && source_session_exists(
            &conn,
            "SELECT 1 FROM session WHERE id = ?1",
            &task.source_id,
        ) == Some(false)
    {
        return TranscriptOutcome::error(format!("会话不存在：{}", task.source_id));
    }

    let has_message_part = table_exists(&conn, "message").unwrap_or(false)
        && table_exists(&conn, "part").unwrap_or(false);
    if !has_message_part && !table_exists(&conn, "session_message").unwrap_or(false) {
        return TranscriptOutcome::error(format!("{label} 数据库缺少消息表"));
    }

    let mut builder = SessionBuilder::new();
    if has_message_part {
        let mut stmt = match conn.prepare(
            "SELECT m.id, m.data, p.data FROM message m JOIN part p ON p.message_id = m.id \
             WHERE m.session_id = ?1 ORDER BY m.time_created, m.rowid, p.time_created, p.rowid",
        ) {
            Ok(stmt) => stmt,
            Err(error) => return TranscriptOutcome::error(format!("读取会话消息失败：{error}")),
        };
        let mut rows = match stmt.query([&task.source_id]) {
            Ok(rows) => rows,
            Err(error) => return TranscriptOutcome::error(format!("读取会话消息失败：{error}")),
        };
        let mut last_message_id: Option<String> = None;
        let mut current_role = None;
        while let Some(row) = match rows.next() {
            Ok(row) => row,
            Err(error) => return TranscriptOutcome::error(format!("读取会话消息失败：{error}")),
        } {
            let message_id: String = match row.get(0) {
                Ok(value) => value,
                Err(error) => {
                    return TranscriptOutcome::error(format!("读取会话消息失败：{error}"))
                }
            };
            if last_message_id.as_deref() != Some(message_id.as_str()) {
                let message_data: String = match row.get(1) {
                    Ok(value) => value,
                    Err(error) => {
                        return TranscriptOutcome::error(format!("读取会话消息失败：{error}"))
                    }
                };
                current_role = serde_json::from_str::<Value>(&message_data)
                    .ok()
                    .and_then(|value| opencode_role(&value, None));
                last_message_id = Some(message_id);
            }
            let Some(role) = current_role else {
                continue;
            };
            let part_data: String = match row.get(2) {
                Ok(value) => value,
                Err(error) => {
                    return TranscriptOutcome::error(format!("读取会话消息失败：{error}"))
                }
            };
            // 损坏的 part JSON 跳过并继续，不整屏失败。
            let Ok(value) = serde_json::from_str::<Value>(&part_data) else {
                continue;
            };
            match opencode_part(&value) {
                Some(OpenCodePart::Text(text)) => builder.push_text(role, text),
                Some(OpenCodePart::Tool) => builder.add_tools(1),
                None => {}
            }
        }
    } else {
        let mut stmt = match conn.prepare(
            "SELECT type, data FROM session_message \
             WHERE session_id = ?1 ORDER BY time_created, rowid",
        ) {
            Ok(stmt) => stmt,
            Err(error) => return TranscriptOutcome::error(format!("读取会话消息失败：{error}")),
        };
        let mut rows = match stmt.query([&task.source_id]) {
            Ok(rows) => rows,
            Err(error) => return TranscriptOutcome::error(format!("读取会话消息失败：{error}")),
        };
        while let Some(row) = match rows.next() {
            Ok(row) => row,
            Err(error) => return TranscriptOutcome::error(format!("读取会话消息失败：{error}")),
        } {
            let row_type: Option<String> = match row.get(0) {
                Ok(value) => value,
                Err(error) => {
                    return TranscriptOutcome::error(format!("读取会话消息失败：{error}"))
                }
            };
            let data: String = match row.get(1) {
                Ok(value) => value,
                Err(error) => {
                    return TranscriptOutcome::error(format!("读取会话消息失败：{error}"))
                }
            };
            // 损坏的消息 JSON 跳过并继续，不整屏失败。
            let Ok(value) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            let Some(role) = opencode_role(&value, row_type.as_deref()) else {
                continue;
            };
            for event in legacy_session_message_events(&value, role) {
                match event {
                    TranscriptEvent::Text { role, text } => builder.push_text(role, text),
                    TranscriptEvent::Tools(count) => builder.add_tools(count),
                }
            }
        }
    }

    builder.finish()
}

fn opencode_role(message_data: &Value, fallback_type: Option<&str>) -> Option<TranscriptRole> {
    let raw = message_data
        .get("role")
        .and_then(Value::as_str)
        .or(fallback_type);
    match raw {
        Some("user") => Some(TranscriptRole::User),
        Some("assistant") => Some(TranscriptRole::Assistant),
        _ => None,
    }
}

enum OpenCodePart {
    Text(String),
    Tool,
}

/// part.data 归一：type=text 且非 synthetic 产文本；type=tool 计数；其余跳过。
fn opencode_part(value: &Value) -> Option<OpenCodePart> {
    match value.get("type").and_then(Value::as_str) {
        Some("text") => {
            if value
                .get("synthetic")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return None;
            }
            let raw = value.get("text").and_then(Value::as_str)?;
            if raw.trim().is_empty() {
                None
            } else {
                Some(OpenCodePart::Text(raw.to_owned()))
            }
        }
        Some("tool") => Some(OpenCodePart::Tool),
        _ => None,
    }
}

/// 旧 session_message 行归一：data 内嵌 parts 数组时按序回放（tool 块计数），
/// 否则回退 data.text 单条文本。
fn legacy_session_message_events(value: &Value, role: TranscriptRole) -> Vec<TranscriptEvent> {
    let mut events = Vec::new();
    let mut tool_run = 0usize;
    let mut text_run: Vec<String> = Vec::new();
    if let Some(parts) = value.get("parts").and_then(Value::as_array) {
        for part in parts {
            match opencode_part(part) {
                Some(OpenCodePart::Text(text)) => {
                    if tool_run > 0 {
                        events.push(TranscriptEvent::Tools(tool_run));
                        tool_run = 0;
                    }
                    text_run.push(text);
                }
                Some(OpenCodePart::Tool) => tool_run += 1,
                None => {}
            }
        }
    }
    if let Some(text) = joined_visible_text(&text_run) {
        events.push(TranscriptEvent::Text { role, text });
    } else if events.is_empty() && tool_run == 0 {
        if let Some(raw) = value.get("text").and_then(Value::as_str) {
            if !raw.trim().is_empty() {
                events.push(TranscriptEvent::Text {
                    role,
                    text: raw.to_owned(),
                });
            }
        }
    }
    if tool_run > 0 {
        events.push(TranscriptEvent::Tools(tool_run));
    }
    events
}
