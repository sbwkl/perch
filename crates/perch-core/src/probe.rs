use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use rusqlite::Connection;
use walkdir::WalkDir;

use crate::collectors::common::{is_jsonl, open_read_only};
use crate::paths::SourcePaths;

pub struct SourceProbe {
    sqlite: Vec<SqliteProbe>,
    jsonl: Vec<JsonlProbe>,
}

struct SqliteProbe {
    source: &'static str,
    conn: Option<Connection>,
    last_version: i64,
}

struct JsonlProbe {
    source: &'static str,
    roots: Vec<PathBuf>,
    last_signature: (u64, u128),
}

impl SourceProbe {
    pub fn new(paths: &SourcePaths) -> Self {
        let opencode_db = SourcePaths::first_existing(&[
            paths.opencode_db.clone(),
            paths.opencode_stable_db.clone(),
        ]);
        let sqlite = [
            ("qoder", Some(paths.qoder_db.clone())),
            ("opencode", opencode_db),
            ("kilo", Some(paths.kilo_db.clone())),
        ]
        .into_iter()
        .map(|(source, path)| {
            let conn = path.as_deref().and_then(|path| open_read_only(path).ok());
            let last_version = conn.as_ref().and_then(data_version).unwrap_or(0);
            SqliteProbe {
                source,
                conn,
                last_version,
            }
        })
        .collect();

        let jsonl = vec![
            JsonlProbe {
                source: "claude",
                roots: vec![paths.claude_dir.join("projects")],
                last_signature: (0, 0),
            },
            JsonlProbe {
                source: "codex",
                roots: vec![
                    paths.codex_dir.join("sessions"),
                    paths.codex_dir.join("archived_sessions"),
                ],
                last_signature: (0, 0),
            },
            JsonlProbe {
                source: crate::models::SOURCE_TRAE_CN,
                roots: vec![
                    paths.trae_cn_events_dir.clone(),
                    paths.trae_cn_memory_dir.clone(),
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
            let Some(conn) = probe.conn.as_ref() else {
                continue;
            };
            let Some(version) = data_version(conn) else {
                continue;
            };
            if version != probe.last_version {
                probe.last_version = version;
                changed.push(probe.source);
            }
        }

        for probe in &mut self.jsonl {
            let signature = jsonl_signature(&probe.roots);
            if signature != probe.last_signature {
                probe.last_signature = signature;
                changed.push(probe.source);
            }
        }

        changed
    }

    fn reset_jsonl_baseline(&mut self) {
        for probe in &mut self.jsonl {
            probe.last_signature = jsonl_signature(&probe.roots);
        }
    }
}

fn data_version(conn: &Connection) -> Option<i64> {
    conn.query_row("PRAGMA data_version", [], |row| row.get(0))
        .ok()
}

fn jsonl_signature(roots: &[PathBuf]) -> (u64, u128) {
    let mut count = 0u64;
    let mut latest = 0u128;
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() || !is_jsonl(entry.path()) {
                continue;
            }
            count += 1;
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            let Ok(duration) = modified.duration_since(UNIX_EPOCH) else {
                continue;
            };
            latest = latest.max(duration.as_nanos());
        }
    }
    (count, latest)
}
