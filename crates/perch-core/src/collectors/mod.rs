pub mod claude;
pub mod cline;
pub mod codex;
pub mod common;
pub mod kilocode;
pub mod opencode;
pub mod qoder;
pub mod session_db;
pub mod trae_cn;

use crate::models::SourceResult;
use crate::paths::SourcePaths;

pub trait Collector: Send {
    fn source(&self) -> &'static str;
    fn scan(&self) -> SourceResult;
}

pub fn all_collectors(paths: &SourcePaths) -> Vec<Box<dyn Collector>> {
    vec![
        Box::new(qoder::QoderCollector::new(paths.qoder_db.clone())),
        Box::new(claude::ClaudeCollector::new(paths.claude_dir.clone())),
        Box::new(codex::CodexCollector::new(paths.codex_dir.clone())),
        Box::new(opencode::OpenCodeCollector::new(
            paths.opencode_db.clone(),
            paths.opencode_stable_db.clone(),
        )),
        Box::new(kilocode::KiloCollector::new(paths.kilo_db.clone())),
        Box::new(trae_cn::TraeCnCollector::new(
            paths.trae_cn_events_dir.clone(),
            paths.trae_cn_memory_dir.clone(),
        )),
        Box::new(cline::ClineCollector::new(paths.cline_db.clone())),
    ]
}
