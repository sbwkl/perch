use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::models::{SourceResult, SourceSyncState, TaskSnapshot, TodoSnapshot};
use crate::paths::ensure_parent;

const SCAN_RUNS_KEPT_PER_SOURCE: i64 = 200;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS task (
  key             TEXT PRIMARY KEY,
  source          TEXT NOT NULL,
  source_id       TEXT NOT NULL,
  parent_key      TEXT,
  title           TEXT NOT NULL,
  title_source    TEXT NOT NULL,
  project_path    TEXT,
  project_name    TEXT,
  created_at      REAL,
  updated_at      REAL,
  status_source   TEXT,
  status_raw      TEXT,
  source_archived INTEGER NOT NULL DEFAULT 0,
  source_unread   INTEGER NOT NULL DEFAULT 0,
  source_pinned   INTEGER NOT NULL DEFAULT 0,
  source_missing  INTEGER NOT NULL DEFAULT 0,
  model           TEXT,
  git_branch      TEXT,
  agent_kind      TEXT,
  message_count   INTEGER,
  todo_total      INTEGER,
  todo_done       INTEGER,
  deeplink        TEXT,
  resume_cmd      TEXT,
  source_path     TEXT,
  fingerprint     TEXT,
  seen_at         REAL NOT NULL,
  last_seen_run   INTEGER,
  UNIQUE(source, source_id)
);
CREATE INDEX IF NOT EXISTS idx_task_updated  ON task(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_task_source   ON task(source);
CREATE INDEX IF NOT EXISTS idx_task_project  ON task(project_name);
CREATE INDEX IF NOT EXISTS idx_task_workspace ON task(project_path, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_task_parent   ON task(parent_key);
CREATE INDEX IF NOT EXISTS idx_task_status   ON task(status_source);
CREATE INDEX IF NOT EXISTS idx_task_missing  ON task(source_missing);

CREATE TABLE IF NOT EXISTS task_todo (
  task_key   TEXT NOT NULL,
  item_id    TEXT NOT NULL,
  subject    TEXT NOT NULL,
  status     TEXT NOT NULL,
  position   INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (task_key, item_id)
);
CREATE INDEX IF NOT EXISTS idx_todo_task ON task_todo(task_key, position);

CREATE TABLE IF NOT EXISTS sync_state (
  source         TEXT PRIMARY KEY,
  last_scan_at   REAL,
  last_ok_at     REAL,
  last_mode      TEXT,
  last_trigger   TEXT,
  status         TEXT NOT NULL,
  files_seen     INTEGER,
  task_count     INTEGER,
  schema_version TEXT,
  error          TEXT
);

CREATE TABLE IF NOT EXISTS scan_run (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  source        TEXT NOT NULL,
  mode          TEXT NOT NULL,
  scope         TEXT,
  trigger       TEXT,
  started_at    REAL NOT NULL,
  finished_at   REAL,
  status        TEXT NOT NULL,
  task_count    INTEGER NOT NULL DEFAULT 0,
  missing_count INTEGER NOT NULL DEFAULT 0,
  error         TEXT
);
CREATE INDEX IF NOT EXISTS idx_scan_run_source ON scan_run(source, started_at DESC);
"#;

/// cache.db 自身 schema 版本（与来源侧 schema_version 无关）。
/// v2：task 增加 source_path 指针列（可重建，删库后重新扫描恢复）。
const CACHE_SCHEMA_VERSION: i64 = 2;

const TASK_UPSERT: &str = r#"
INSERT INTO task (
  key, source, source_id, parent_key, title, title_source, project_path, project_name,
  created_at, updated_at, status_source, status_raw, source_archived, source_unread,
  source_pinned, source_missing, model, git_branch, agent_kind, message_count,
  todo_total, todo_done, deeplink, resume_cmd, source_path, fingerprint, seen_at, last_seen_run
) VALUES (
  ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
  ?9, ?10, ?11, ?12, ?13, ?14,
  ?15, ?16, ?17, ?18, ?19, ?20,
  ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28
)
ON CONFLICT(key) DO UPDATE SET
  parent_key      = excluded.parent_key,
  title           = excluded.title,
  title_source    = excluded.title_source,
  project_path    = excluded.project_path,
  project_name    = excluded.project_name,
  created_at      = excluded.created_at,
  updated_at      = excluded.updated_at,
  status_source   = excluded.status_source,
  status_raw      = excluded.status_raw,
  source_archived = excluded.source_archived,
  source_unread   = excluded.source_unread,
  source_pinned   = excluded.source_pinned,
  source_missing  = 0,
  model           = excluded.model,
  git_branch      = excluded.git_branch,
  agent_kind      = excluded.agent_kind,
  message_count   = excluded.message_count,
  todo_total      = excluded.todo_total,
  todo_done       = excluded.todo_done,
  deeplink        = excluded.deeplink,
  resume_cmd      = excluded.resume_cmd,
  source_path     = excluded.source_path,
  fingerprint     = excluded.fingerprint,
  seen_at         = excluded.seen_at,
  last_seen_run   = excluded.last_seen_run
"#;

pub struct CacheStore {
    conn: Connection,
}

impl CacheStore {
    pub fn open(path: &Path) -> Result<Self> {
        ensure_parent(path)?;
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.init()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.init()?;
        Ok(store)
    }

    fn init(&self) -> Result<()> {
        self.conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;",
        )?;
        self.conn.execute_batch(SCHEMA)?;
        self.migrate()?;
        Ok(())
    }

    /// 幂等迁移到 CACHE_SCHEMA_VERSION：缺列则补列（v1 → v2），
    /// user_version 只升不降；删库后由 SCHEMA 直接以最新结构重建。
    fn migrate(&self) -> Result<()> {
        let has_source_path: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('task') WHERE name = 'source_path'",
            [],
            |row| row.get(0),
        )?;
        if has_source_path == 0 {
            self.conn
                .execute_batch("ALTER TABLE task ADD COLUMN source_path TEXT;")?;
        }
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < CACHE_SCHEMA_VERSION {
            self.conn
                .execute_batch(&format!("PRAGMA user_version = {CACHE_SCHEMA_VERSION};"))?;
        }
        Ok(())
    }

    pub fn begin_scan(&self, source: &str, mode: &str, scope: &str, trigger: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO scan_run (source, mode, scope, trigger, started_at, status) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'running')",
            params![
                source,
                mode,
                scope,
                trigger,
                crate::collectors::common::now_seconds()
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn finish_scan(
        &self,
        scan_id: i64,
        status: &str,
        task_count: usize,
        missing_count: usize,
        error: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE scan_run SET finished_at = ?2, status = ?3, task_count = ?4, \
             missing_count = ?5, error = ?6 WHERE id = ?1",
            params![
                scan_id,
                crate::collectors::common::now_seconds(),
                status,
                task_count as i64,
                missing_count as i64,
                error
            ],
        )?;
        // The run log is diagnostic only; cap it per source so long-lived
        // installations do not grow the cache without bound.
        self.conn.execute(
            "DELETE FROM scan_run WHERE source = (SELECT source FROM scan_run WHERE id = ?1) \
             AND id NOT IN (SELECT id FROM scan_run \
                            WHERE source = (SELECT source FROM scan_run WHERE id = ?1) \
                            ORDER BY id DESC LIMIT ?2)",
            params![scan_id, SCAN_RUNS_KEPT_PER_SOURCE],
        )?;
        Ok(())
    }

    pub fn save_result(&self, result: &SourceResult, scan_id: i64) -> Result<()> {
        let mut todos_by_task: HashMap<&str, Vec<&TodoSnapshot>> = HashMap::new();
        for todo in &result.todos {
            todos_by_task
                .entry(todo.task_key.as_str())
                .or_default()
                .push(todo);
        }

        let now = crate::collectors::common::now_seconds();
        let tx = self.conn.unchecked_transaction()?;
        for task in &result.tasks {
            let task_todos = todos_by_task
                .get(task.key.as_str())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let total = task_todos.len() as i64;
            let done = task_todos
                .iter()
                .filter(|todo| todo.status == "completed")
                .count() as i64;

            let mut snapshot = task.clone();
            snapshot.todo_total = Some(total);
            snapshot.todo_done = Some(done);
            snapshot.source_missing = false;
            snapshot.seen_at = now;
            snapshot.last_seen_run = Some(scan_id);
            snapshot.refresh_fingerprint();

            self.upsert_row(&snapshot)?;
            self.replace_todos(&snapshot.key, task_todos)?;
        }
        tx.commit()?;
        Ok(())
    }

    fn upsert_row(&self, task: &TaskSnapshot) -> Result<()> {
        self.conn.execute(
            TASK_UPSERT,
            params![
                task.key,
                task.source,
                task.source_id,
                task.parent_key,
                task.title,
                task.title_source,
                task.project_path,
                task.project_name,
                task.created_at,
                task.updated_at,
                task.status_source,
                task.status_raw,
                task.source_archived as i64,
                task.source_unread as i64,
                task.source_pinned as i64,
                task.source_missing as i64,
                task.model,
                task.git_branch,
                task.agent_kind,
                task.message_count,
                task.todo_total,
                task.todo_done,
                task.deeplink,
                task.resume_cmd,
                task.source_path,
                task.fingerprint,
                task.seen_at,
                task.last_seen_run
            ],
        )?;
        Ok(())
    }

    fn replace_todos(&self, task_key: &str, todos: &[&TodoSnapshot]) -> Result<()> {
        self.conn.execute(
            "DELETE FROM task_todo WHERE task_key = ?1",
            params![task_key],
        )?;
        let mut statement = self.conn.prepare_cached(
            "INSERT OR REPLACE INTO task_todo (task_key, item_id, subject, status, position) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for todo in todos {
            statement.execute(params![
                task_key,
                todo.item_id,
                todo.subject,
                todo.status,
                todo.position
            ])?;
        }
        Ok(())
    }

    pub fn count_present(&self, source: &str) -> Result<i64> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM task WHERE source = ?1 AND source_missing = 0",
            params![source],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn mark_missing(&self, source: &str, scan_id: i64) -> Result<usize> {
        let newly_missing: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM task \
             WHERE source = ?1 AND source_missing = 0 AND last_seen_run IS NOT ?2",
            params![source, scan_id],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "UPDATE task SET source_missing = 1 \
             WHERE source = ?1 AND last_seen_run IS NOT ?2",
            params![source, scan_id],
        )?;
        Ok(newly_missing as usize)
    }

    pub fn record_sync_state(&self, state: &SourceSyncState) -> Result<()> {
        let scanned_at = state
            .last_scan_at
            .unwrap_or_else(crate::collectors::common::now_seconds);
        self.conn.execute(
            "INSERT INTO sync_state \
               (source, last_scan_at, last_ok_at, last_mode, last_trigger, status, \
                files_seen, task_count, schema_version, error) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
             ON CONFLICT(source) DO UPDATE SET \
               last_scan_at = excluded.last_scan_at, \
               last_ok_at = COALESCE(excluded.last_ok_at, sync_state.last_ok_at), \
               last_mode = excluded.last_mode, \
               last_trigger = excluded.last_trigger, \
               status = excluded.status, \
               files_seen = excluded.files_seen, \
               task_count = excluded.task_count, \
               schema_version = excluded.schema_version, \
               error = excluded.error",
            params![
                state.source,
                scanned_at,
                state.last_ok_at,
                state.last_mode,
                state.last_trigger,
                state.status,
                state.files_seen,
                state.task_count,
                state.schema_version,
                state.error
            ],
        )?;
        Ok(())
    }

    pub fn load_tasks(&self) -> Result<Vec<TaskSnapshot>> {
        let mut statement = self.conn.prepare(
            "SELECT key, source, source_id, parent_key, title, title_source, project_path, \
                    project_name, created_at, updated_at, status_source, status_raw, \
                    source_archived, source_unread, source_pinned, source_missing, model, \
                    git_branch, agent_kind, message_count, todo_total, todo_done, deeplink, \
                    resume_cmd, source_path, fingerprint, seen_at, last_seen_run \
             FROM task ORDER BY COALESCE(updated_at, seen_at) DESC",
        )?;
        let rows = statement.query_map([], task_from_row)?;
        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row?);
        }
        Ok(tasks)
    }

    pub fn load_task(&self, key: &str) -> Result<Option<TaskSnapshot>> {
        let mut statement = self.conn.prepare(
            "SELECT key, source, source_id, parent_key, title, title_source, project_path, \
                    project_name, created_at, updated_at, status_source, status_raw, \
                    source_archived, source_unread, source_pinned, source_missing, model, \
                    git_branch, agent_kind, message_count, todo_total, todo_done, deeplink, \
                    resume_cmd, source_path, fingerprint, seen_at, last_seen_run \
             FROM task WHERE key = ?1",
        )?;
        let mut rows = statement.query_map([key], task_from_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn load_todos(&self) -> Result<Vec<TodoSnapshot>> {
        let mut statement = self.conn.prepare(
            "SELECT task_key, item_id, subject, status, position \
             FROM task_todo ORDER BY task_key, position, item_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(TodoSnapshot {
                task_key: row.get(0)?,
                item_id: row.get(1)?,
                subject: row.get(2)?,
                status: row.get(3)?,
                position: row.get(4)?,
            })
        })?;
        let mut todos = Vec::new();
        for row in rows {
            todos.push(row?);
        }
        Ok(todos)
    }

    pub fn load_sync_states(&self) -> Result<Vec<SourceSyncState>> {
        let mut statement = self.conn.prepare(
            "SELECT source, last_scan_at, last_ok_at, last_mode, last_trigger, status, \
                    files_seen, task_count, schema_version, error \
             FROM sync_state ORDER BY source",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SourceSyncState {
                source: row.get(0)?,
                last_scan_at: row.get(1)?,
                last_ok_at: row.get(2)?,
                last_mode: row.get(3)?,
                last_trigger: row.get(4)?,
                status: row.get(5)?,
                files_seen: row.get(6)?,
                task_count: row.get(7)?,
                schema_version: row.get(8)?,
                error: row.get(9)?,
            })
        })?;
        let mut states = Vec::new();
        for row in rows {
            states.push(row?);
        }
        Ok(states)
    }
}

fn row_bool(row: &Row<'_>, index: usize) -> rusqlite::Result<bool> {
    Ok(row.get::<_, i64>(index)? != 0)
}

fn task_from_row(row: &Row<'_>) -> rusqlite::Result<TaskSnapshot> {
    Ok(TaskSnapshot {
        key: row.get(0)?,
        source: row.get(1)?,
        source_id: row.get(2)?,
        parent_key: row.get(3)?,
        title: row.get(4)?,
        title_source: row.get(5)?,
        project_path: row.get(6)?,
        project_name: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        status_source: row.get(10)?,
        status_raw: row.get(11)?,
        source_archived: row_bool(row, 12)?,
        source_unread: row_bool(row, 13)?,
        source_pinned: row_bool(row, 14)?,
        source_missing: row_bool(row, 15)?,
        model: row.get(16)?,
        git_branch: row.get(17)?,
        agent_kind: row.get(18)?,
        message_count: row.get(19)?,
        todo_total: row.get(20)?,
        todo_done: row.get(21)?,
        deeplink: row.get(22)?,
        resume_cmd: row.get(23)?,
        source_path: row.get(24)?,
        fingerprint: row.get(25)?,
        seen_at: row.get(26)?,
        last_seen_run: row.get(27)?,
    })
}
