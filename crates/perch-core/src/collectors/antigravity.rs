use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use walkdir::WalkDir;

use super::common::{fallback_title, parse_timestamp_str, project_name, truncate_chars};
use super::Collector;
use crate::models::{SourceResult, TaskSnapshot, SOURCE_ANTIGRAVITY};

const TRANSCRIPT_NAME: &str = "transcript_full.jsonl";
const SUMMARIES_DB: &str = "conversation_summaries.db";
const DEFAULT_PROJECT: &str = "default-cli-project";

/// Antigravity：IDE 与独立 agy CLI 各写一个 app data dir，格式一致，
/// 彼此不复用会话。会话索引来自 `brain/<conv>/.system_generated/logs/`
/// 下的 transcript_full.jsonl；标题/项目来自 conversation_summaries.db。
/// transcript 属于消息正文：为满足 Perch 的只读元数据边界，只有 summaries
/// 没有标题时才做一次流式元数据探测，并把可从 summaries 恢复标题的会话数
/// 记录在 `schema_version = summary-titles:<n>`，便于实机审计。
pub struct AntigravityCollector {
    ide_root: PathBuf,
    cli_root: PathBuf,
}

impl AntigravityCollector {
    pub fn new(ide_root: PathBuf, cli_root: PathBuf) -> Self {
        Self { ide_root, cli_root }
    }

    fn scan_inner(&self) -> crate::Result<SourceResult> {
        let roots = [(&self.ide_root, "ide"), (&self.cli_root, "cli")];
        let mut warnings = Vec::new();
        let mut transcripts = Vec::new();
        for (root, agent) in &roots {
            if !root.is_dir() {
                warnings.push(format!("{} 目录不存在：{}", agent, root.display()));
                continue;
            }
            let brain = root.join("brain");
            for entry in WalkDir::new(&brain).into_iter().filter_map(Result::ok) {
                if !entry.file_type().is_file() {
                    continue;
                }
                let path = entry.path();
                if path.file_name().and_then(|name| name.to_str()) == Some(TRANSCRIPT_NAME) {
                    transcripts.push((path.to_path_buf(), *agent));
                }
            }
        }
        if transcripts.is_empty() {
            return Ok(SourceResult::unavailable(
                SOURCE_ANTIGRAVITY,
                warnings.join("；"),
            ));
        }
        transcripts.sort_by(|a, b| a.0.cmp(&b.0));

        let mut summaries: HashMap<PathBuf, HashMap<String, SummaryEntry>> = HashMap::new();
        let mut sessions: HashMap<String, SessionEntry> = HashMap::new();
        let mut files_seen = 0i64;
        let mut summary_titles = 0i64;

        for (path, agent) in &transcripts {
            files_seen += 1;
            let Some(root) = root_of(path, &self.ide_root, &self.cli_root) else {
                continue;
            };
            let summary = summaries
                .entry(root.clone())
                .or_insert_with(|| read_summaries(&root.join(SUMMARIES_DB)));
            let Some(conv) = conversation_id(path) else {
                continue;
            };
            let mut entry = parse_transcript(path, &mut warnings);
            entry.agent_kind = (*agent).to_owned();
            entry.path = Some(path.to_path_buf());
            if let Some(item) = summary.get(&conv) {
                // summaries 的标题/项目是权威来源，覆盖转录推导值。
                if item.title.is_some() {
                    summary_titles += 1;
                    entry.title = item.title.clone();
                }
                if let Some(project) = item.project.clone() {
                    entry.project = Some(project);
                }
                if entry.project.is_none() {
                    entry.project = Some(DEFAULT_PROJECT.to_owned());
                }
            } else {
                entry.project = Some(DEFAULT_PROJECT.to_owned());
            }
            let entry = entry;
            match sessions.get_mut(&conv) {
                Some(existing) => {
                    if entry.updated_at.unwrap_or(0.0) >= existing.updated_at.unwrap_or(0.0) {
                        sessions.insert(conv.clone(), entry);
                    }
                }
                None => {
                    sessions.insert(conv.clone(), entry);
                }
            }
        }

        let mut tasks: Vec<TaskSnapshot> = Vec::new();
        for (conv, entry) in sessions {
            let mut task = TaskSnapshot::new(SOURCE_ANTIGRAVITY, &conv);
            // 可重建的源文件定位指针：对话正文按需读取时据此定位转录。
            task.source_path = entry
                .path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned());
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
            task.project_name = entry.project.clone();
            task.project_path = entry.project_path;
            task.created_at = entry.created_at;
            task.updated_at = entry.updated_at;
            task.model = entry.model;
            task.agent_kind = Some(entry.agent_kind);
            task.status_source = Some("idle".to_owned());
            task.refresh_fingerprint();
            tasks.push(task);
        }

        let mut result = if warnings.is_empty() {
            SourceResult::ok(SOURCE_ANTIGRAVITY, tasks, Vec::new())
        } else {
            SourceResult::partial(SOURCE_ANTIGRAVITY, tasks, Vec::new(), warnings.join("；"))
        };
        result.files_seen = files_seen;
        result.schema_version = Some(format!("summary-titles:{summary_titles}"));
        result.probe_kind = "file_tree";
        Ok(result)
    }
}

impl Collector for AntigravityCollector {
    fn source(&self) -> &'static str {
        SOURCE_ANTIGRAVITY
    }

    fn scan(&self) -> SourceResult {
        match self.scan_inner() {
            Ok(result) => result,
            Err(error) => SourceResult::error(SOURCE_ANTIGRAVITY, error.to_string()),
        }
    }
}

fn root_of(path: &Path, ide_root: &Path, cli_root: &Path) -> Option<PathBuf> {
    for root in [ide_root, cli_root] {
        if path.starts_with(root) {
            return Some(root.to_path_buf());
        }
    }
    None
}

/// .../brain/<conv>/.system_generated/logs/transcript_full.jsonl → <conv>
fn conversation_id(path: &Path) -> Option<String> {
    path.ancestors()
        .nth(3)
        .and_then(|dir| dir.file_name())
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
}

struct SummaryEntry {
    title: Option<String>,
    project: Option<String>,
}

fn read_summaries(db_path: &Path) -> HashMap<String, SummaryEntry> {
    let mut map = HashMap::new();
    let Ok(conn) = super::common::open_read_only(db_path) else {
        return map;
    };
    let Ok(mut statement) =
        conn.prepare("SELECT conversation_id, title, workspace_uris FROM conversation_summaries")
    else {
        return map;
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    }) else {
        return map;
    };
    for row in rows.flatten() {
        let (conv, title, workspace_uris) = row;
        let title = title.trim().to_owned();
        let project = workspace_project(&workspace_uris);
        map.insert(
            conv,
            SummaryEntry {
                title: if title.is_empty() { None } else { Some(title) },
                project,
            },
        );
    }
    map
}

fn workspace_project(raw: &str) -> Option<String> {
    let uris: Vec<Value> = serde_json::from_str(raw).ok()?;
    for uri in uris {
        let Some(uri) = uri.as_str() else {
            continue;
        };
        let path = uri.split_once("://").map(|(_, rest)| rest).unwrap_or(uri);
        let path = path.trim_end_matches('/');
        if path.is_empty() {
            continue;
        }
        if let Some(name) = project_name(path) {
            return Some(name);
        }
    }
    None
}

struct SessionEntry {
    title: Option<String>,
    project: Option<String>,
    project_path: Option<String>,
    created_at: Option<f64>,
    updated_at: Option<f64>,
    model: Option<String>,
    agent_kind: String,
    /// 去重后胜出的转录文件路径，作为正文读取的源文件指针。
    path: Option<PathBuf>,
}

fn parse_transcript(path: &Path, warnings: &mut Vec<String>) -> SessionEntry {
    let mut entry = SessionEntry {
        title: None,
        project: None,
        project_path: None,
        created_at: None,
        updated_at: None,
        model: None,
        agent_kind: "unknown".to_owned(),
        path: None,
    };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            warnings.push(format!("读取 {} 失败：{error}", path.display()));
            return entry;
        }
    };
    let (steps, fragments) = read_json_stream(&text);
    // 游离片段是已知的写入中断产物，能被重新同步跳过，不算降级。
    let _ = fragments;
    for step in &steps {
        if step.get("source").and_then(Value::as_str) == Some("USER_EXPLICIT")
            && entry.title.is_none()
        {
            entry.title = user_input_title(step);
        }
        if step.get("source").and_then(Value::as_str) == Some("MODEL") {
            if let Some(model) = step.get("model").and_then(Value::as_str) {
                entry.model = Some(model.to_owned());
            }
        }
        if let Some(ts) = step
            .get("created_at")
            .and_then(Value::as_str)
            .and_then(parse_timestamp_str)
        {
            entry.created_at = Some(entry.created_at.map_or(ts, |value| value.min(ts)));
            entry.updated_at = Some(entry.updated_at.map_or(ts, |value| value.max(ts)));
        }
    }
    if entry.created_at.is_none() {
        // 转录时间不可用时回退文件 mtime，保证排序仍有依据。
        entry.updated_at = super::common::mtime_seconds(path);
        entry.created_at = entry.updated_at;
    }
    entry
}

fn user_input_title(step: &Value) -> Option<String> {
    let content = step.get("content").and_then(Value::as_str)?;
    let inner = content
        .find("<USER_REQUEST>")
        .zip(content.find("</USER_REQUEST>"))
        .filter(|(start, end)| end > start)
        .map(|(start, end)| &content[start + "<USER_REQUEST>".len()..end])
        .unwrap_or(content);
    let line = inner.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(truncate_chars(line, 240))
}

/// 转录不是严格 JSONL：写入中断会留下独立成行的片段（形如 `}]}`），
/// 这里按 JSON 流扫描，失败时从下一个 `{` 重新同步。Antigravity 的
/// `USER_EXPLICIT.content` 是用户 prompt，只读标题所需行，不保留正文。
fn read_json_stream(text: &str) -> (Vec<Value>, usize) {
    let mut items = Vec::new();
    let mut fragments = 0usize;
    let mut offset = 0usize;
    while offset < text.len() {
        offset += text[offset..]
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .map(char::len_utf8)
            .sum::<usize>();
        if offset >= text.len() {
            break;
        }
        let mut stream = serde_json::Deserializer::from_str(&text[offset..]).into_iter::<Value>();
        match stream.next() {
            Some(Ok(value)) => {
                offset += stream.byte_offset();
                items.push(value);
            }
            Some(Err(_)) => {
                fragments += 1;
                match text[offset..].find('{') {
                    // 重新同步到下一个 `{`（含），让下一次解析从对象开头开始。
                    Some(relative) => offset += relative,
                    None => break,
                }
            }
            None => break,
        }
    }
    (items, fragments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_parser_resyncs_after_fragments() {
        let text = "{\"a\":1}\n}]}\n{\"a\":2}\nnot json at all {\"a\":3}";
        let (items, fragments) = read_json_stream(text);
        assert_eq!(items.len(), 3);
        assert_eq!(fragments, 2);
    }

    #[test]
    fn conversation_id_walks_three_levels_up() {
        let path = Path::new("/tmp/root/brain/conv-1/.system_generated/logs/transcript_full.jsonl");
        assert_eq!(conversation_id(path).as_deref(), Some("conv-1"));
    }
}
