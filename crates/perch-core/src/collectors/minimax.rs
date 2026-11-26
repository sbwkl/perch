use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use walkdir::WalkDir;

use super::common::{fallback_title, open_read_only, project_name, truncate_chars};
use super::Collector;
use crate::models::{SourceResult, TaskSnapshot, SOURCE_MINIMAX};

const MESSAGES_NAME: &str = "messages.jsonl";
const MANIFEST_NAME: &str = "manifest.json";

/// MiniMax Code：会话目录按日期分层（`~/.minimax/v2/sessions/YYYY/MM/DD/
/// <dir>/messages.jsonl`）。会话 ID 以 manifest.json 的 sessionId 为权威，
/// 缺失时回退目录名尾段 base64url 解码；项目名来自 runtime-state.sqlite
/// 的 session → workspace 映射，取不到时降级为无项目。
pub struct MiniMaxCollector {
    root: PathBuf,
}

impl MiniMaxCollector {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn sessions_root(&self) -> PathBuf {
        self.root.join("v2/sessions")
    }

    fn runtime_db(&self) -> PathBuf {
        self.root.join("v2/sqlite/runtime-state.sqlite")
    }

    fn scan_inner(&self) -> crate::Result<SourceResult> {
        let sessions_root = self.sessions_root();
        if !sessions_root.is_dir() {
            return Ok(SourceResult::unavailable(
                SOURCE_MINIMAX,
                format!("未找到 MiniMax 会话目录：{}", sessions_root.display()),
            ));
        }

        let projects = read_projects(&self.runtime_db());
        let mut warnings = Vec::new();
        let mut files = Vec::new();
        for entry in WalkDir::new(&sessions_root)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_type().is_file()
                && entry.path().file_name().and_then(|name| name.to_str()) == Some(MESSAGES_NAME)
            {
                files.push(entry.path().to_path_buf());
            }
        }
        files.sort();

        let mut tasks: Vec<TaskSnapshot> = Vec::new();
        for path in &files {
            let session_dir = path.parent().unwrap_or(path);
            let session_id = session_identity(session_dir, &mut warnings);
            let mut entry = parse_messages(path, &mut warnings);
            if let Some(project_path) = projects.get(&session_id) {
                entry.project_path = Some(project_path.clone());
            }
            let mut task = TaskSnapshot::new(SOURCE_MINIMAX, &session_id);
            // 可重建的源文件定位指针：对话正文按需读取时据此定位 messages.jsonl。
            task.source_path = Some(path.to_string_lossy().into_owned());
            task.title = match entry
                .title
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
            {
                Some(title) => {
                    task.title_source = "source".to_owned();
                    truncate_chars(title, 240)
                }
                None => {
                    task.title_source = "fallback".to_owned();
                    fallback_title()
                }
            };
            task.project_path = entry.project_path.clone();
            task.project_name = entry.project_path.as_deref().and_then(project_name);
            task.created_at = entry.created_at;
            task.updated_at = entry.updated_at;
            task.model = entry.model;
            task.status_source = Some("idle".to_owned());
            task.agent_kind = Some("cli".to_owned());
            task.refresh_fingerprint();
            tasks.push(task);
        }

        let mut result = if warnings.is_empty() {
            SourceResult::ok(SOURCE_MINIMAX, tasks, Vec::new())
        } else {
            SourceResult::partial(SOURCE_MINIMAX, tasks, Vec::new(), warnings.join("；"))
        };
        result.files_seen = files.len() as i64;
        result.probe_kind = "file_tree";
        Ok(result)
    }
}

impl Collector for MiniMaxCollector {
    fn source(&self) -> &'static str {
        SOURCE_MINIMAX
    }

    fn scan(&self) -> SourceResult {
        match self.scan_inner() {
            Ok(result) => result,
            Err(error) => SourceResult::error(SOURCE_MINIMAX, error.to_string()),
        }
    }
}

struct SessionEntry {
    title: Option<String>,
    project_path: Option<String>,
    created_at: Option<f64>,
    updated_at: Option<f64>,
    model: Option<String>,
}

fn session_identity(session_dir: &Path, warnings: &mut Vec<String>) -> String {
    let manifest_path = session_dir.join(MANIFEST_NAME);
    if let Ok(text) = std::fs::read_to_string(&manifest_path) {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            if let Some(id) = value.get("sessionId").and_then(Value::as_str) {
                let id = id.trim();
                if !id.is_empty() {
                    return id.to_owned();
                }
            }
        }
    }
    let name = session_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let marker = "-session_";
    if let Some((_, tail)) = name.rsplit_once(marker) {
        if !tail.is_empty() && tail.bytes().all(is_base64url_byte) {
            match urlsafe_b64_decode(tail) {
                Some(decoded) if decoded.chars().all(|ch| !ch.is_control()) => {
                    return decoded;
                }
                _ => {}
            }
        }
    }
    warnings.push(format!("manifest 缺失，会话 ID 回退目录名：{}", name));
    name.to_owned()
}

fn is_base64url_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

pub(crate) fn urlsafe_b64_decode(input: &str) -> Option<String> {
    let mut buffer = 0u32;
    let mut bits = 0u32;
    let mut output = Vec::new();
    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    String::from_utf8(output).ok()
}

/// session_id → workspace 完整路径（workspace_dir 优先，回退
/// project_workspace_dir）；project_name 由调用方从路径推导。
fn read_projects(db_path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if !db_path.is_file() {
        return map;
    }
    let Ok(conn) = open_read_only(db_path) else {
        return map;
    };
    let Ok(mut statement) = conn.prepare(
        "SELECT session_id, workspace_dir, project_workspace_dir FROM local_runtime_sessions",
    ) else {
        return map;
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    }) else {
        return map;
    };
    for row in rows.flatten() {
        let (session_id, workspace, project_workspace) = row;
        let Some(session_id) = session_id.filter(|value| !value.is_empty()) else {
            continue;
        };
        let workspace = workspace
            .map(|value| value.trim().trim_end_matches('/').to_owned())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                project_workspace
                    .map(|value| value.trim().trim_end_matches('/').to_owned())
                    .filter(|value| !value.is_empty())
            });
        if let Some(path) = workspace {
            map.insert(session_id, path);
        }
    }
    map
}

fn parse_messages(path: &Path, warnings: &mut Vec<String>) -> SessionEntry {
    let mut entry = SessionEntry {
        title: None,
        project_path: None,
        created_at: None,
        updated_at: None,
        model: None,
    };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            warnings.push(format!("读取 {} 失败：{error}", path.display()));
            return entry;
        }
    };
    let manifest_ts = manifest_timestamps(path.parent().unwrap_or(path));
    entry.created_at = manifest_ts.0;
    entry.updated_at = manifest_ts.1;
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(message) = value.get("message").filter(|m| m.is_object()) else {
            continue;
        };
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match role {
            "user" => {
                if entry.title.is_none() {
                    entry.title = user_message_title(message);
                }
            }
            "assistant" => {
                if let Some(model) = message.get("model").and_then(Value::as_str) {
                    entry.model = Some(model.to_owned());
                }
            }
            _ => {}
        }
    }
    entry
}

/// manifest 的 createdAtMs/updatedAtMs 是会话时间权威；缺失时回退 None。
fn manifest_timestamps(session_dir: &Path) -> (Option<f64>, Option<f64>) {
    let manifest_path = session_dir.join(MANIFEST_NAME);
    let Ok(text) = std::fs::read_to_string(&manifest_path) else {
        return (None, None);
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return (None, None);
    };
    let to_seconds = |key: &str| {
        value.get(key).and_then(Value::as_f64).map(|ms| {
            if ms > 10_000_000_000.0 {
                ms / 1000.0
            } else {
                ms
            }
        })
    };
    (to_seconds("createdAtMs"), to_seconds("updatedAtMs"))
}

fn user_message_title(message: &Value) -> Option<String> {
    let content = message.get("content")?;
    let text = match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => parts.iter().find_map(|part| {
            let type_name = part.get("type").and_then(Value::as_str)?;
            if type_name != "text" {
                return None;
            }
            part.get("text")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        }),
        _ => None,
    }?;
    let cleaned = strip_reminder_blocks(&text);
    cleaned
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| truncate_chars(line, 240))
}

/// 用户消息常被 <system-reminder>/<media-output-reminder>/<async-audit>
/// 等块包裹；剥掉这些块后再取首行，避免标题全是注入的提示文本。
/// 标题派生与对话正文读取共用同一清洗口径。
pub(crate) fn strip_reminder_blocks(text: &str) -> String {
    const TAGS: [&str; 4] = [
        "system-reminder",
        "media-output-reminder",
        "async-audit",
        "agent-context",
    ];
    let mut output = text.to_owned();
    for tag in TAGS {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        while let Some(start) = output.find(&open) {
            let Some(relative) = output[start..].find(&close) else {
                break;
            };
            let end = start + relative + close.len();
            output.replace_range(start..end, "");
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_decode_matches_std_alphabet() {
        assert_eq!(
            urlsafe_b64_decode("bXZzXzMxMmI2YjU1ZTljMTQzOTQ5YjEzY2JkZmE0YTUwZjQ0").as_deref(),
            Some("mvs_312b6b55e9c143949b13cbdfa4a50f44")
        );
        assert_eq!(urlsafe_b64_decode("!!!"), None);
    }

    #[test]
    fn reminder_blocks_are_stripped() {
        let text = "<system-reminder>\nhidden\n</system-reminder>\n真实的用户输入\n";
        let cleaned = strip_reminder_blocks(text);
        assert_eq!(cleaned.trim(), "真实的用户输入");
    }

    #[test]
    fn unclosed_block_does_not_loop() {
        let text = "<system-reminder>\nnever closed";
        let cleaned = strip_reminder_blocks(text);
        assert_eq!(cleaned, text);
    }
}
