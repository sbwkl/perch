use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;

use super::common::{
    epoch_ms_to_seconds, fallback_title, json_string, normalize_project_path, now_seconds,
    open_read_only, project_name, table_exists, truncate_chars,
};
use super::Collector;
use crate::models::{SourceResult, TaskSnapshot, SOURCE_QODER};

pub struct QoderCollector {
    db_path: PathBuf,
}

impl QoderCollector {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    fn scan_inner(&self) -> crate::Result<SourceResult> {
        if !self.db_path.is_file() {
            return Ok(SourceResult::unavailable(
                SOURCE_QODER,
                format!("未找到 Qoder 数据库：{}", self.db_path.display()),
            ));
        }
        let conn = open_read_only(&self.db_path)?;

        let has_messages = table_exists(&conn, "chat_session_messages")?;
        let has_turns = table_exists(&conn, "chat_session_turn_states")?;
        let has_placements = table_exists(&conn, "chat_session_sidebar_placements")?;

        let mut message_counts: HashMap<String, i64> = HashMap::new();
        if has_messages {
            let mut stmt = conn.prepare(
                "SELECT session_id, COUNT(*) FROM chat_session_messages GROUP BY session_id",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (session_id, count) = row?;
                message_counts.insert(session_id, count);
            }
        }

        let mut last_message_roles: HashMap<String, String> = HashMap::new();
        if has_messages {
            let mut stmt = conn.prepare(
                "SELECT m.session_id, m.role FROM chat_session_messages m \
                 JOIN (SELECT session_id, MAX(rowid) AS last_row FROM chat_session_messages \
                       WHERE role IS NOT NULL AND TRIM(role) != '' \
                       GROUP BY session_id) latest \
                 ON latest.session_id = m.session_id AND latest.last_row = m.rowid",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?;
            for row in rows {
                let (session_id, role) = row?;
                if let Some(role) = role.filter(|value| !value.trim().is_empty()) {
                    last_message_roles.insert(session_id, role);
                }
            }
        }

        let mut turn_status: HashMap<String, String> = HashMap::new();
        if has_turns {
            let mut stmt = conn.prepare(
                "SELECT session_id, status FROM chat_session_turn_states ORDER BY rowid",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (session_id, status) = row?;
                turn_status.insert(session_id, status);
            }
        }

        let mut pinned_sessions: HashMap<String, bool> = HashMap::new();
        if has_placements {
            let mut stmt =
                conn.prepare("SELECT session_id, pinned FROM chat_session_sidebar_placements")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (session_id, pinned) = row?;
                let entry = pinned_sessions.entry(session_id).or_insert(false);
                *entry = *entry || pinned != 0;
            }
        }

        let mut stmt = conn.prepare(
            "SELECT session_id, session_kind, owner_session_id, title, workspace_id, cwd, \
                    git_branch, model, created_at, updated_at, archived, unread, extra_json \
             FROM chat_sessions WHERE deleted_at IS NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(QoderRow {
                session_id: row.get(0)?,
                session_kind: row.get(1)?,
                owner_session_id: row.get(2)?,
                title: row.get(3)?,
                workspace_id: row.get(4)?,
                cwd: row.get(5)?,
                git_branch: row.get(6)?,
                model: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                archived: row.get(10)?,
                unread: row.get(11)?,
                extra_json: row.get(12)?,
            })
        })?;

        let mut tasks = Vec::new();
        for row in rows {
            let row = row?;
            let extra = row
                .extra_json
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                .unwrap_or(Value::Null);

            // Qoder 对未打开项目的会话留空 workspace_id，并把 cwd 填成
            // ~/Documents/Qoder/<日期>/<会话前缀> 这类自动生成的沙箱目录，不能当作项目。
            let has_workspace = row
                .workspace_id
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
            let project_path = if has_workspace {
                row.cwd
                    .as_deref()
                    .map(normalize_project_path)
                    .filter(|value| !value.is_empty())
            } else {
                None
            };
            let project_name = project_path.as_deref().and_then(project_name);
            let created_at = epoch_ms_to_seconds(row.created_at);
            let updated_at = epoch_ms_to_seconds(row.updated_at).or(created_at);

            let raw_title = row
                .title
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let provisional = json_string(&extra, "provisionalTitle");
            let title_source_extra = json_string(&extra, "titleSource");
            let (title, title_source) = if let Some(title) = raw_title {
                (
                    title.to_owned(),
                    title_source_extra.unwrap_or_else(|| "source".to_owned()),
                )
            } else if let Some(provisional) = provisional {
                (provisional, "provisional".to_owned())
            } else {
                (
                    fallback_title(project_name.as_deref(), created_at),
                    "fallback".to_owned(),
                )
            };

            let archived = row.archived.unwrap_or(0) != 0;
            let turn_raw = turn_status.get(&row.session_id);
            let running_turn = turn_raw.is_some_and(|raw| raw.eq_ignore_ascii_case("running"));
            // 新版 Qoder 的 turn_states 可能为空，此时以“最后一条消息是 user、尚无回复”判定运行中。
            // 超过一天的未回复多半是中断后遗留的会话，不再当作运行中。
            let fresh = row
                .updated_at
                .is_some_and(|ms| now_seconds() - ms as f64 / 1000.0 <= 86_400.0);
            let awaiting_reply = fresh
                && last_message_roles
                    .get(&row.session_id)
                    .is_some_and(|role| role.eq_ignore_ascii_case("user"));
            let (status_source, status_raw) = if archived {
                (Some("archived".to_owned()), Some("archived".to_owned()))
            } else if running_turn {
                (Some("running".to_owned()), turn_raw.cloned())
            } else if awaiting_reply {
                (Some("running".to_owned()), Some("waiting_reply".to_owned()))
            } else if let Some(raw) = turn_raw {
                (None, Some(raw.clone()))
            } else {
                (Some("idle".to_owned()), None)
            };

            let parent_key = if row
                .session_kind
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("side_chat"))
            {
                row.owner_session_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty() && *value != row.session_id)
                    .map(|owner| format!("{SOURCE_QODER}:{owner}"))
            } else {
                None
            };

            let mut task = TaskSnapshot::new(SOURCE_QODER, &row.session_id);
            task.parent_key = parent_key;
            task.title = truncate_chars(&title, 240);
            task.title_source = title_source;
            task.project_path = project_path;
            task.project_name = project_name;
            task.created_at = created_at;
            task.updated_at = updated_at;
            task.status_source = status_source;
            task.status_raw = status_raw;
            task.source_archived = archived;
            task.source_unread = row.unread.unwrap_or(0) != 0;
            task.source_pinned = pinned_sessions
                .get(&row.session_id)
                .copied()
                .unwrap_or(false);
            task.model = row.model.filter(|value| !value.trim().is_empty());
            task.git_branch = row.git_branch.filter(|value| !value.trim().is_empty());
            task.agent_kind = Some("desktop".to_owned());
            task.message_count = message_counts.get(&row.session_id).copied();
            task.refresh_fingerprint();
            tasks.push(task);
        }

        let mut result = SourceResult::ok(SOURCE_QODER, tasks, Vec::new());
        result.files_seen = 1;
        result.probe_kind = "sqlite";
        Ok(result)
    }
}

impl Collector for QoderCollector {
    fn source(&self) -> &'static str {
        SOURCE_QODER
    }

    fn scan(&self) -> SourceResult {
        match self.scan_inner() {
            Ok(result) => result,
            Err(error) => SourceResult::error(SOURCE_QODER, error.to_string()),
        }
    }
}

struct QoderRow {
    session_id: String,
    session_kind: Option<String>,
    owner_session_id: Option<String>,
    title: Option<String>,
    workspace_id: Option<String>,
    cwd: Option<String>,
    git_branch: Option<String>,
    model: Option<String>,
    created_at: Option<i64>,
    updated_at: Option<i64>,
    archived: Option<i64>,
    unread: Option<i64>,
    extra_json: Option<String>,
}
