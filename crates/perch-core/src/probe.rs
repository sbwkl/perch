use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::Connection;
use walkdir::WalkDir;

use crate::collectors::common::open_read_only;
use crate::paths::SourcePaths;

pub struct SourceProbe {
    sqlite: Vec<SqliteProbe>,
    jsonl: Vec<JsonlProbe>,
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
                last_signature: (0, 0),
            },
            JsonlProbe {
                source: crate::models::SOURCE_MINIMAX,
                roots: vec![paths.minimax_dir.join("v2/sessions")],
                json_roots: Vec::new(),
                log_roots: Vec::new(),
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
                last_signature: (0, 0),
            },
        ];
        let mut probe = Self { sqlite, jsonl };
        probe.reset_jsonl_baseline();
        probe
    }

    pub fn poll(&mut self) -> Vec<&'static str> {
        let mut changed = Vec::new();

        for probe in &mut self.sqlite {
            if probe.conn.is_none() {
                // The database may appear after startup (agent installed later);
                // keep retrying instead of staying deaf until relaunch.
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
                // Unreadable or deleted database: drop the handle so the next
                // poll can reopen a replacement.
                probe.conn = None;
                continue;
            };
            if version != probe.last_version {
                probe.last_version = version;
                changed.push(probe.source);
            }
        }

        for probe in &mut self.jsonl {
            let signature = jsonl_signature(&probe.roots, &probe.json_roots, &probe.log_roots);
            if signature != probe.last_signature {
                probe.last_signature = signature;
                changed.push(probe.source);
            }
        }

        changed
    }

    fn reset_jsonl_baseline(&mut self) {
        for probe in &mut self.jsonl {
            probe.last_signature =
                jsonl_signature(&probe.roots, &probe.json_roots, &probe.log_roots);
        }
    }
}

fn data_version(conn: &Connection) -> Option<i64> {
    conn.query_row("PRAGMA data_version", [], |row| row.get(0))
        .ok()
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

fn add_signature(root: &Path, extension: &str, count: &mut u64, latest: &mut u128) {
    if !root.is_dir() {
        return;
    }
    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        if !entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        {
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
