use std::path::PathBuf;

use super::session_db::{scan_session_db, ResumeBuilder};
use super::Collector;
use crate::models::{SourceResult, SOURCE_CODEARTS};

/// CodeArts（码道）复用 OpenCode 旧版 schema（session 表），数据在
/// `~/.codeartsdoer/codearts-data/opencode.db`；共享 session_db 采集器。
pub struct CodeArtsCollector {
    db_path: PathBuf,
}

impl CodeArtsCollector {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

impl Collector for CodeArtsCollector {
    fn source(&self) -> &'static str {
        SOURCE_CODEARTS
    }

    fn scan(&self) -> SourceResult {
        scan_session_db(SOURCE_CODEARTS, &self.db_path, resume as ResumeBuilder)
    }
}

// CodeArts Agent 没有经验证的会话级恢复命令，恢复走桌面 App 激活。
fn resume(_directory: Option<&str>, _session_id: &str) -> Option<String> {
    None
}
