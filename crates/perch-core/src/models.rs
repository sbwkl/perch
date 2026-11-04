use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::usage::{TaskUsage, UsageOverview};

pub const SOURCE_QODER: &str = "qoder";
pub const SOURCE_CLAUDE: &str = "claude";
pub const SOURCE_CODEX: &str = "codex";
pub const SOURCE_OPENCODE: &str = "opencode";
pub const SOURCE_KILO: &str = "kilo";

pub const ALL_SOURCES: [&str; 5] = [
    SOURCE_QODER,
    SOURCE_CLAUDE,
    SOURCE_CODEX,
    SOURCE_OPENCODE,
    SOURCE_KILO,
];

pub fn source_label(source: &str) -> &'static str {
    match source {
        SOURCE_QODER => "Qoder",
        SOURCE_CLAUDE => "Claude Code",
        SOURCE_CODEX => "Codex",
        SOURCE_OPENCODE => "OpenCode",
        SOURCE_KILO => "Kilo",
        _ => "未知来源",
    }
}

pub const RECENT_WINDOW_SECONDS: f64 = 7.0 * 24.0 * 60.0 * 60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceScanStatus {
    Ok,
    Partial,
    Unavailable,
    Error,
}

impl SourceScanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Partial => "partial",
            Self::Unavailable => "unavailable",
            Self::Error => "error",
        }
    }

    pub fn is_success(self) -> bool {
        matches!(self, Self::Ok)
    }

    pub fn keeps_snapshot(self) -> bool {
        matches!(self, Self::Unavailable | Self::Error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub key: String,
    pub source: String,
    pub source_id: String,
    pub parent_key: Option<String>,
    pub title: String,
    pub title_source: String,
    pub project_path: Option<String>,
    pub project_name: Option<String>,
    pub created_at: Option<f64>,
    pub updated_at: Option<f64>,
    pub status_source: Option<String>,
    pub status_raw: Option<String>,
    pub source_archived: bool,
    pub source_unread: bool,
    pub source_pinned: bool,
    pub source_missing: bool,
    pub model: Option<String>,
    pub git_branch: Option<String>,
    pub agent_kind: Option<String>,
    pub message_count: Option<i64>,
    pub todo_total: Option<i64>,
    pub todo_done: Option<i64>,
    pub deeplink: Option<String>,
    pub resume_cmd: Option<String>,
    pub fingerprint: String,
    pub seen_at: f64,
    pub last_seen_run: Option<i64>,
}

impl TaskSnapshot {
    pub fn new(source: &str, source_id: &str) -> Self {
        Self {
            key: format!("{source}:{source_id}"),
            source: source.to_owned(),
            source_id: source_id.to_owned(),
            parent_key: None,
            title: String::new(),
            title_source: "fallback".to_owned(),
            project_path: None,
            project_name: None,
            created_at: None,
            updated_at: None,
            status_source: None,
            status_raw: None,
            source_archived: false,
            source_unread: false,
            source_pinned: false,
            source_missing: false,
            model: None,
            git_branch: None,
            agent_kind: None,
            message_count: None,
            todo_total: None,
            todo_done: None,
            deeplink: None,
            resume_cmd: None,
            fingerprint: String::new(),
            seen_at: 0.0,
            last_seen_run: None,
        }
    }

    pub fn compute_fingerprint(&self) -> String {
        let mut hasher = DefaultHasher::new();
        self.title.hash(&mut hasher);
        self.title_source.hash(&mut hasher);
        self.parent_key.hash(&mut hasher);
        self.project_path.hash(&mut hasher);
        self.project_name.hash(&mut hasher);
        hash_optional_f64(self.created_at, &mut hasher);
        hash_optional_f64(self.updated_at, &mut hasher);
        self.status_source.hash(&mut hasher);
        self.status_raw.hash(&mut hasher);
        self.source_archived.hash(&mut hasher);
        self.source_unread.hash(&mut hasher);
        self.source_pinned.hash(&mut hasher);
        self.model.hash(&mut hasher);
        self.git_branch.hash(&mut hasher);
        self.agent_kind.hash(&mut hasher);
        self.message_count.hash(&mut hasher);
        self.todo_total.hash(&mut hasher);
        self.todo_done.hash(&mut hasher);
        self.deeplink.hash(&mut hasher);
        self.resume_cmd.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    pub fn refresh_fingerprint(&mut self) {
        self.fingerprint = self.compute_fingerprint();
    }
}

fn hash_optional_f64(value: Option<f64>, hasher: &mut DefaultHasher) {
    match value {
        Some(number) => {
            1u8.hash(hasher);
            number.to_bits().hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoSnapshot {
    pub task_key: String,
    pub item_id: String,
    pub subject: String,
    pub status: String,
    pub position: i64,
}

#[derive(Debug, Clone)]
pub struct SourceResult {
    pub source: String,
    pub status: SourceScanStatus,
    pub tasks: Vec<TaskSnapshot>,
    pub todos: Vec<TodoSnapshot>,
    pub files_seen: i64,
    pub schema_version: Option<String>,
    pub error: Option<String>,
    pub probe_kind: &'static str,
}

impl SourceResult {
    pub fn ok(source: &str, tasks: Vec<TaskSnapshot>, todos: Vec<TodoSnapshot>) -> Self {
        Self {
            source: source.to_owned(),
            status: SourceScanStatus::Ok,
            tasks,
            todos,
            files_seen: 0,
            schema_version: None,
            error: None,
            probe_kind: "sqlite",
        }
    }

    pub fn partial(
        source: &str,
        tasks: Vec<TaskSnapshot>,
        todos: Vec<TodoSnapshot>,
        error: String,
    ) -> Self {
        Self {
            source: source.to_owned(),
            status: SourceScanStatus::Partial,
            tasks,
            todos,
            files_seen: 0,
            schema_version: None,
            error: Some(error),
            probe_kind: "sqlite",
        }
    }

    pub fn unavailable(source: &str, error: impl Into<String>) -> Self {
        Self {
            source: source.to_owned(),
            status: SourceScanStatus::Unavailable,
            tasks: Vec::new(),
            todos: Vec::new(),
            files_seen: 0,
            schema_version: None,
            error: Some(error.into()),
            probe_kind: "sqlite",
        }
    }

    pub fn error(source: &str, error: impl Into<String>) -> Self {
        Self {
            source: source.to_owned(),
            status: SourceScanStatus::Error,
            tasks: Vec::new(),
            todos: Vec::new(),
            files_seen: 0,
            schema_version: None,
            error: Some(error.into()),
            probe_kind: "sqlite",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoView {
    pub item_id: String,
    pub subject: String,
    pub status: String,
    pub position: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskView {
    pub key: String,
    pub source: String,
    pub source_label: String,
    pub source_id: String,
    pub parent_key: Option<String>,
    pub is_child: bool,
    pub is_automation: bool,
    pub title: String,
    pub title_source: String,
    pub project_path: Option<String>,
    pub project_name: Option<String>,
    pub workspace_key: String,
    pub workspace_name: String,
    pub created_at: Option<f64>,
    pub updated_at: Option<f64>,
    pub status: String,
    pub status_raw: Option<String>,
    pub source_archived: bool,
    pub source_unread: bool,
    pub source_pinned: bool,
    pub source_missing: bool,
    pub model: Option<String>,
    pub git_branch: Option<String>,
    pub agent_kind: Option<String>,
    pub message_count: Option<i64>,
    pub todo_total: i64,
    pub todo_done: i64,
    pub deeplink: Option<String>,
    pub resume_cmd: Option<String>,
    pub is_recent: bool,
    pub todos: Vec<TodoView>,
    /// Optional usage-ledger projection; absent when the archive is missing or
    /// this session has no recorded usage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TaskUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceGroup {
    pub key: String,
    pub name: String,
    pub project_path: Option<String>,
    pub tasks: Vec<TaskView>,
    pub last_activity_at: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceView {
    pub source: String,
    pub label: String,
    pub status: String,
    pub last_scan_at: Option<f64>,
    pub last_ok_at: Option<f64>,
    pub task_count: i64,
    pub files_seen: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Counts {
    pub total: i64,
    pub recent: i64,
    pub older: i64,
    pub archived: i64,
    pub attention: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapPayload {
    pub generated_at: f64,
    pub sources: Vec<SourceView>,
    pub workspaces: Vec<WorkspaceGroup>,
    pub counts: Counts,
    /// Present only when usage-ledger's database was readable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageOverview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceReport {
    pub source: String,
    pub status: String,
    pub mode: String,
    pub task_count: usize,
    pub todo_count: usize,
    pub missing_count: usize,
    pub duration_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncReport {
    pub started_at: f64,
    pub finished_at: f64,
    pub reports: Vec<SourceReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSyncState {
    pub source: String,
    pub last_scan_at: Option<f64>,
    pub last_ok_at: Option<f64>,
    pub last_mode: Option<String>,
    pub last_trigger: Option<String>,
    pub status: String,
    pub files_seen: Option<i64>,
    pub task_count: Option<i64>,
    pub schema_version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRun {
    pub id: i64,
    pub source: String,
    pub mode: String,
    pub scope: Option<String>,
    pub trigger: Option<String>,
    pub started_at: f64,
    pub finished_at: Option<f64>,
    pub status: String,
    pub task_count: i64,
    pub missing_count: i64,
    pub error: Option<String>,
}
