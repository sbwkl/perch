use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::Value;

use crate::collectors::claude::strip_injected_wrappers;
use crate::collectors::common::{open_read_only, table_exists};
use crate::models::{
    TaskSnapshot, TranscriptMessage, TranscriptOutcome, TranscriptRole, SOURCE_CLAUDE,
    SOURCE_CODEARTS, SOURCE_KILO, SOURCE_OPENCODE, SOURCE_QODER,
};

/// 单一公共读取入口：输入任务快照，输出对话记录（角色、文本、是否截断）或错误。
/// 后续来源通过在本模块增加读取器接入同一入口；读取是内存内单次快照，
/// 不缓存正文、不做后台重读，源文件一律只读打开。
pub fn read_transcript(task: &TaskSnapshot) -> TranscriptOutcome {
    match task.source.as_str() {
        SOURCE_CLAUDE => read_claude_transcript(task),
        SOURCE_QODER => read_qoder_transcript(task),
        SOURCE_OPENCODE | SOURCE_KILO | SOURCE_CODEARTS => read_opencode_transcript(task),
        other if crate::models::transcript_supported(other) => {
            TranscriptOutcome::error("该来源的对话正文读取尚未接入")
        }
        _ => TranscriptOutcome::error("来源不支持读取对话正文"),
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

    let mut fold = FoldState::new();

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
            fold.push(event);
        }
    }

    fold.finish()
}

/// 事件折叠状态（Qoder 与 OpenCode 系共用）：纯工具轮累积成「工具调用 N 次」
/// 计数行；单条按 8,000 字符截断，会话超出 200,000 只保留最近部分。
struct FoldState {
    messages: VecDeque<TranscriptMessage>,
    session_chars: usize,
    session_truncated: bool,
    pending_tools: usize,
}

impl FoldState {
    fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            session_chars: 0,
            session_truncated: false,
            pending_tools: 0,
        }
    }

    /// 文本事件：先冲刷累积的工具计数再落文本；行内后置工具计入下一轮。
    fn push_text(&mut self, role: TranscriptRole, text: String) {
        if self.pending_tools > 0 {
            let count = self.pending_tools;
            self.pending_tools = 0;
            self.push_message(tool_call_message(count));
        }
        let (text, truncated) = clip_message_text(text);
        if !text.is_empty() {
            self.push_message(TranscriptMessage {
                role,
                text,
                truncated,
            });
        }
    }

    fn push_message(&mut self, message: TranscriptMessage) {
        push_message(
            &mut self.messages,
            &mut self.session_chars,
            &mut self.session_truncated,
            message,
        );
    }

    fn add_tools(&mut self, count: usize) {
        self.pending_tools += count;
    }

    fn push(&mut self, event: TranscriptEvent) {
        match event {
            TranscriptEvent::Text { role, text } => self.push_text(role, text),
            TranscriptEvent::Tools(count) => self.pending_tools += count,
        }
    }

    /// 收尾：冲刷剩余工具计数，空会话给出错误；否则产出正序消息。
    fn finish(mut self) -> TranscriptOutcome {
        if self.pending_tools > 0 {
            let count = self.pending_tools;
            self.pending_tools = 0;
            self.push_message(tool_call_message(count));
        }
        if self.messages.is_empty() {
            return TranscriptOutcome::error("会话没有可显示的对话内容");
        }
        TranscriptOutcome::Ready {
            messages: self.messages.into_iter().collect(),
            session_truncated: self.session_truncated,
        }
    }
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

    let mut fold = FoldState::new();
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
                Some(OpenCodePart::Text(text)) => fold.push_text(role, text),
                Some(OpenCodePart::Tool) => fold.add_tools(1),
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
                    TranscriptEvent::Text { role, text } => fold.push_text(role, text),
                    TranscriptEvent::Tools(count) => fold.add_tools(count),
                }
            }
        }
    }

    fold.finish()
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
