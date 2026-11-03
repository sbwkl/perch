use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;
use walkdir::WalkDir;

use super::common::{
    fallback_title, is_jsonl, json_string, mtime_seconds, normalize_project_path,
    parse_timestamp_value, project_name, shell_cd_command, truncate_chars,
};
use super::Collector;
use crate::models::{SourceResult, TaskSnapshot, TodoSnapshot, SOURCE_CLAUDE};

pub struct ClaudeCollector {
    root: PathBuf,
}

impl ClaudeCollector {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn scan_inner(&self) -> crate::Result<SourceResult> {
        let projects_root = self.root.join("projects");
        if !projects_root.is_dir() {
            return Ok(SourceResult::unavailable(
                SOURCE_CLAUDE,
                format!("未找到 Claude Code 会话目录：{}", projects_root.display()),
            ));
        }

        let mut session_files = Vec::new();
        let mut files_seen = 0i64;
        let mut warnings = Vec::new();

        for entry in WalkDir::new(&projects_root).into_iter() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    warnings.push(format!("遍历会话目录失败：{error}"));
                    continue;
                }
            };
            if !entry.file_type().is_file() || !is_jsonl(entry.path()) {
                continue;
            }
            files_seen += 1;
            let relative = match entry.path().strip_prefix(&projects_root) {
                Ok(relative) => relative,
                Err(_) => continue,
            };
            let components: Vec<String> = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect();
            if components.iter().any(|component| component == "subagents") {
                continue;
            }
            if components.len() == 2 {
                session_files.push(entry.path().to_path_buf());
            }
        }

        let mut tasks = Vec::new();
        let mut todos = Vec::new();
        for path in session_files {
            match self.parse_session(&path) {
                Ok(Some(parsed)) => {
                    let mut task = parsed.task;
                    let session_id = task.source_id.clone();
                    match self.load_todos(&session_id, &task.key) {
                        Ok(mut task_todos) => {
                            task.todo_total = Some(task_todos.len() as i64);
                            task.todo_done = Some(
                                task_todos
                                    .iter()
                                    .filter(|todo| todo.status == "completed")
                                    .count() as i64,
                            );
                            task.refresh_fingerprint();
                            todos.append(&mut task_todos);
                        }
                        Err(error) => {
                            warnings.push(format!("读取 {} 的 todo 失败：{error}", path.display()))
                        }
                    }
                    tasks.push(task);
                }
                Ok(None) => {}
                Err(error) => warnings.push(format!("解析 {} 失败：{error}", path.display())),
            }
        }

        let mut result = if warnings.is_empty() {
            SourceResult::ok(SOURCE_CLAUDE, tasks, todos)
        } else {
            SourceResult::partial(SOURCE_CLAUDE, tasks, todos, warnings.join("；"))
        };
        result.files_seen = files_seen;
        result.probe_kind = "file_tree";
        Ok(result)
    }

    fn parse_session(&self, path: &Path) -> crate::Result<Option<ParsedSession>> {
        let session_id = match path.file_stem().and_then(|stem| stem.to_str()) {
            Some(stem) if !stem.is_empty() => stem.to_owned(),
            _ => return Ok(None),
        };
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut cwd = None;
        let mut git_branch = None;
        let mut model = None;
        let mut custom_title = None;
        let mut ai_title = None;
        let mut first_ts: Option<f64> = None;
        let mut last_ts: Option<f64> = None;
        let mut message_count = 0i64;
        let mut parsed_records = 0i64;

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let value: Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(_) => continue,
            };
            parsed_records += 1;

            if let Some(found) = json_string(&value, "cwd") {
                cwd = Some(found);
            }
            if let Some(found) = json_string(&value, "gitBranch") {
                git_branch = Some(found);
            }
            if let Some(timestamp) = value.get("timestamp").and_then(parse_timestamp_value) {
                first_ts.get_or_insert(timestamp);
                if last_ts.is_none_or(|current| timestamp > current) {
                    last_ts = Some(timestamp);
                }
            }

            match value.get("type").and_then(Value::as_str).unwrap_or("") {
                "custom-title" => {
                    if let Some(title) = json_string(&value, "customTitle") {
                        custom_title = Some(title);
                    }
                }
                "ai-title" => {
                    if let Some(title) = json_string(&value, "aiTitle") {
                        ai_title = Some(title);
                    }
                }
                "user" => {
                    if !value
                        .get("isSidechain")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        message_count += 1;
                    }
                }
                "assistant" => {
                    if !value
                        .get("isSidechain")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        message_count += 1;
                    }
                    if let Some(found) = value
                        .pointer("/message/model")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    {
                        model = Some(found.to_owned());
                    }
                }
                _ => {}
            }
        }

        if parsed_records == 0 {
            return Ok(None);
        }

        let file_mtime = mtime_seconds(path);
        let created_at = first_ts.or(file_mtime);
        let updated_at = last_ts.or(file_mtime);
        let project_path = cwd
            .as_deref()
            .map(normalize_project_path)
            .filter(|value| !value.is_empty());
        let project_name = project_path.as_deref().and_then(project_name);

        let (title, title_source) = if let Some(title) = custom_title {
            (title, "custom")
        } else if let Some(title) = ai_title {
            (title, "ai")
        } else {
            (
                fallback_title(project_name.as_deref(), created_at),
                "fallback",
            )
        };

        let mut task = TaskSnapshot::new(SOURCE_CLAUDE, &session_id);
        task.title = truncate_chars(&title, 240);
        task.title_source = title_source.to_owned();
        task.project_path = project_path.clone();
        task.project_name = project_name;
        task.created_at = created_at;
        task.updated_at = updated_at;
        task.status_source = Some("idle".to_owned());
        task.model = model;
        task.git_branch = git_branch;
        task.agent_kind = Some("cli".to_owned());
        task.message_count = Some(message_count);
        task.resume_cmd =
            shell_cd_command(cwd.as_deref(), &format!("claude --resume {session_id}"));
        task.refresh_fingerprint();

        Ok(Some(ParsedSession { task }))
    }

    fn load_todos(&self, session_id: &str, task_key: &str) -> crate::Result<Vec<TodoSnapshot>> {
        let todo_dir = self.root.join("tasks").join(session_id);
        if !todo_dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(&todo_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) == Some("json") {
                files.push(path);
            }
        }
        files.sort();

        let mut todos = Vec::new();
        for (position, path) in files.into_iter().enumerate() {
            let raw = fs::read_to_string(&path)?;
            let value: Value = match serde_json::from_str(&raw) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let subject = match json_string(&value, "subject") {
                Some(subject) => subject,
                None => continue,
            };
            let item_id = json_string(&value, "id").unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("unknown")
                    .to_owned()
            });
            let status = normalize_todo_status(json_string(&value, "status").as_deref());
            todos.push(TodoSnapshot {
                task_key: task_key.to_owned(),
                item_id,
                subject: truncate_chars(&subject, 240),
                status,
                position: position as i64,
            });
        }
        Ok(todos)
    }
}

impl Collector for ClaudeCollector {
    fn source(&self) -> &'static str {
        SOURCE_CLAUDE
    }

    fn scan(&self) -> SourceResult {
        match self.scan_inner() {
            Ok(result) => result,
            Err(error) => SourceResult::error(SOURCE_CLAUDE, error.to_string()),
        }
    }
}

struct ParsedSession {
    task: TaskSnapshot,
}

fn normalize_todo_status(raw: Option<&str>) -> String {
    match raw.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "pending" => "pending",
        "in_progress" | "in-progress" | "doing" => "in_progress",
        "completed" | "complete" | "done" => "completed",
        "cancelled" | "canceled" => "cancelled",
        _ => "unknown",
    }
    .to_owned()
}
