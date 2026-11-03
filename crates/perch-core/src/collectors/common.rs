use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::{DateTime, Local};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::error::{PerchError, Result};

pub fn open_read_only(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.execute_batch("PRAGMA query_only=ON; PRAGMA busy_timeout=5000;")?;
    Ok(conn)
}

pub fn epoch_ms_to_seconds(value: Option<i64>) -> Option<f64> {
    value.map(|millis| millis as f64 / 1000.0)
}

pub fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )
        .ok();
    Ok(found.is_some())
}

fn source_home() -> Option<PathBuf> {
    std::env::var_os("PERCH_SOURCE_HOME")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

pub fn is_generated_sandbox_path(path: &str, agent_dir: &str) -> bool {
    let normalized = normalize_project_path(path);
    let Some(home) = source_home() else {
        return false;
    };
    let base = home.join("Documents").join(agent_dir);
    let Ok(relative) = Path::new(&normalized).strip_prefix(&base) else {
        return false;
    };
    let mut components = relative.components();
    let (Some(date), Some(leaf), None) = (components.next(), components.next(), components.next())
    else {
        return false;
    };
    is_iso_date(&date.as_os_str().to_string_lossy()) && !leaf.as_os_str().is_empty()
}

pub fn is_default_project_path(path: &str) -> bool {
    let normalized = normalize_project_path(path);
    source_home().is_some_and(|home| {
        Path::new(&normalized) == home.join("Documents").join("Default Project")
    })
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(index, byte)| {
            if index == 4 || index == 7 {
                *byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
}

pub fn project_name(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

pub fn normalize_project_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let expanded = if let Some(rest) = trimmed.strip_prefix("~/") {
        match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home)
                .join(rest)
                .to_string_lossy()
                .into_owned(),
            None => trimmed.to_owned(),
        }
    } else {
        trimmed.to_owned()
    };
    expanded.trim_end_matches('/').to_owned()
}

pub fn fallback_title(project_name: Option<&str>, created_at: Option<f64>) -> String {
    let project = project_name
        .filter(|name| !name.is_empty())
        .unwrap_or("未知项目");
    let date = created_at
        .and_then(|seconds| DateTime::from_timestamp(seconds as i64, 0))
        .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "未知日期".to_owned());
    format!("未命名 · {project} · {date}")
}

pub fn shell_cd_command(directory: Option<&str>, command: &str) -> Option<String> {
    let directory = directory.filter(|value| !value.trim().is_empty())?;
    let quoted = directory.replace('\'', "'\\''");
    Some(format!("cd '{quoted}' && {command}"))
}

pub fn truncate_chars(value: &str, limit: usize) -> String {
    let mut output = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= limit {
            break;
        }
        output.push(ch);
    }
    output
}

pub fn map_goal_status(raw: &str) -> (Option<String>, Option<String>) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (None, None);
    }
    let lowered = trimmed.to_ascii_lowercase();
    let mapped = match lowered.as_str() {
        "running" | "active" => Some("active"),
        "paused" => Some("paused"),
        "blocked" => Some("blocked"),
        "usage_limited" | "budget_limited" | "limited" => Some("limited"),
        "complete" | "completed" | "done" => Some("complete"),
        "archived" => Some("archived"),
        _ => None,
    };
    match mapped {
        Some(value) => (Some(value.to_owned()), Some(trimmed.to_owned())),
        None => (None, Some(trimmed.to_owned())),
    }
}

pub fn parse_timestamp_value(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64().map(|raw| {
            if raw > 10_000_000_000.0 {
                raw / 1000.0
            } else {
                raw
            }
        }),
        Value::String(text) => parse_timestamp_str(text),
        _ => None,
    }
}

pub fn parse_timestamp_str(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(number) = trimmed.parse::<f64>() {
        return Some(if number > 10_000_000_000.0 {
            number / 1000.0
        } else {
            number
        });
    }
    DateTime::parse_from_rfc3339(trimmed)
        .ok()
        .map(|dt| dt.timestamp_millis() as f64 / 1000.0)
}

pub fn mtime_seconds(path: &Path) -> Option<f64> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs_f64())
}

pub fn read_first_json_line(path: &Path) -> Result<Option<Value>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return Ok(None);
        }
        if line.trim().is_empty() {
            continue;
        }
        return match serde_json::from_str::<Value>(&line) {
            Ok(value) => Ok(Some(value)),
            Err(error) => Err(PerchError::Message(format!(
                "invalid JSON in {}: {error}",
                path.display()
            ))),
        };
    }
}

pub fn json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn json_bool(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub fn is_jsonl(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
}

pub fn read_jsonl_values(path: &Path) -> Result<Vec<Value>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut values = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(value) => values.push(value),
            Err(_) => continue,
        }
    }
    Ok(values)
}

pub fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn is_recent(updated_at: Option<f64>, seen_at: f64, now: f64, window_seconds: f64) -> bool {
    updated_at.unwrap_or(seen_at) >= now - window_seconds
}

pub fn path_key(path: &str) -> String {
    normalize_project_path(path)
}
