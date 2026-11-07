use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::common::{
    fallback_title, is_jsonl, json_string, parse_timestamp_value, project_name, truncate_chars,
};
use super::Collector;
use crate::models::{SourceResult, TaskSnapshot, SOURCE_TRAE_CN};

const UNKNOWN: &str = "unknown";
const INSTALL_HINT: &str = "usage hooks install trae-cn --scope global";

/// Trae CN does not keep a readable local session index: the agent database is
/// encrypted and the session list lives server-side. usage-ledger's Trae hooks
/// append plaintext metadata events (`events-*.jsonl`) for every turn, which is
/// the only stable, message-free source Perch can aggregate.
pub struct TraeCnCollector {
    events_dir: PathBuf,
}

impl TraeCnCollector {
    pub fn new(events_dir: PathBuf) -> Self {
        Self { events_dir }
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
        let files_seen = event_files.len() as i64;
        let mut warnings = Vec::new();
        let mut sessions: HashMap<String, SessionAggregate> = HashMap::new();

        for path in &event_files {
            if let Err(error) = self.read_events(path, &mut sessions, &mut warnings) {
                warnings.push(format!("读取 {} 失败：{error}", path.display()));
            }
        }

        let mut tasks: Vec<TaskSnapshot> = sessions
            .into_iter()
            .map(|(session_id, aggregate)| {
                let mut task = TaskSnapshot::new(SOURCE_TRAE_CN, &session_id);
                task.title = fallback_title();
                task.title_source = "fallback".to_owned();
                task.project_name = aggregate
                    .project
                    .as_deref()
                    .and_then(|value| project_name(value).or_else(|| Some(value.to_owned())));
                task.created_at = aggregate.first_ts;
                task.updated_at = aggregate.last_ts;
                task.status_source = Some("idle".to_owned());
                task.model = aggregate.model.map(|value| truncate_chars(&value, 80));
                task.agent_kind = Some("desktop".to_owned());
                task.refresh_fingerprint();
                task
            })
            .collect();
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
        }
        Ok(())
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

#[derive(Default)]
struct SessionAggregate {
    first_ts: Option<f64>,
    last_ts: Option<f64>,
    project: Option<String>,
    project_ts: f64,
    model: Option<String>,
    model_ts: f64,
}
