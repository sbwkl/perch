use std::fs;
use std::path::Path;

use perch_core::models::{SourceResult, TaskSnapshot};
use perch_core::query::{build_bootstrap, build_bootstrap_with_usage};
use perch_core::store::CacheStore;
use perch_core::usage::{SourceUsageSummary, UsageOverview, UsageSnapshot};
use rusqlite::Connection;
use tempfile::TempDir;

fn ts(value: &str) -> f64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .unwrap()
        .timestamp() as f64
}

fn source<'a>(overview: &'a UsageOverview, id: &str) -> &'a SourceUsageSummary {
    overview
        .sources
        .iter()
        .find(|item| item.source == id)
        .unwrap_or_else(|| panic!("missing usage summary for {id}"))
}

fn make_usage_db(path: &Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE usage_turn (
          id TEXT PRIMARY KEY,
          source TEXT NOT NULL,
          day TEXT NOT NULL,
          session_id TEXT,
          model TEXT NOT NULL,
          input_tokens INTEGER NOT NULL DEFAULT 0,
          output_tokens INTEGER NOT NULL DEFAULT 0,
          cache_read INTEGER NOT NULL DEFAULT 0,
          cache_write INTEGER NOT NULL DEFAULT 0,
          reasoning INTEGER NOT NULL DEFAULT 0,
          archived INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE usage_daily (
          day TEXT NOT NULL, source TEXT NOT NULL, model TEXT NOT NULL,
          project TEXT NOT NULL DEFAULT '', session_id TEXT NOT NULL DEFAULT '',
          input_tokens INTEGER NOT NULL DEFAULT 0, output_tokens INTEGER NOT NULL DEFAULT 0,
          cache_read INTEGER NOT NULL DEFAULT 0, cache_write INTEGER NOT NULL DEFAULT 0,
          reasoning INTEGER NOT NULL DEFAULT 0, turn_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE usage_credit (
          request_id TEXT PRIMARY KEY, ts REAL NOT NULL, day TEXT NOT NULL,
          project TEXT NOT NULL DEFAULT '', model TEXT NOT NULL, session_id TEXT,
          credits REAL NOT NULL DEFAULT 0, billable INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE usage_est (
          request_id TEXT PRIMARY KEY, source TEXT NOT NULL DEFAULT 'qoder.est',
          ts REAL NOT NULL, day TEXT NOT NULL, project TEXT NOT NULL DEFAULT '',
          model TEXT NOT NULL, session_id TEXT,
          est_input INTEGER NOT NULL DEFAULT 0, est_output INTEGER NOT NULL DEFAULT 0
        );
        "#,
    )
    .unwrap();

    conn.execute(
        "INSERT INTO usage_turn (id, source, day, session_id, model, input_tokens, output_tokens, cache_read, cache_write, reasoning, archived) \
         VALUES ('t1', 'claude', '2026-01-05', 's1', 'claude-sonnet', 100, 20, 5, 3, 2, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO usage_turn (id, source, day, session_id, model, input_tokens, output_tokens, cache_read, cache_write, reasoning, archived) \
         VALUES ('t2', 'claude', '2026-01-06', 's1', 'claude-opus', 50, 10, 0, 0, 0, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO usage_turn (id, source, day, session_id, model, input_tokens, output_tokens, cache_read, cache_write, reasoning, archived) \
         VALUES ('t3', 'claude', '2026-01-06', 's1', 'claude-opus', 999, 999, 0, 0, 0, 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO usage_turn (id, source, day, session_id, model, input_tokens, output_tokens, cache_read, cache_write, reasoning, archived) \
         VALUES ('t4', 'codex', '2026-01-06', 's2', 'gpt-5-codex', 10, 0, 0, 0, 0, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO usage_turn (id, source, day, session_id, model, input_tokens, output_tokens, cache_read, cache_write, reasoning, archived) \
         VALUES ('t5', 'claude', '2025-12-20', 's4', 'claude-sonnet', 1000, 0, 0, 0, 0, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO usage_daily (day, source, model, project, session_id, input_tokens, output_tokens, cache_read, cache_write, reasoning, turn_count) \
         VALUES ('2025-12-01', 'claude', 'claude-sonnet', 'alpha', 's3', 7, 0, 0, 0, 0, 2)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO usage_credit (request_id, ts, day, project, model, session_id, credits, billable) \
         VALUES ('c1', 0, '2026-01-06', 'alpha', 'qwen3.5', 's1', 2.5, 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO usage_credit (request_id, ts, day, project, model, session_id, credits, billable) \
         VALUES ('c2', 0, '2026-01-06', 'alpha', 'qwen3.5', 's1', 0.5, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO usage_est (request_id, source, ts, day, project, model, session_id, est_input, est_output) \
         VALUES ('e1', 'qoder.est', 0, '2026-01-06', 'alpha', 'qwen3.5', 's1', 1000, 200)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO usage_est (request_id, source, ts, day, project, model, session_id, est_input, est_output) \
         VALUES ('e2', 'trae-cn.est', 0, '2026-01-07', 'beta', 'trae-model', 't1', 250, 50)",
        [],
    )
    .unwrap();
}

fn make_task(source: &str, id: &str) -> TaskSnapshot {
    let mut task = TaskSnapshot::new(source, id);
    task.title = format!("任务 {id}");
    task.title_source = "source".to_owned();
    task.project_path = Some("/Users/x/work/alpha".to_owned());
    task.project_name = Some("alpha".to_owned());
    task.updated_at = Some(ts("2026-01-06T12:00:00Z"));
    task.status_source = Some("idle".to_owned());
    task.refresh_fingerprint();
    task
}

fn seed_tasks(store: &CacheStore) {
    let scan = store.begin_scan("all", "full", "source", "test").unwrap();
    for (source, id) in [("claude", "s1"), ("codex", "s2"), ("qoder", "s1")] {
        store
            .save_result(
                &SourceResult::ok(source, vec![make_task(source, id)], Vec::new()),
                scan,
            )
            .unwrap();
    }
}

#[test]
fn usage_maps_sessions_and_keeps_tracks_separate() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("usage.db");
    make_usage_db(&db);

    let now = ts("2026-01-08T12:00:00Z");
    let snapshot = UsageSnapshot::try_load(&db, now).unwrap().unwrap();

    let claude_summary = source(&snapshot.overview, "claude");
    assert_eq!(claude_summary.token_total, 1_197);
    assert_eq!(claude_summary.token_7d, 190);
    assert_eq!(claude_summary.credits_billable_7d, 0.0);
    assert_eq!(claude_summary.est_total, 0);
    assert_eq!(claude_summary.last_day.as_deref(), Some("2026-01-06"));

    let codex_summary = source(&snapshot.overview, "codex");
    assert_eq!(codex_summary.token_total, 10);
    assert_eq!(codex_summary.token_7d, 10);
    assert_eq!(codex_summary.last_day.as_deref(), Some("2026-01-06"));

    // Credits and estimates belong to their own source and never appear as
    // token truth.
    let qoder_summary = source(&snapshot.overview, "qoder");
    assert_eq!(qoder_summary.token_total, 0);
    assert_eq!(qoder_summary.token_7d, 0);
    assert_eq!(qoder_summary.credits_billable_7d, 2.5);
    assert_eq!(qoder_summary.est_total, 1_200);
    assert_eq!(qoder_summary.last_day.as_deref(), Some("2026-01-06"));

    // `trae-cn.est` has no Perch tasks, so it never becomes a source summary.
    assert!(snapshot
        .overview
        .sources
        .iter()
        .all(|item| item.source != "trae-cn"));
    assert_eq!(snapshot.overview.sources.len(), 3);

    let claude = snapshot.task_usage("claude", "s1").unwrap();
    let token = claude.token.unwrap();
    assert_eq!(token.input, 150);
    assert_eq!(token.output, 30);
    assert_eq!(token.cache_read, 5);
    assert_eq!(token.cache_write, 3);
    assert_eq!(token.reasoning, 2);
    assert_eq!(token.total, 190);
    assert_eq!(token.turns, 2);
    assert_eq!(token.models, vec!["claude-opus", "claude-sonnet"]);
    assert_eq!(token.last_day.as_deref(), Some("2026-01-06"));
    assert!(claude.credits.is_none());
    assert!(claude.est.is_none());

    let codex = snapshot.task_usage("codex", "s2").unwrap();
    assert_eq!(codex.token.unwrap().total, 10);
    assert!(snapshot.task_usage("claude", "s3").is_some());
    assert!(snapshot.task_usage("claude", "s4").is_some());

    let qoder = snapshot.task_usage("qoder", "s1").unwrap();
    assert!(
        qoder.token.is_none(),
        "credits/est must not be folded into token truth"
    );
    let credits = qoder.credits.unwrap();
    assert_eq!(credits.billable, 2.5);
    assert_eq!(credits.free, 0.5);
    assert_eq!(credits.billable_requests, 1);
    assert_eq!(credits.free_requests, 1);
    let est = qoder.est.unwrap();
    assert_eq!(est.source, "qoder.est");
    assert_eq!(est.total, 1_200);
    assert_eq!(est.requests, 1);
    assert!(snapshot.task_usage("trae-cn", "t1").is_none());
}

#[test]
fn bootstrap_attaches_usage_and_reports_matches() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("usage.db");
    make_usage_db(&db);
    let now = ts("2026-01-08T12:00:00Z");
    let snapshot = UsageSnapshot::try_load(&db, now).unwrap().unwrap();

    let store = CacheStore::open_in_memory().unwrap();
    seed_tasks(&store);

    let plain = build_bootstrap(&store, now).unwrap();
    assert!(plain.usage.is_none());
    assert!(plain.workspaces[0]
        .tasks
        .iter()
        .all(|task| task.usage.is_none()));

    let payload = build_bootstrap_with_usage(&store, now, Some(&snapshot)).unwrap();
    let overview = payload.usage.unwrap();
    assert_eq!(overview.sources.len(), 3);
    assert_eq!(source(&overview, "claude").matched_tasks, 1);
    assert_eq!(source(&overview, "claude").token_7d, 190);
    assert_eq!(source(&overview, "codex").matched_tasks, 1);
    assert_eq!(source(&overview, "qoder").matched_tasks, 1);
    assert_eq!(source(&overview, "qoder").credits_billable_7d, 2.5);
    assert_eq!(source(&overview, "qoder").token_7d, 0);

    let mut found = 0;
    for group in &payload.workspaces {
        for task in &group.tasks {
            if let Some(usage) = &task.usage {
                found += 1;
                match task.source.as_str() {
                    "claude" => assert_eq!(usage.token.as_ref().unwrap().total, 190),
                    "codex" => assert_eq!(usage.token.as_ref().unwrap().total, 10),
                    "qoder" => {
                        assert!(usage.token.is_none());
                        assert!(usage.credits.is_some());
                        assert!(usage.est.is_some());
                    }
                    other => panic!("unexpected source {other}"),
                }
            }
        }
    }
    assert_eq!(found, 3);
}

#[test]
fn legacy_est_schema_without_source_is_supported() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("legacy.db");
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE usage_turn (
          source TEXT NOT NULL, day TEXT NOT NULL, session_id TEXT,
          model TEXT NOT NULL, input_tokens INTEGER NOT NULL DEFAULT 0,
          output_tokens INTEGER NOT NULL DEFAULT 0, cache_read INTEGER NOT NULL DEFAULT 0,
          cache_write INTEGER NOT NULL DEFAULT 0, reasoning INTEGER NOT NULL DEFAULT 0,
          archived INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE usage_est (
          request_id TEXT PRIMARY KEY, ts REAL NOT NULL, day TEXT NOT NULL,
          project TEXT NOT NULL DEFAULT '', model TEXT NOT NULL, session_id TEXT,
          est_input INTEGER NOT NULL DEFAULT 0, est_output INTEGER NOT NULL DEFAULT 0
        );
        INSERT INTO usage_est (request_id, ts, day, project, model, session_id, est_input, est_output)
        VALUES ('e1', 0, '2026-01-06', 'alpha', 'qwen3.5', 's1', 300, 40);
        "#,
    )
    .unwrap();

    let now = ts("2026-01-08T12:00:00Z");
    let snapshot = UsageSnapshot::try_load(&db, now).unwrap().unwrap();
    let est = snapshot.task_usage("qoder", "s1").unwrap().est.unwrap();
    assert_eq!(est.source, "qoder.est");
    assert_eq!(est.total, 340);
    assert_eq!(source(&snapshot.overview, "qoder").est_total, 340);
}

#[test]
fn missing_or_incompatible_database_is_ignored() {
    let temp = TempDir::new().unwrap();
    let now = ts("2026-01-08T12:00:00Z");

    let missing = temp.path().join("missing.db");
    assert!(UsageSnapshot::try_load(&missing, now).unwrap().is_none());

    let empty = temp.path().join("empty.db");
    Connection::open(&empty)
        .unwrap()
        .execute_batch("CREATE TABLE other (x INTEGER);")
        .unwrap();
    assert!(UsageSnapshot::try_load(&empty, now).unwrap().is_none());

    let broken = temp.path().join("broken.db");
    fs::write(&broken, "not a sqlite database").unwrap();
    assert!(UsageSnapshot::load(&broken, now).is_none());

    // Without usage-ledger the integration disappears completely: no overview,
    // no Inspector section, and no `usage` key in the serialized payload, so
    // the UI never has to render an empty state for a tool that is not there.
    let store = CacheStore::open_in_memory().unwrap();
    seed_tasks(&store);
    let snapshot = UsageSnapshot::load(&missing, now);
    let payload = build_bootstrap_with_usage(&store, now, snapshot.as_ref()).unwrap();
    assert!(payload.usage.is_none());
    let json = serde_json::to_value(&payload).unwrap();
    assert!(json.get("usage").is_none());
}
