use std::path::{Path, PathBuf};

use super::common::{
    fallback_title, normalize_project_path, open_read_only, parse_timestamp_str, project_name,
    shell_cd_command, table_exists, truncate_chars,
};
use super::Collector;
use crate::continue_work::valid_session_id_charset;
use crate::models::{SourceResult, TaskSnapshot, SOURCE_CLINE};

/// Cline CLI and the Cline macOS app share one session index at
/// `~/.cline/data/db/sessions.db`. The table is metadata-only; scanning never
/// opens the per-session message files, it only records a rebuildable pointer
/// to `<sessions>/<id>/<id>.messages.json` for the on-demand transcript reader.
pub struct ClineCollector {
    db_path: PathBuf,
}

impl ClineCollector {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    fn scan_inner(&self) -> crate::Result<SourceResult> {
        if !self.db_path.is_file() {
            return Ok(SourceResult::unavailable(
                SOURCE_CLINE,
                format!("未找到 Cline 数据库：{}", self.db_path.display()),
            ));
        }
        let conn = open_read_only(&self.db_path)?;
        if !table_exists(&conn, "sessions")? {
            return Err(crate::PerchError::message(format!(
                "{} 中缺少 sessions 表",
                self.db_path.display()
            )));
        }

        let mut stmt = conn.prepare(
            "SELECT session_id, source, status, started_at, updated_at, cwd, workspace_root, \
                    parent_session_id, model, \
                    CASE WHEN metadata_json IS NOT NULL AND json_valid(metadata_json) \
                         THEN json_extract(metadata_json, '$.title') END \
             FROM sessions",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SessionRow {
                session_id: row.get(0)?,
                source: row.get(1)?,
                status: row.get(2)?,
                started_at: row.get(3)?,
                updated_at: row.get(4)?,
                cwd: row.get(5)?,
                workspace_root: row.get(6)?,
                parent_session_id: row.get(7)?,
                model: row.get(8)?,
                title: row.get(9)?,
            })
        })?;

        let mut tasks = Vec::new();
        let sessions_root = sessions_root(&self.db_path);
        for row in rows {
            tasks.push(build_task(row?, sessions_root.as_deref()));
        }
        tasks.sort_by(|left, right| {
            right
                .updated_at
                .unwrap_or(0.0)
                .total_cmp(&left.updated_at.unwrap_or(0.0))
                .then_with(|| left.source_id.cmp(&right.source_id))
        });

        let mut result = SourceResult::ok(SOURCE_CLINE, tasks, Vec::new());
        result.files_seen = 1;
        result.probe_kind = "sqlite";
        Ok(result)
    }
}

impl Collector for ClineCollector {
    fn source(&self) -> &'static str {
        SOURCE_CLINE
    }

    fn scan(&self) -> SourceResult {
        match self.scan_inner() {
            Ok(result) => result,
            Err(error) => SourceResult::error(SOURCE_CLINE, error.to_string()),
        }
    }
}

fn build_task(row: SessionRow, sessions_root: Option<&Path>) -> TaskSnapshot {
    let raw_directory = if row.workspace_root.trim().is_empty() {
        row.cwd.as_str()
    } else {
        row.workspace_root.as_str()
    };
    // Chat sessions run inside Cline's own workspace directory, which is not a user project.
    let project_path = normalize_project_path(raw_directory);
    let project_path =
        Some(project_path).filter(|path| !path.is_empty() && !is_internal_workspace(path));
    let project_name = project_path.as_deref().and_then(project_name);

    let created_at = parse_timestamp_str(&row.started_at);
    let updated_at = parse_timestamp_str(&row.updated_at).or(created_at);
    let title = row
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| truncate_chars(value, 240));
    let title_source = if title.is_some() {
        "source"
    } else {
        "fallback"
    };

    let raw_status = row.status.trim();
    let status_source = match raw_status.to_ascii_lowercase().as_str() {
        "running" => "running",
        "completed" | "complete" | "done" => "complete",
        "failed" | "error" => "failed",
        _ => "idle",
    };
    let agent_kind = match row.source.as_str() {
        "cli" | "desktop" => row.source.as_str(),
        _ => "unknown",
    };

    let parent_key = row
        .parent_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != row.session_id)
        .map(|parent| format!("{SOURCE_CLINE}:{parent}"));

    let mut task = TaskSnapshot::new(SOURCE_CLINE, &row.session_id);
    task.parent_key = parent_key;
    task.title = title.unwrap_or_else(fallback_title);
    task.title_source = title_source.to_owned();
    task.project_path = project_path;
    task.project_name = project_name;
    task.created_at = created_at;
    task.updated_at = updated_at;
    task.status_source = Some(status_source.to_owned());
    task.status_raw = (!raw_status.is_empty()).then(|| raw_status.to_owned());
    task.model = row
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| truncate_chars(value, 80));
    task.agent_kind = Some(agent_kind.to_owned());
    task.source_path = messages_pointer(sessions_root, &row.session_id);
    // Mirror what continue_task executes: cwd comes from the filtered
    // project_path, so internal chat workspaces get a bare command.
    let command = format!("cline --id {}", row.session_id);
    task.resume_cmd =
        Some(shell_cd_command(task.project_path.as_deref(), &command).unwrap_or(command));
    task.refresh_fingerprint();
    task
}

fn is_internal_workspace(path: &str) -> bool {
    path.contains("/.cline/data/workspaces/")
}

/// 会话目录与数据库同源：`<data>/db/sessions.db` 旁的 `<data>/sessions`。
/// 覆盖 `PERCH_CLINE_DB` 时按同样相对布局推导，fixture 由此获得独立目录。
fn sessions_root(db_path: &Path) -> Option<PathBuf> {
    db_path
        .parent()
        .and_then(Path::parent)
        .map(|data_dir| data_dir.join("sessions"))
}

/// 指针列只记录可重建的 messages.json 绝对路径；session id 来自 Cline 自己的
/// 数据库，仍按任务 key 的 sessionId 字符集过滤，杜绝路径片段拼进指针。
fn messages_pointer(sessions_root: Option<&Path>, session_id: &str) -> Option<String> {
    let root = sessions_root?;
    if !valid_session_id_charset(session_id) {
        return None;
    }
    let pointer = root
        .join(session_id)
        .join(format!("{session_id}.messages.json"));
    Some(pointer.to_string_lossy().into_owned())
}

struct SessionRow {
    session_id: String,
    source: String,
    status: String,
    started_at: String,
    updated_at: String,
    cwd: String,
    workspace_root: String,
    parent_session_id: Option<String>,
    model: Option<String>,
    title: Option<String>,
}
