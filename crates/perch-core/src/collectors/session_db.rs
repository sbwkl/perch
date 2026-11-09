use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::Value;

use super::common::{
    epoch_ms_to_seconds, fallback_title, is_default_project_path, normalize_project_path,
    open_read_only, project_name, table_exists, truncate_chars,
};
use crate::models::{SourceResult, TaskSnapshot, TodoSnapshot};

pub(crate) type ResumeBuilder = fn(Option<&str>, &str) -> Option<String>;

pub(crate) fn scan_session_db(
    source: &str,
    db_path: &Path,
    resume_builder: ResumeBuilder,
) -> SourceResult {
    if !db_path.is_file() {
        return SourceResult::unavailable(source, format!("未找到数据库：{}", db_path.display()));
    }
    match scan_inner(source, db_path, resume_builder) {
        Ok(result) => result,
        Err(error) => SourceResult::error(source, error.to_string()),
    }
}

fn scan_inner(
    source: &str,
    db_path: &Path,
    resume_builder: ResumeBuilder,
) -> crate::Result<SourceResult> {
    let conn = open_read_only(db_path)?;
    // 新版 OpenCode 迁移到 session_v2（旧 session 表不再更新），Kilo 仍用 session。
    let has_v2 = table_exists(&conn, "session_v2")?;
    if !has_v2 && !table_exists(&conn, "session")? {
        return Err(crate::PerchError::message(format!(
            "{} 中缺少 session/session_v2 表",
            db_path.display()
        )));
    }

    let has_todo = table_exists(&conn, "todo")?;

    let query = if has_v2 {
        "SELECT id, parent_id, title, directory, model, time_created, time_updated, time_archived, \
                time_busy, time_idle FROM session_v2"
    } else {
        "SELECT id, parent_id, title, directory, model, time_created, time_updated, time_archived, \
                NULL, NULL FROM session"
    };
    let mut stmt = conn.prepare(query)?;
    let rows = stmt.query_map([], |row| {
        Ok(SessionRow {
            id: row.get(0)?,
            parent_id: row.get(1)?,
            title: row.get(2)?,
            directory: row.get(3)?,
            model: row.get(4)?,
            time_created: row.get(5)?,
            time_updated: row.get(6)?,
            time_archived: row.get(7)?,
            time_busy: row.get(8)?,
            time_idle: row.get(9)?,
        })
    })?;

    let mut known_sessions = HashSet::new();
    let mut tasks = Vec::new();
    for row in rows {
        let row = row?;
        known_sessions.insert(row.id.clone());
        tasks.push(build_task(source, row, resume_builder));
    }

    let todos = if has_todo {
        load_todos(&conn, source, &known_sessions)?
    } else {
        Vec::new()
    };

    let mut todos_by_task: HashMap<String, (i64, i64)> = HashMap::new();
    for todo in &todos {
        let entry = todos_by_task.entry(todo.task_key.clone()).or_insert((0, 0));
        entry.0 += 1;
        if todo.status == "completed" {
            entry.1 += 1;
        }
    }
    for task in &mut tasks {
        if let Some((total, done)) = todos_by_task.get(&task.key) {
            task.todo_total = Some(*total);
            task.todo_done = Some(*done);
        }
    }

    let mut result = SourceResult::ok(source, tasks, todos);
    result.files_seen = 1;
    result.probe_kind = "sqlite";
    Ok(result)
}

fn build_task(source: &str, row: SessionRow, resume_builder: ResumeBuilder) -> TaskSnapshot {
    // OpenCode 会用空的 ~/Documents/Default Project 作为未选项目时的占位目录。
    let project_path = row
        .directory
        .as_deref()
        .map(normalize_project_path)
        .filter(|value| !value.is_empty())
        .filter(|value| !is_default_project_path(value));
    let project_name = project_path.as_deref().and_then(project_name);
    let created_at = epoch_ms_to_seconds(row.time_created);
    let updated_at = epoch_ms_to_seconds(row.time_updated).or(created_at);
    let archived = row.time_archived.is_some_and(|value| value > 0);
    let busy = row.time_busy.unwrap_or(0);
    let idle = row.time_idle.unwrap_or(0);
    let running = !archived && busy > 0 && busy > idle;

    let raw_title = row
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let (title, title_source) = match raw_title {
        Some(title) => (title.to_owned(), "source"),
        None => (fallback_title(), "fallback"),
    };

    let parent_key = row
        .parent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != row.id)
        .map(|parent| format!("{source}:{parent}"));

    let mut task = TaskSnapshot::new(source, &row.id);
    task.parent_key = parent_key;
    task.title = truncate_chars(&title, 240);
    task.title_source = title_source.to_owned();
    task.project_path = project_path;
    task.project_name = project_name;
    task.created_at = created_at;
    task.updated_at = updated_at;
    task.status_source = Some(
        if archived {
            "archived"
        } else if running {
            "running"
        } else {
            "idle"
        }
        .to_owned(),
    );
    task.status_raw = if archived {
        Some("archived".to_owned())
    } else if running {
        Some("busy".to_owned())
    } else {
        None
    };
    task.source_archived = archived;
    task.model = model_label(row.model);
    task.agent_kind = Some("unknown".to_owned());
    task.resume_cmd = resume_builder(row.directory.as_deref(), &row.id);
    task.refresh_fingerprint();
    task
}

fn load_todos(
    conn: &rusqlite::Connection,
    source: &str,
    known_sessions: &HashSet<String>,
) -> crate::Result<Vec<TodoSnapshot>> {
    let mut stmt = conn.prepare(
        "SELECT rowid, session_id, content, status, position FROM todo ORDER BY session_id, position, rowid",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<i64>>(4)?,
        ))
    })?;

    let mut todos = Vec::new();
    for row in rows {
        let (rowid, session_id, content, status, position) = row?;
        if !known_sessions.contains(&session_id) {
            continue;
        }
        let subject = content.trim();
        if subject.is_empty() {
            continue;
        }
        todos.push(TodoSnapshot {
            task_key: format!("{source}:{session_id}"),
            item_id: rowid.to_string(),
            subject: truncate_chars(subject, 240),
            status: normalize_todo_status(&status),
            position: position.unwrap_or(0),
        });
    }
    Ok(todos)
}

fn model_label(raw: Option<String>) -> Option<String> {
    let raw = raw?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('{') {
        let value: Value = serde_json::from_str(trimmed).ok()?;
        return value
            .get("id")
            .or_else(|| value.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
    }
    Some(trimmed.to_owned())
}

fn normalize_todo_status(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "pending" => "pending",
        "in_progress" | "in-progress" | "doing" => "in_progress",
        "completed" | "complete" | "done" => "completed",
        "cancelled" | "canceled" => "cancelled",
        _ => "unknown",
    }
    .to_owned()
}

struct SessionRow {
    id: String,
    parent_id: Option<String>,
    title: Option<String>,
    directory: Option<String>,
    model: Option<String>,
    time_created: Option<i64>,
    time_updated: Option<i64>,
    time_archived: Option<i64>,
    time_busy: Option<i64>,
    time_idle: Option<i64>,
}
