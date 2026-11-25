use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use rusqlite::Connection;
use walkdir::WalkDir;

use crate::collectors::common::open_read_only;
use crate::paths::SourcePaths;

pub const JSONL_POLL_INTERVAL: Duration = Duration::from_secs(15);
pub const LIVE_DEBOUNCE: Duration = Duration::from_secs(6);
pub const LIVE_COOLDOWN: Duration = Duration::from_secs(30);
pub const LIVE_MAX_WAIT: Duration = Duration::from_secs(60);

pub struct SourceProbe {
    sqlite: Vec<SqliteProbe>,
    jsonl: Vec<JsonlProbe>,
    last_jsonl_poll: Option<Instant>,
    deferred_jsonl: HashMap<&'static str, Instant>,
    pending_changes: HashMap<&'static str, Instant>,
}

struct SqliteProbe {
    source: &'static str,
    candidates: Vec<PathBuf>,
    conn: Option<Connection>,
    last_version: i64,
}

struct JsonlProbe {
    source: &'static str,
    roots: Vec<PathBuf>,
    json_roots: Vec<PathBuf>,
    log_roots: Vec<PathBuf>,
    named_json_roots: Vec<(PathBuf, &'static str)>,
    last_signature: (u64, u128),
}

impl SourceProbe {
    pub fn new(paths: &SourcePaths) -> Self {
        let sqlite = [
            ("qoder", vec![paths.qoder_db.clone()]),
            (
                "opencode",
                vec![paths.opencode_db.clone(), paths.opencode_stable_db.clone()],
            ),
            ("kilo", vec![paths.kilo_db.clone()]),
            ("cline", vec![paths.cline_db.clone()]),
            ("codearts", vec![paths.codearts_db.clone()]),
        ]
        .into_iter()
        .map(|(source, candidates)| {
            let conn = SourcePaths::first_existing(&candidates)
                .as_deref()
                .and_then(|path| open_read_only(path).ok());
            let last_version = conn.as_ref().and_then(data_version).unwrap_or(0);
            SqliteProbe {
                source,
                candidates,
                conn,
                last_version,
            }
        })
        .collect();

        let jsonl = vec![
            JsonlProbe {
                source: "claude",
                roots: vec![paths.claude_dir.join("projects")],
                json_roots: vec![paths.claude_dir.join("sessions")],
                log_roots: Vec::new(),
                named_json_roots: vec![(paths.claude_dir.join("projects"), "custom-title.json")],
                last_signature: (0, 0),
            },
            JsonlProbe {
                source: "codex",
                roots: vec![
                    paths.codex_dir.join("sessions"),
                    paths.codex_dir.join("archived_sessions"),
                ],
                json_roots: Vec::new(),
                log_roots: Vec::new(),
                named_json_roots: Vec::new(),
                last_signature: (0, 0),
            },
            JsonlProbe {
                source: crate::models::SOURCE_TRAE_CN,
                roots: vec![
                    paths.trae_cn_events_dir.clone(),
                    paths.trae_cn_memory_dir.clone(),
                ],
                json_roots: Vec::new(),
                log_roots: Vec::new(),
                named_json_roots: Vec::new(),
                last_signature: (0, 0),
            },
            JsonlProbe {
                source: crate::models::SOURCE_ANTIGRAVITY,
                roots: vec![
                    paths.antigravity_ide_root.clone(),
                    paths.antigravity_cli_root.clone(),
                ],
                json_roots: Vec::new(),
                log_roots: Vec::new(),
                named_json_roots: Vec::new(),
                last_signature: (0, 0),
            },
            JsonlProbe {
                source: crate::models::SOURCE_MINIMAX,
                roots: vec![paths.minimax_dir.join("v2/sessions")],
                json_roots: Vec::new(),
                log_roots: Vec::new(),
                named_json_roots: Vec::new(),
                last_signature: (0, 0),
            },
            JsonlProbe {
                source: crate::models::SOURCE_CODEBUDDY,
                roots: Vec::new(),
                json_roots: Vec::new(),
                log_roots: vec![
                    paths.codebuddy_cn_logs_dir.clone(),
                    paths.codebuddy_ide_logs_dir.clone(),
                ],
                named_json_roots: Vec::new(),
                last_signature: (0, 0),
            },
        ];
        let mut probe = Self {
            sqlite,
            jsonl,
            last_jsonl_poll: None,
            deferred_jsonl: HashMap::new(),
            pending_changes: HashMap::new(),
        };
        probe.reset_jsonl_baseline();
        probe
    }

    /// Resident-loop helper used while live scans are paused or a manual scan
    /// owns the engine lock: consume source changes without emitting them.
    pub fn poll_throttled(&mut self) {
        let now = Instant::now();
        let _ = self.changed_sources(now);
    }

    /// Immediate full probe for tests and one-shot callers. The resident Tauri
    /// loop uses `changed_sources` so directory walks are throttled.
    pub fn poll(&mut self) -> Vec<&'static str> {
        let mut changed = Vec::new();
        for probe in &mut self.sqlite {
            if probe.conn.is_none() {
                let Some(path) = SourcePaths::first_existing(&probe.candidates) else {
                    continue;
                };
                let Ok(conn) = open_read_only(&path) else {
                    continue;
                };
                probe.last_version = data_version(&conn).unwrap_or(0);
                probe.conn = Some(conn);
                changed.push(probe.source);
                continue;
            }
            let version = probe.conn.as_ref().and_then(data_version);
            let Some(version) = version else {
                probe.conn = None;
                continue;
            };
            if version != probe.last_version {
                probe.last_version = version;
                changed.push(probe.source);
            }
        }
        for probe in &mut self.jsonl {
            let signature = probe_signature(probe);
            if signature != probe.last_signature {
                probe.last_signature = signature;
                changed.push(probe.source);
            }
        }
        changed
    }

    pub fn poll_at(&mut self, now: Instant) {
        if self
            .last_jsonl_poll
            .is_some_and(|last| now.duration_since(last) < JSONL_POLL_INTERVAL)
        {
            return;
        }
        self.last_jsonl_poll = Some(now);
        for probe in &mut self.jsonl {
            if self
                .deferred_jsonl
                .get(probe.source)
                .is_some_and(|until| now < *until)
            {
                continue;
            }
            self.deferred_jsonl.remove(probe.source);
            let signature = probe_signature(probe);
            if signature != probe.last_signature {
                probe.last_signature = signature;
                self.pending_changes.insert(probe.source, now);
            }
        }
    }

    pub fn changed_sources(&mut self, now: Instant) -> Vec<&'static str> {
        self.poll_at(now);
        let mut changed = Vec::new();
        for probe in &mut self.sqlite {
            if probe.conn.is_none() {
                let Some(path) = SourcePaths::first_existing(&probe.candidates) else {
                    continue;
                };
                let Ok(conn) = open_read_only(&path) else {
                    continue;
                };
                probe.last_version = data_version(&conn).unwrap_or(0);
                probe.conn = Some(conn);
                changed.push(probe.source);
                continue;
            }
            let version = probe.conn.as_ref().and_then(data_version);
            let Some(version) = version else {
                probe.conn = None;
                continue;
            };
            if version != probe.last_version {
                probe.last_version = version;
                changed.push(probe.source);
            }
        }
        for (source, first) in &self.pending_changes {
            let deferred = self
                .deferred_jsonl
                .get(source)
                .is_some_and(|until| now < *until);
            if !deferred && now.duration_since(*first) >= LIVE_DEBOUNCE {
                changed.push(*source);
            }
        }
        for source in changed.iter() {
            self.pending_changes.remove(source);
            self.deferred_jsonl.remove(source);
        }
        changed.sort_unstable();
        changed.dedup();
        changed
    }

    pub fn defer_jsonl(&mut self, source: &str, now: Instant) {
        if let Some(probe_source) = self
            .jsonl
            .iter()
            .find(|probe| probe.source == source)
            .map(|probe| probe.source)
        {
            self.deferred_jsonl
                .insert(probe_source, now + LIVE_COOLDOWN);
        }
    }

    pub fn reset_baseline(&mut self, now: Instant) {
        for probe in &mut self.sqlite {
            probe.conn = SourcePaths::first_existing(&probe.candidates)
                .as_deref()
                .and_then(|path| open_read_only(path).ok());
            probe.last_version = probe.conn.as_ref().and_then(data_version).unwrap_or(0);
        }
        self.reset_jsonl_baseline();
        self.last_jsonl_poll = Some(now);
        self.deferred_jsonl.clear();
        self.pending_changes.clear();
    }

    fn reset_jsonl_baseline(&mut self) {
        for probe in &mut self.jsonl {
            probe.last_signature = probe_signature(probe);
        }
    }
}

#[derive(Debug, Default)]
pub struct LiveScanScheduler {
    first_change: HashMap<&'static str, Instant>,
    last_scan: HashMap<&'static str, Instant>,
}

impl LiveScanScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn note_change(&mut self, source: &'static str, now: Instant) {
        self.first_change.entry(source).or_insert(now);
    }

    pub fn note_changes(&mut self, changed: &[&'static str], now: Instant) {
        for source in changed {
            self.note_change(source, now);
        }
    }

    pub fn due_sources(&mut self, now: Instant) -> Vec<&'static str> {
        let pending: Vec<_> = self
            .first_change
            .iter()
            .map(|(source, first)| (*source, *first))
            .collect();
        let mut due = Vec::new();
        for (source, first) in pending {
            let cooled = self
                .last_scan
                .get(source)
                .is_none_or(|last| now.duration_since(*last) >= LIVE_COOLDOWN);
            let settled = now.duration_since(first) >= LIVE_DEBOUNCE;
            let expired = now.duration_since(first) >= LIVE_MAX_WAIT;
            if cooled && (settled || expired) {
                due.push(source);
                self.first_change.remove(source);
                self.last_scan.insert(source, now);
            }
        }
        due.sort_unstable();
        due.dedup();
        due
    }

    pub fn clear(&mut self) {
        self.first_change.clear();
        self.last_scan.clear();
    }
}

fn data_version(conn: &Connection) -> Option<i64> {
    conn.query_row("PRAGMA data_version", [], |row| row.get(0))
        .ok()
}

fn probe_signature(probe: &JsonlProbe) -> (u64, u128) {
    let mut signature = jsonl_signature(&probe.roots, &probe.json_roots, &probe.log_roots);
    for (root, basename) in &probe.named_json_roots {
        add_named_json_signature(root, basename, &mut signature.0, &mut signature.1);
    }
    signature
}

fn jsonl_signature(
    roots: &[PathBuf],
    json_roots: &[PathBuf],
    log_roots: &[PathBuf],
) -> (u64, u128) {
    let mut count = 0u64;
    let mut latest = 0u128;
    for root in roots {
        add_signature(root, "jsonl", &mut count, &mut latest);
    }
    for root in json_roots {
        add_signature(root, "json", &mut count, &mut latest);
    }
    for root in log_roots {
        add_signature(root, "log", &mut count, &mut latest);
    }
    (count, latest)
}

fn add_named_json_signature(root: &Path, basename: &str, count: &mut u64, latest: &mut u128) {
    add_selected_signature(
        root,
        |path| path.file_name().and_then(|name| name.to_str()) == Some(basename),
        count,
        latest,
    );
}

fn add_signature(root: &Path, extension: &str, count: &mut u64, latest: &mut u128) {
    add_selected_signature(
        root,
        |path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        },
        count,
        latest,
    );
}

fn add_selected_signature(
    root: &Path,
    selected: impl Fn(&Path) -> bool,
    count: &mut u64,
    latest: &mut u128,
) {
    if !root.is_dir() {
        return;
    }
    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !entry.file_type().is_file() || !selected(path) {
            continue;
        }
        *count += 1;
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let Ok(duration) = modified.duration_since(UNIX_EPOCH) else {
            continue;
        };
        *latest = (*latest).max(duration.as_nanos());
    }
}
