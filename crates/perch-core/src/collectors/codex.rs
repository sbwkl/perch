use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;
use walkdir::WalkDir;

use super::common::{
    fallback_title, is_generated_sandbox_path, is_jsonl, json_string, map_goal_status,
    mtime_seconds, normalize_project_path, now_seconds, open_read_only, parse_timestamp_value,
    project_name, read_first_json_line, shell_cd_command, table_exists, truncate_chars,
};
use super::Collector;
use crate::models::{SourceResult, TaskSnapshot, SOURCE_CODEX};

pub struct CodexCollector {
    root: PathBuf,
}

impl CodexCollector {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn scan_inner(&self) -> crate::Result<SourceResult> {
        let sessions_root = self.root.join("sessions");
        let archived_root = self.root.join("archived_sessions");
        let index_path = self.root.join("session_index.jsonl");
        let goals_path = self.root.join("goals_1.sqlite");

        if !sessions_root.is_dir() && !archived_root.is_dir() && !index_path.is_file() {
            return Ok(SourceResult::unavailable(
                SOURCE_CODEX,
                format!("未找到 Codex 会话数据：{}", self.root.display()),
            ));
        }

        let mut warnings = Vec::new();
        let index = read_index(&index_path, &mut warnings);
        let goals = read_goals(&goals_path, &mut warnings);

        let mut entries: HashMap<String, CodexEntry> = HashMap::new();
        let mut files_seen = 0i64;

        for (root, archived) in [(&sessions_root, false), (&archived_root, true)] {
            if !root.is_dir() {
                continue;
            }
            for entry in WalkDir::new(root).into_iter() {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        warnings.push(format!("遍历 {} 失败：{error}", root.display()));
                        continue;
                    }
                };
                if !entry.file_type().is_file() {
                    continue;
                }
                let path = entry.path();
                if !is_jsonl(path) {
                    continue;
                }
                files_seen += 1;
                match read_rollout_meta(path) {
                    Ok(Some(mut candidate)) => {
                        candidate.archived = archived;
                        let id = candidate.id.clone();
                        let replace = match entries.get(&id) {
                            None => true,
                            Some(existing) => {
                                (existing.archived && !candidate.archived)
                                    || (existing.archived == candidate.archived
                                        && candidate.tail_at.unwrap_or(0.0)
                                            > existing.tail_at.unwrap_or(0.0))
                            }
                        };
                        if replace {
                            entries.insert(id, candidate);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        warnings.push(format!("读取 {} 失败：{error}", path.display()));
                    }
                }
            }
        }

        let mut tasks: Vec<TaskSnapshot> = Vec::new();
        for entry in entries.into_values() {
            let index_entry = index.get(&entry.id);
            let goal = goals.get(&entry.id);

            // Codex Desktop 同样会为没有项目的会话建 ~/Documents/Codex/<日期>/<名称> 沙箱。
            let project_path = entry
                .cwd
                .as_deref()
                .map(normalize_project_path)
                .filter(|value| !value.is_empty())
                .filter(|value| !is_generated_sandbox_path(value, "Codex"));
            let project_name = project_path.as_deref().and_then(project_name);

            let (title, title_source) = if let Some(name) = index_entry
                .and_then(|item| item.name.clone())
                .filter(|value| !value.trim().is_empty())
            {
                (truncate_chars(&name, 240), "source")
            } else if let Some(objective) = goal
                .map(|item| item.objective.trim())
                .filter(|value| !value.is_empty())
            {
                (truncate_chars(objective, 240), "source")
            } else {
                (
                    fallback_title(project_name.as_deref(), entry.tail_at),
                    "fallback",
                )
            };

            let running = recent_running_state(&entry);
            let (status_source, status_raw) = if entry.archived {
                (Some("archived".to_owned()), Some("archived".to_owned()))
            } else if running == Some(true) {
                (Some("running".to_owned()), Some("task_started".to_owned()))
            } else if let Some(goal) = goal {
                let (mapped, raw) = map_goal_status(&goal.status);
                (mapped.or_else(|| Some("idle".to_owned())), raw)
            } else {
                (Some("idle".to_owned()), None)
            };

            let agent_kind = if entry
                .thread_source
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("automation"))
            {
                "automation"
            } else if entry
                .originator
                .as_deref()
                .is_some_and(|value| value.contains("Desktop"))
            {
                "desktop"
            } else if entry
                .originator
                .as_deref()
                .is_some_and(|value| value.contains("tui") || value.contains("exec"))
            {
                "cli"
            } else {
                "unknown"
            };

            let updated_at = index_entry
                .and_then(|item| item.updated_at)
                .or(entry.tail_at);

            let mut task = TaskSnapshot::new(SOURCE_CODEX, &entry.id);
            task.title = title;
            task.title_source = title_source.to_owned();
            task.project_path = project_path;
            task.project_name = project_name;
            task.created_at = entry.tail_at;
            task.updated_at = updated_at;
            task.status_source = status_source;
            task.status_raw = status_raw;
            task.source_archived = entry.archived;
            task.model = entry.model;
            task.agent_kind = Some(agent_kind.to_owned());
            task.resume_cmd = shell_cd_command(
                entry.cwd.as_deref(),
                &format!("codex resume {}", entry.id),
            );
            task.refresh_fingerprint();
            tasks.push(task);
        }

        let mut result = if warnings.is_empty() {
            SourceResult::ok(SOURCE_CODEX, tasks, Vec::new())
        } else {
            SourceResult::partial(SOURCE_CODEX, tasks, Vec::new(), warnings.join("；"))
        };
        result.files_seen = files_seen;
        result.probe_kind = "file_tree";
        Ok(result)
    }
}

impl Collector for CodexCollector {
    fn source(&self) -> &'static str {
        SOURCE_CODEX
    }

    fn scan(&self) -> SourceResult {
        match self.scan_inner() {
            Ok(result) => result,
            Err(error) => SourceResult::error(SOURCE_CODEX, error.to_string()),
        }
    }
}

struct CodexEntry {
    id: String,
    path: PathBuf,
    cwd: Option<String>,
    originator: Option<String>,
    thread_source: Option<String>,
    model: Option<String>,
    tail_at: Option<f64>,
    archived: bool,
}

struct IndexEntry {
    name: Option<String>,
    updated_at: Option<f64>,
}

struct GoalEntry {
    objective: String,
    status: String,
}

fn read_rollout_meta(path: &Path) -> crate::Result<Option<CodexEntry>> {
    let value = match read_first_json_line(path)? {
        Some(value) => value,
        None => return Ok(None),
    };
    if value.get("type").and_then(Value::as_str) != Some("session_meta") {
        return Ok(None);
    }
    let payload = value.get("payload").unwrap_or(&value);
    let id = match json_string(payload, "id").or_else(|| json_string(payload, "session_id")) {
        Some(id) => id,
        None => return Ok(None),
    };
    let timestamp = payload
        .get("timestamp")
        .or_else(|| value.get("timestamp"))
        .and_then(parse_timestamp_value);
    let model = payload
        .pointer("/base_instructions/provenance/model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    Ok(Some(CodexEntry {
        id,
        path: path.to_path_buf(),
        cwd: json_string(payload, "cwd"),
        originator: json_string(payload, "originator"),
        thread_source: json_string(payload, "thread_source"),
        model,
        tail_at: timestamp,
        archived: false,
    }))
}

fn recent_running_state(entry: &CodexEntry) -> Option<bool> {
    let modified = mtime_seconds(&entry.path)?;
    if now_seconds() - modified > 86_400.0 {
        return None;
    }
    read_running_state(&entry.path)
}

fn read_running_state(path: &Path) -> Option<bool> {
    const TAIL_BYTES: u64 = 256 * 1024;
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = length.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).ok()?;
    let text = String::from_utf8_lossy(&buffer);
    let mut lines: Vec<&str> = text.lines().collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    for line in lines.iter().rev() {
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if value.get("type").and_then(Value::as_str) != Some("event_msg") {
            continue;
        }
        let Some(event) = value.pointer("/payload/type").and_then(Value::as_str) else {
            continue;
        };
        match event {
            "task_started" => return Some(true),
            "task_complete" | "turn_aborted" => return Some(false),
            _ => {}
        }
    }
    None
}

fn read_index(path: &Path, warnings: &mut Vec<String>) -> HashMap<String, IndexEntry> {
    let mut index = HashMap::new();
    if !path.is_file() {
        return index;
    }
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => {
            warnings.push(format!("读取 session_index 失败：{error}"));
            return index;
        }
    };
    for line in BufReader::new(file).lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                warnings.push(format!("读取 session_index 行失败：{error}"));
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let id = match json_string(&value, "id") {
            Some(id) => id,
            None => continue,
        };
        let entry = IndexEntry {
            name: json_string(&value, "thread_name"),
            updated_at: value.get("updated_at").and_then(parse_timestamp_value),
        };
        match index.get(&id) {
            Some(existing) if existing.updated_at.unwrap_or(0.0) > entry.updated_at.unwrap_or(0.0) => {}
            _ => {
                index.insert(id, entry);
            }
        }
    }
    index
}

fn read_goals(path: &Path, warnings: &mut Vec<String>) -> HashMap<String, GoalEntry> {
    let mut goals = HashMap::new();
    if !path.is_file() {
        return goals;
    }
    let conn = match open_read_only(path) {
        Ok(conn) => conn,
        Err(error) => {
            warnings.push(format!("打开 goals_1.sqlite 失败：{error}"));
            return goals;
        }
    };
    match table_exists(&conn, "thread_goals") {
        Ok(true) => {}
        Ok(false) => return goals,
        Err(error) => {
            warnings.push(format!("检查 thread_goals 失败：{error}"));
            return goals;
        }
    }
    let mut statement = match conn.prepare("SELECT thread_id, objective, status FROM thread_goals") {
        Ok(statement) => statement,
        Err(error) => {
            warnings.push(format!("查询 thread_goals 失败：{error}"));
            return goals;
        }
    };
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    });
    match rows {
        Ok(rows) => {
            for row in rows {
                match row {
                    Ok((thread_id, objective, status)) => {
                        goals.insert(thread_id, GoalEntry { objective, status });
                    }
                    Err(error) => warnings.push(format!("读取 thread_goals 行失败：{error}")),
                }
            }
        }
        Err(error) => warnings.push(format!("读取 thread_goals 失败：{error}")),
    }
    goals
}
