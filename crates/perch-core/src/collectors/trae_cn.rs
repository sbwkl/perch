use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::{Local, NaiveDate, TimeZone};
use serde_json::Value;
use walkdir::WalkDir;

use super::common::{
    fallback_title, is_jsonl, json_string, parse_timestamp_value, project_name, truncate_chars,
};
use super::Collector;
use crate::models::{SourceResult, TaskSnapshot, SOURCE_TRAE_CN};

const UNKNOWN: &str = "unknown";
const INSTALL_HINT: &str = "usage hooks install trae-cn --scope global";
const RUNNING_EVENTS: [&str; 3] = ["UserPromptSubmit", "PreToolUse", "PostToolUse"];
const IDLE_EVENTS: [&str; 2] = ["SessionStart", "Stop"];
const MEMORY_PREFIX: &str = "session_memory_";

/// Trae CN does not keep a readable local session index: the agent database is
/// encrypted and the session list lives server-side. usage-ledger's Trae hooks
/// append plaintext metadata events (`events-*.jsonl`) for every turn, which is
/// the only stable, message-free source Perch can aggregate. Hook installs are
/// recent, so `memory/projects/**/session_memory_<id>.jsonl` file names are also
/// enumerated to cover sessions that predate the hook. Memory files are opened
/// only to read the first record's `intent` summary for a title; actions,
/// outcomes and message bodies are never read.
pub struct TraeCnCollector {
    events_dir: PathBuf,
    memory_dir: PathBuf,
}

impl TraeCnCollector {
    pub fn new(events_dir: PathBuf, memory_dir: PathBuf) -> Self {
        Self {
            events_dir,
            memory_dir,
        }
    }

    fn scan_inner(&self) -> crate::Result<SourceResult> {
        if !self.events_dir.is_dir() {
            let mut result = SourceResult::unavailable(
                SOURCE_TRAE_CN,
                format!(
                    "未找到 Trae CN hook 事件目录：{}；请先运行 `{INSTALL_HINT}`",
                    self.events_dir.display()
                ),
            );
            result.probe_kind = "file_tree";
            return Ok(result);
        }

        let event_files = self.event_files()?;
        let (memory_sessions, memory_files) = self.memory_sessions();
        let files_seen = (event_files.len() + memory_files) as i64;
        let mut warnings = Vec::new();
        let mut sessions: HashMap<String, SessionAggregate> = HashMap::new();

        for path in &event_files {
            if let Err(error) = self.read_events(path, &mut sessions, &mut warnings) {
                warnings.push(format!("读取 {} 失败：{error}", path.display()));
            }
        }

        let mut tasks: Vec<TaskSnapshot> =
            Vec::with_capacity(sessions.len() + memory_sessions.len());
        for (session_id, aggregate) in &sessions {
            let memory = memory_sessions.get(session_id);
            let mut task = TaskSnapshot::new(SOURCE_TRAE_CN, session_id);
            task.title = memory
                .and_then(|session| session.title.clone())
                .unwrap_or_else(fallback_title);
            task.title_source = if memory.and_then(|session| session.title.as_ref()).is_some() {
                "ai".to_owned()
            } else {
                "fallback".to_owned()
            };
            task.project_name = aggregate
                .project
                .as_deref()
                .and_then(project_name)
                .or_else(|| memory.map(|session| session.project.clone()));
            task.created_at = aggregate.first_ts;
            task.updated_at = aggregate.last_ts;
            task.status_source = Some(status_from_event(aggregate.last_event.as_deref()));
            task.model = aggregate
                .model
                .as_deref()
                .map(|value| truncate_chars(value, 80));
            task.agent_kind = Some("desktop".to_owned());
            task.refresh_fingerprint();
            tasks.push(task);
        }

        for (session_id, memory) in &memory_sessions {
            if sessions.contains_key(session_id) {
                continue;
            }
            let mut task = TaskSnapshot::new(SOURCE_TRAE_CN, session_id);
            task.title = memory.title.clone().unwrap_or_else(fallback_title);
            task.title_source = if memory.title.is_some() {
                "ai"
            } else {
                "fallback"
            }
            .to_owned();
            task.project_name = Some(memory.project.clone());
            task.created_at = memory.created_at;
            task.updated_at = memory.updated_at.or(memory.created_at);
            task.status_source = Some("idle".to_owned());
            task.agent_kind = Some("desktop".to_owned());
            task.refresh_fingerprint();
            tasks.push(task);
        }
        tasks.sort_by(|left, right| {
            right
                .updated_at
                .unwrap_or(0.0)
                .total_cmp(&left.updated_at.unwrap_or(0.0))
                .then_with(|| left.source_id.cmp(&right.source_id))
        });

        let mut result = if warnings.is_empty() {
            SourceResult::ok(SOURCE_TRAE_CN, tasks, Vec::new())
        } else {
            SourceResult::partial(SOURCE_TRAE_CN, tasks, Vec::new(), warnings.join("；"))
        };
        result.files_seen = files_seen;
        result.probe_kind = "file_tree";
        Ok(result)
    }

    fn event_files(&self) -> crate::Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.events_dir)? {
            let entry = entry?;
            if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("events-") && is_jsonl(&entry.path()) {
                files.push(entry.path());
            }
        }
        files.sort();
        Ok(files)
    }

    fn read_events(
        &self,
        path: &Path,
        sessions: &mut HashMap<String, SessionAggregate>,
        warnings: &mut Vec<String>,
    ) -> crate::Result<()> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let value: Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(error) => {
                    warnings.push(format!(
                        "{}:{} 解析失败：{error}",
                        path.display(),
                        index + 1
                    ));
                    continue;
                }
            };
            let Some(session_id) = json_string(&value, "session") else {
                continue;
            };
            if session_id == UNKNOWN {
                continue;
            }
            let Some(ts) = value.get("ts").and_then(parse_timestamp_value) else {
                continue;
            };
            let aggregate = sessions.entry(session_id).or_default();
            aggregate.first_ts = Some(aggregate.first_ts.map_or(ts, |first| first.min(ts)));
            aggregate.last_ts = Some(aggregate.last_ts.map_or(ts, |last| last.max(ts)));
            if let Some(project) = json_string(&value, "project").filter(|value| value != UNKNOWN) {
                if ts >= aggregate.project_ts {
                    aggregate.project_ts = ts;
                    aggregate.project = Some(project);
                }
            }
            if let Some(model) = json_string(&value, "model").filter(|value| value != UNKNOWN) {
                if ts >= aggregate.model_ts {
                    aggregate.model_ts = ts;
                    aggregate.model = Some(model);
                }
            }
            if let Some(event) = json_string(&value, "event") {
                if (RUNNING_EVENTS.contains(&event.as_str())
                    || IDLE_EVENTS.contains(&event.as_str()))
                    && ts >= aggregate.last_event_ts
                {
                    aggregate.last_event_ts = ts;
                    aggregate.last_event = Some(event);
                }
            }
        }
        Ok(())
    }

    fn memory_sessions(&self) -> (HashMap<String, MemorySession>, usize) {
        let mut sessions: HashMap<String, MemorySession> = HashMap::new();
        let mut files = 0usize;
        if !self.memory_dir.is_dir() {
            return (sessions, files);
        }
        for entry in WalkDir::new(&self.memory_dir)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(session_id) = name
                .strip_prefix(MEMORY_PREFIX)
                .and_then(|rest| rest.strip_suffix(".jsonl"))
            else {
                continue;
            };
            if session_id.is_empty() {
                continue;
            }
            files += 1;
            let Some(date_dir) = entry.path().parent() else {
                continue;
            };
            let Some(project_dir) = date_dir.parent() else {
                continue;
            };
            let project_dir_name = project_dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let project = memory_project_name(&project_dir_name);
            let updated_at = entry
                .metadata()
                .ok()
                .and_then(|metadata| file_modified_ts(&metadata));
            let created_at = date_dir
                .file_name()
                .and_then(|name| memory_date_ts(&name.to_string_lossy()))
                .or(updated_at);
            let intent = read_memory_intent(entry.path());

            let memory = sessions.entry(session_id.to_owned()).or_default();
            let is_newer = match (memory.updated_at, updated_at) {
                (Some(current), Some(next)) => next >= current,
                _ => memory.project.is_empty(),
            };
            if memory.project.is_empty() || is_newer {
                memory.project = project;
            }
            if let Some(intent) = intent {
                let title_date = created_at.or(updated_at).unwrap_or(f64::MAX);
                if memory.title.is_none() || title_date < memory.title_date {
                    memory.title = Some(intent);
                    memory.title_date = title_date;
                }
            }
            memory.created_at = match (memory.created_at, created_at) {
                (Some(current), Some(next)) => Some(current.min(next)),
                (current, next) => current.or(next),
            };
            memory.updated_at = match (memory.updated_at, updated_at) {
                (Some(current), Some(next)) => Some(current.max(next)),
                (current, next) => current.or(next),
            };
        }
        (sessions, files)
    }
}

impl Collector for TraeCnCollector {
    fn source(&self) -> &'static str {
        SOURCE_TRAE_CN
    }

    fn scan(&self) -> SourceResult {
        match self.scan_inner() {
            Ok(result) => result,
            Err(error) => SourceResult::error(SOURCE_TRAE_CN, error.to_string()),
        }
    }
}

fn status_from_event(event: Option<&str>) -> String {
    match event {
        Some(event) if RUNNING_EVENTS.contains(&event) => "running".to_owned(),
        _ => "idle".to_owned(),
    }
}

#[derive(Default)]
struct MemorySession {
    project: String,
    created_at: Option<f64>,
    updated_at: Option<f64>,
    title: Option<String>,
    title_date: f64,
}

fn read_memory_intent(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(intent) = json_string(&value, "intent") {
            let intent = intent.trim();
            if !intent.is_empty() {
                return Some(compact_title(intent, 60));
            }
        }
    }
    None
}

fn compact_title(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_owned();
    }
    format!("{}…", truncate_chars(value, limit))
}

fn file_modified_ts(metadata: &fs::Metadata) -> Option<f64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs_f64())
}

fn memory_date_ts(date: &str) -> Option<f64> {
    let date = NaiveDate::parse_from_str(date, "%Y%m%d").ok()?;
    let midnight = date.and_hms_opt(0, 0, 0)?;
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map(|value| value.timestamp() as f64)
}

/// Trae encodes project directories by replacing non-alphanumerics with `-`
/// and appending `--p2-<hash>`, so the real path is ambiguous. Rebuild a
/// readable workspace name instead: drop the hash, strip common parent
/// segments and keep the meaningful tail.
fn memory_project_name(directory: &str) -> String {
    let base = directory
        .rfind("--p2-")
        .map(|index| &directory[..index])
        .unwrap_or(directory);
    let segments: Vec<&str> = base
        .trim_start_matches('-')
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.is_empty() {
        return directory.to_owned();
    }
    if let Some(name) = trailing_workdir_name(&segments) {
        return name;
    }
    let roots = [
        "Users",
        "wayne",
        "Documents",
        "workspace",
        "private",
        "var",
        "home",
        "tmp",
    ];
    let last = segments.len() - 1;
    let start = segments[..last]
        .iter()
        .rposition(|segment| roots.contains(segment))
        .map(|index| index + 1)
        .unwrap_or(last);
    segments[start..].join("-")
}

fn trailing_workdir_name(segments: &[&str]) -> Option<String> {
    let length = segments.len();
    if length < 3 {
        return None;
    }
    let date = segments[length - 2];
    let index = segments[length - 1];
    let date_like =
        date.len() == 8 && date.starts_with("20") && date.chars().all(|ch| ch.is_ascii_digit());
    if !date_like || !index.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let start = segments[..length - 2]
        .iter()
        .rposition(|segment| segment.to_ascii_lowercase().contains("temp"))?;
    Some(segments[start..].join("-"))
}

#[derive(Default)]
struct SessionAggregate {
    first_ts: Option<f64>,
    last_ts: Option<f64>,
    project: Option<String>,
    project_ts: f64,
    model: Option<String>,
    model_ts: f64,
    last_event: Option<String>,
    last_event_ts: f64,
}
