use std::panic::AssertUnwindSafe;
use std::time::Instant;

use crate::collectors::common::now_seconds;
use crate::collectors::{all_collectors, Collector};
use crate::models::{
    SourceReport, SourceResult, SourceScanStatus, SourceSyncState, SyncReport,
};
use crate::paths::SourcePaths;
use crate::store::CacheStore;

pub struct ScanEngine {
    store: CacheStore,
    collectors: Vec<Box<dyn Collector>>,
}

impl ScanEngine {
    pub fn new(store: CacheStore, paths: &SourcePaths) -> Self {
        Self {
            store,
            collectors: all_collectors(paths),
        }
    }

    pub fn with_store(store: CacheStore) -> Self {
        Self {
            store,
            collectors: Vec::new(),
        }
    }

    pub fn store(&self) -> &CacheStore {
        &self.store
    }

    pub fn scan(&self, only: Option<&str>, trigger: &str) -> SyncReport {
        let started_at = now_seconds();
        let mut reports = Vec::new();

        for collector in &self.collectors {
            let source = collector.source();
            if only.is_some_and(|wanted| wanted != source) {
                continue;
            }
            let scan_started = Instant::now();
            let scan_id = match self.store.begin_scan(source, "full", "source", trigger) {
                Ok(id) => id,
                Err(error) => {
                    reports.push(SourceReport {
                        source: source.to_owned(),
                        status: "error".to_owned(),
                        mode: "full".to_owned(),
                        task_count: 0,
                        todo_count: 0,
                        missing_count: 0,
                        duration_ms: scan_started.elapsed().as_millis() as u64,
                        error: Some(format!("创建扫描记录失败：{error}")),
                    });
                    continue;
                }
            };

            let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| collector.scan()));
            let result = match outcome {
                Ok(result) => result,
                Err(_) => SourceResult::error(source, "采集器异常退出，已保留旧快照"),
            };

            let mut status = result.status;
            let mut error = result.error.clone();
            let mut missing_count = 0usize;
            let mut snapshot_saved = false;

            match status {
                SourceScanStatus::Ok => {
                    let previous = self.store.count_present(source).unwrap_or(0);
                    let scanned = result.tasks.len() as i64;
                    if previous > 20 && scanned * 2 < previous {
                        status = SourceScanStatus::Partial;
                        error = Some(format!(
                            "任务数从 {previous} 降到 {scanned}，疑似异常，已保留旧快照；请手动刷新确认"
                        ));
                    } else {
                        match self.store.save_result(&result, scan_id) {
                            Ok(()) => {
                                snapshot_saved = true;
                                match self.store.mark_missing(source, scan_id) {
                                    Ok(count) => missing_count = count,
                                    Err(store_error) => {
                                        status = SourceScanStatus::Error;
                                        error = Some(format!("更新缺失标记失败：{store_error}"));
                                    }
                                }
                            }
                            Err(store_error) => {
                                status = SourceScanStatus::Error;
                                error = Some(format!("写入快照失败：{store_error}"));
                            }
                        }
                    }
                }
                SourceScanStatus::Partial => {
                    match self.store.save_result(&result, scan_id) {
                        Ok(()) => snapshot_saved = true,
                        Err(store_error) => {
                            status = SourceScanStatus::Error;
                            error = Some(format!("写入快照失败：{store_error}"));
                        }
                    }
                }
                SourceScanStatus::Unavailable | SourceScanStatus::Error => {}
            }

            let present_count = self.store.count_present(source).unwrap_or(0);
            let task_count_for_state = if snapshot_saved {
                result.tasks.len() as i64
            } else {
                present_count
            };
            let scanned_at = now_seconds();
            let state = SourceSyncState {
                source: source.to_owned(),
                last_scan_at: Some(scanned_at),
                last_ok_at: (status == SourceScanStatus::Ok).then_some(scanned_at),
                last_mode: Some("full".to_owned()),
                last_trigger: Some(trigger.to_owned()),
                status: status.as_str().to_owned(),
                files_seen: Some(result.files_seen),
                task_count: Some(task_count_for_state),
                schema_version: result.schema_version.clone(),
                error: error.clone(),
            };
            if let Err(store_error) = self.store.record_sync_state(&state) {
                error = Some(match error {
                    Some(existing) => format!("{existing}；写入同步状态失败：{store_error}"),
                    None => format!("写入同步状态失败：{store_error}"),
                });
            }

            if let Err(store_error) =
                self.store
                    .finish_scan(scan_id, status.as_str(), result.tasks.len(), missing_count, error.as_deref())
            {
                error = Some(match error {
                    Some(existing) => format!("{existing}；结束扫描记录失败：{store_error}"),
                    None => format!("结束扫描记录失败：{store_error}"),
                });
            }

            reports.push(SourceReport {
                source: source.to_owned(),
                status: status.as_str().to_owned(),
                mode: "full".to_owned(),
                task_count: result.tasks.len(),
                todo_count: result.todos.len(),
                missing_count,
                duration_ms: scan_started.elapsed().as_millis() as u64,
                error,
            });
        }

        SyncReport {
            started_at,
            finished_at: now_seconds(),
            reports,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{SourceResult, SourceScanStatus, TaskSnapshot};

    struct FixedCollector {
        result: SourceResult,
    }

    impl Collector for FixedCollector {
        fn source(&self) -> &'static str {
            "qoder"
        }

        fn scan(&self) -> SourceResult {
            self.result.clone()
        }
    }

    fn stub_task(index: usize) -> TaskSnapshot {
        let mut task = TaskSnapshot::new("qoder", &format!("task-{index}"));
        task.title = format!("任务 {index}");
        task.updated_at = Some(1_000.0 + index as f64);
        task.refresh_fingerprint();
        task
    }

    #[test]
    fn abnormal_shrink_keeps_previous_snapshot() {
        let store = CacheStore::open_in_memory().unwrap();
        let scan_id = store.begin_scan("qoder", "full", "source", "test").unwrap();
        let initial: Vec<TaskSnapshot> = (0..25).map(stub_task).collect();
        store
            .save_result(&SourceResult::ok("qoder", initial, Vec::new()), scan_id)
            .unwrap();

        let engine = ScanEngine {
            store,
            collectors: vec![Box::new(FixedCollector {
                result: SourceResult::ok("qoder", vec![stub_task(99)], Vec::new()),
            })],
        };
        let report = engine.scan(None, "test");

        assert_eq!(report.reports.len(), 1);
        assert_eq!(report.reports[0].status, "partial");
        assert_eq!(report.reports[0].status, SourceScanStatus::Partial.as_str());
        assert!(
            report.reports[0]
                .error
                .as_deref()
                .is_some_and(|error| error.contains("疑似异常")),
            "{:?}",
            report.reports[0].error
        );
        assert_eq!(engine.store().count_present("qoder").unwrap(), 25);
        assert!(
            engine
                .store()
                .load_tasks()
                .unwrap()
                .iter()
                .all(|task| !task.source_missing),
            "anomaly must not mark tasks missing"
        );
    }
}
