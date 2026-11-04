//! Optional read-only bridge to usage-ledger's `usage.db`.
//!
//! usage-ledger is a separate local tool. Perch never writes to its database;
//! this module only opens it read-only and projects per-session totals into the
//! task list. The four usage tracks (token truth, activity, credits, estimates)
//! stay separate and are never summed together.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{Duration as ChronoDuration, Local, TimeZone};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::Result;

/// usage-ledger's hourly schedule keeps the database fresh; if it has not been
/// touched for longer than this, the UI marks the snapshot as possibly stale.
const STALE_AFTER_SECONDS: f64 = 6.0 * 3600.0;

/// Token truth exists for these sources. Qoder only has historical token data;
/// its newer desktop sessions expose activity / credits / estimates instead.
const PERCH_SOURCES: [&str; 5] = ["qoder", "claude", "codex", "opencode", "kilo"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
    pub total: i64,
    pub turns: i64,
    pub models: Vec<String>,
    pub last_day: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreditUsage {
    /// Server-side billable credits, not tokens.
    pub billable: f64,
    pub free: f64,
    pub billable_requests: i64,
    pub free_requests: i64,
    pub last_day: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EstUsage {
    /// `qoder.est` / `trae-cn.est`; estimates are never added to token truth.
    pub source: String,
    pub input: i64,
    pub output: i64,
    pub total: i64,
    pub requests: i64,
    pub last_day: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<TokenUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits: Option<CreditUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub est: Option<EstUsage>,
}

/// Totals for a single source. Units differ per source: token truth and
/// estimates are token counts, credits are server-side billing. They are never
/// combined, and sources are never summed because their units disagree.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceUsageSummary {
    pub source: String,
    pub token_total: i64,
    pub token_7d: i64,
    pub credits_billable_7d: f64,
    pub est_total: i64,
    pub last_day: Option<String>,
    /// Filled in by `query` once tasks are matched.
    pub matched_tasks: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageOverview {
    /// Database file mtime (seconds); WAL writes are included.
    pub as_of: Option<f64>,
    pub stale: bool,
    /// Only sources that matched at least one task, sorted by source id.
    pub sources: Vec<SourceUsageSummary>,
}

#[derive(Debug, Clone, Default)]
pub struct UsageSnapshot {
    pub overview: UsageOverview,
    by_session: HashMap<(String, String), TaskUsage>,
}

impl UsageSnapshot {
    /// Best-effort load: a missing, locked, or incompatible database simply
    /// hides the usage section instead of failing `bootstrap`.
    pub fn load(path: &Path, now: f64) -> Option<Self> {
        Self::try_load(path, now).ok().flatten()
    }

    pub fn try_load(path: &Path, now: f64) -> Result<Option<Self>> {
        if !path.is_file() {
            return Ok(None);
        }
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.busy_timeout(Duration::from_millis(300))?;
        conn.execute_batch("PRAGMA query_only=ON;")?;

        if !table_exists(&conn, "usage_turn")?
            || !has_columns(
                &conn,
                "usage_turn",
                &[
                    "source",
                    "session_id",
                    "day",
                    "model",
                    "input_tokens",
                    "output_tokens",
                    "cache_read",
                    "cache_write",
                    "reasoning",
                ],
            )?
        {
            return Ok(None);
        }

        let cutoff = cutoff_day(now);
        let mut by_source: HashMap<String, SourceUsageSummary> = HashMap::new();
        let mut tokens: HashMap<(String, String), TokenAccum> = HashMap::new();

        let archived_clause = if has_column(&conn, "usage_turn", "archived")? {
            "AND archived = 0"
        } else {
            ""
        };
        for row in load_turn_rows(&conn, "usage_turn", "1", archived_clause)? {
            add_token_row(&mut tokens, &mut by_source, &cutoff, row);
        }
        if table_exists(&conn, "usage_daily")?
            && has_columns(
                &conn,
                "usage_daily",
                &[
                    "source",
                    "session_id",
                    "day",
                    "model",
                    "input_tokens",
                    "output_tokens",
                    "cache_read",
                    "cache_write",
                    "reasoning",
                    "turn_count",
                ],
            )?
        {
            for row in load_turn_rows(&conn, "usage_daily", "turn_count", "")? {
                add_token_row(&mut tokens, &mut by_source, &cutoff, row);
            }
        }

        let mut by_session: HashMap<(String, String), TaskUsage> = HashMap::new();
        for (key, accum) in tokens {
            by_session.entry(key).or_default().token = Some(accum.into_usage());
        }

        load_credits(&conn, &mut by_session, &mut by_source, &cutoff)?;
        load_estimates(&conn, &mut by_session, &mut by_source)?;

        let as_of = newest_mtime(path);
        let stale = as_of.is_none_or(|stamp| now - stamp > STALE_AFTER_SECONDS);
        let mut sources: Vec<SourceUsageSummary> = by_source.into_values().collect();
        sources.sort_by(|a, b| a.source.cmp(&b.source));

        Ok(Some(Self {
            overview: UsageOverview {
                as_of,
                stale,
                sources,
            },
            by_session,
        }))
    }

    pub fn task_usage(&self, source: &str, session_id: &str) -> Option<TaskUsage> {
        if session_id.is_empty() {
            return None;
        }
        self.by_session
            .get(&(source.to_owned(), session_id.to_owned()))
            .cloned()
    }
}

#[derive(Default)]
struct TokenAccum {
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
    turns: i64,
    models: BTreeSet<String>,
    last_day: Option<String>,
}

impl TokenAccum {
    fn add(&mut self, row: &TokenRow) {
        self.input += row.input;
        self.output += row.output;
        self.cache_read += row.cache_read;
        self.cache_write += row.cache_write;
        self.reasoning += row.reasoning;
        self.turns += row.turns;
        if !row.model.is_empty() {
            self.models.insert(row.model.clone());
        }
        update_day(&mut self.last_day, &row.day);
    }

    fn into_usage(self) -> TokenUsage {
        let total = self.input + self.output + self.cache_read + self.cache_write + self.reasoning;
        TokenUsage {
            input: self.input,
            output: self.output,
            cache_read: self.cache_read,
            cache_write: self.cache_write,
            reasoning: self.reasoning,
            total,
            turns: self.turns,
            models: self.models.into_iter().collect(),
            last_day: self.last_day,
        }
    }
}

struct TokenRow {
    source: String,
    session_id: String,
    day: String,
    model: String,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
    turns: i64,
}

fn load_turn_rows(
    conn: &Connection,
    table: &str,
    turns_expr: &str,
    archived_clause: &str,
) -> Result<Vec<TokenRow>> {
    let sql = format!(
        "SELECT source, session_id, day, model, input_tokens, output_tokens, \
         cache_read, cache_write, reasoning, {turns_expr} \
         FROM {table} \
         WHERE session_id IS NOT NULL AND session_id <> '' {archived_clause}"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        Ok(TokenRow {
            source: row.get(0)?,
            session_id: row.get(1)?,
            day: row.get(2)?,
            model: row.get(3)?,
            input: row.get(4)?,
            output: row.get(5)?,
            cache_read: row.get(6)?,
            cache_write: row.get(7)?,
            reasoning: row.get(8)?,
            turns: row.get(9)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn source_entry<'a>(
    by_source: &'a mut HashMap<String, SourceUsageSummary>,
    source: &str,
) -> &'a mut SourceUsageSummary {
    by_source
        .entry(source.to_owned())
        .or_insert_with(|| SourceUsageSummary {
            source: source.to_owned(),
            ..Default::default()
        })
}

fn add_token_row(
    tokens: &mut HashMap<(String, String), TokenAccum>,
    by_source: &mut HashMap<String, SourceUsageSummary>,
    cutoff: &str,
    row: TokenRow,
) {
    if !PERCH_SOURCES.contains(&row.source.as_str()) || row.session_id.is_empty() {
        return;
    }
    let total = row.input + row.output + row.cache_read + row.cache_write + row.reasoning;
    let entry = source_entry(by_source, &row.source);
    entry.token_total += total;
    if row.day.as_str() >= cutoff {
        entry.token_7d += total;
    }
    update_day(&mut entry.last_day, &row.day);
    tokens
        .entry((row.source.clone(), row.session_id.clone()))
        .or_default()
        .add(&row);
}

fn load_credits(
    conn: &Connection,
    by_session: &mut HashMap<(String, String), TaskUsage>,
    by_source: &mut HashMap<String, SourceUsageSummary>,
    cutoff: &str,
) -> Result<()> {
    if !table_exists(conn, "usage_credit")?
        || !has_columns(
            conn,
            "usage_credit",
            &["session_id", "day", "credits", "billable"],
        )?
    {
        return Ok(());
    }

    let mut statement = conn.prepare(
        "SELECT session_id, \
         COALESCE(SUM(CASE WHEN billable = 1 THEN credits ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN billable = 0 THEN credits ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN billable = 1 THEN 1 ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN billable = 0 THEN 1 ELSE 0 END), 0), \
         MAX(day) \
         FROM usage_credit \
         WHERE session_id IS NOT NULL AND session_id <> '' \
         GROUP BY session_id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    })?;

    for row in rows {
        let (session, billable, free, billable_requests, free_requests, day) = row?;
        let day = day.unwrap_or_default();
        if day.is_empty() {
            continue;
        }
        let qoder = source_entry(by_source, "qoder");
        if day.as_str() >= cutoff {
            qoder.credits_billable_7d += billable;
        }
        update_day(&mut qoder.last_day, &day);
        let entry = by_session
            .entry(("qoder".to_owned(), session))
            .or_default()
            .credits
            .get_or_insert_with(CreditUsage::default);
        entry.billable += billable;
        entry.free += free;
        entry.billable_requests += billable_requests;
        entry.free_requests += free_requests;
        update_day(&mut entry.last_day, &day);
    }
    Ok(())
}

fn load_estimates(
    conn: &Connection,
    by_session: &mut HashMap<(String, String), TaskUsage>,
    by_source: &mut HashMap<String, SourceUsageSummary>,
) -> Result<()> {
    if !table_exists(conn, "usage_est")?
        || !has_columns(
            conn,
            "usage_est",
            &["session_id", "day", "est_input", "est_output"],
        )?
    {
        return Ok(());
    }

    // Older archives predate the `source` column; usage-ledger migrates them
    // with `qoder.est` as the default, so treat the stored rows the same way.
    let source_expr = if has_column(conn, "usage_est", "source")? {
        "source"
    } else {
        "'qoder.est'"
    };
    let sql = format!(
        "SELECT {source_expr}, session_id, \
         COALESCE(SUM(est_input), 0), COALESCE(SUM(est_output), 0), \
         COUNT(*), MAX(day) \
         FROM usage_est \
         WHERE session_id IS NOT NULL AND session_id <> '' \
         GROUP BY {source_expr}, session_id"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    })?;

    for row in rows {
        let (source, session, input, output, requests, day) = row?;
        let day = day.unwrap_or_default();
        if day.is_empty() {
            continue;
        }
        let base_source = source.strip_suffix(".est").unwrap_or(&source);
        if !PERCH_SOURCES.contains(&base_source) {
            continue;
        }
        let totals = source_entry(by_source, base_source);
        totals.est_total += input + output;
        update_day(&mut totals.last_day, &day);

        let entry = by_session
            .entry((base_source.to_owned(), session))
            .or_default();
        match entry.est.as_mut() {
            Some(existing) => {
                existing.input += input;
                existing.output += output;
                existing.total = existing.input + existing.output;
                existing.requests += requests;
                if existing.source != source {
                    existing.source = "mixed".to_owned();
                }
                update_day(&mut existing.last_day, &day);
            }
            None => {
                entry.est = Some(EstUsage {
                    source,
                    input,
                    output,
                    total: input + output,
                    requests,
                    last_day: Some(day),
                });
            }
        }
    }
    Ok(())
}

fn cutoff_day(now: f64) -> String {
    // usage-ledger writes local calendar days, so anchor the 7-day window the
    // same way instead of using UTC.
    Local
        .timestamp_opt(now as i64, 0)
        .single()
        .map(|stamp| (stamp - ChronoDuration::days(7)).date_naive().to_string())
        .unwrap_or_else(|| "9999-12-31".to_owned())
}

fn update_day(target: &mut Option<String>, day: &str) {
    if day.is_empty() {
        return;
    }
    if target.as_deref().is_none_or(|current| day > current) {
        *target = Some(day.to_owned());
    }
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    has_columns(conn, table, &[column])
}

fn has_columns(conn: &Connection, table: &str, columns: &[&str]) -> Result<bool> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns
        .iter()
        .all(|column| names.iter().any(|name| name == column)))
}

fn newest_mtime(path: &Path) -> Option<f64> {
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    [path.to_path_buf(), wal]
        .iter()
        .filter_map(|candidate| std::fs::metadata(candidate).ok()?.modified().ok())
        .max()
        .and_then(|stamp: SystemTime| stamp.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs_f64())
}
