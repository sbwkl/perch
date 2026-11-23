pub mod antigravity;
pub mod claude;
pub mod cline;
pub mod codearts;
pub mod codebuddy;
pub mod codex;
pub mod common;
pub mod kilocode;
pub mod minimax;
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
        Box::new(
            claude::ClaudeCollector::new(paths.claude_dir.clone())
                .with_desktop_sessions_root(paths.claude_desktop_sessions_dir.clone()),
        ),
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
        Box::new(antigravity::AntigravityCollector::new(
            paths.antigravity_ide_root.clone(),
            paths.antigravity_cli_root.clone(),
        )),
        Box::new(minimax::MiniMaxCollector::new(paths.minimax_dir.clone())),
        Box::new(codearts::CodeArtsCollector::new(paths.codearts_db.clone())),
        Box::new(codebuddy::CodeBuddyCollector::new(
            paths.codebuddy_cn_logs_dir.clone(),
            paths.codebuddy_ide_logs_dir.clone(),
            paths.codebuddy_genie_dir.clone(),
        )),
    ]
}
