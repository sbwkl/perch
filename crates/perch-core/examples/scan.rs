use std::path::PathBuf;

use perch_core::collectors::common::now_seconds;
use perch_core::models::source_label;
use perch_core::query::build_bootstrap;
use perch_core::{CacheStore, ScanEngine, SourcePaths};

fn main() {
    let cache_path = std::env::var_os("PERCH_CACHE_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("perch-cache.db"));
    let store = match CacheStore::open(&cache_path) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("open cache {} failed: {error}", cache_path.display());
            std::process::exit(1);
        }
    };
    let engine = ScanEngine::new(store, &SourcePaths::detect());
    println!("cache: {}", cache_path.display());

    let report = engine.scan(None, "manual");
    println!(
        "scan finished in {:.2}s",
        report.finished_at - report.started_at
    );
    for item in &report.reports {
        println!(
            "{:<9} {:<10} tasks={:<4} todos={:<3} missing={:<3} {:>5}ms{}",
            item.source,
            item.status,
            item.task_count,
            item.todo_count,
            item.missing_count,
            item.duration_ms,
            item.error
                .as_deref()
                .map(|error| format!("  [{error}]"))
                .unwrap_or_default()
        );
    }

    match build_bootstrap(engine.store(), now_seconds()) {
        Ok(payload) => {
            println!(
                "workspaces={} total={} recent={} older={} archived={} attention={}",
                payload.workspaces.len(),
                payload.counts.total,
                payload.counts.recent,
                payload.counts.older,
                payload.counts.archived,
                payload.counts.attention
            );
            for source in &payload.sources {
                println!(
                    "source {:<9} {:<10} tasks={} files={:?}",
                    source_label(&source.source),
                    source.status,
                    source.task_count,
                    source.files_seen
                );
            }
            for group in payload.workspaces.iter().take(10) {
                println!(
                    "workspace {:<24} tasks={:<3} last={:?}",
                    group.name,
                    group.tasks.len(),
                    group.last_activity_at
                );
            }
        }
        Err(error) => eprintln!("build bootstrap failed: {error}"),
    }
}
