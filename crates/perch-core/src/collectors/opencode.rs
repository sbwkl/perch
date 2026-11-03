use std::path::PathBuf;

use super::session_db::{scan_session_db, ResumeBuilder};
use super::Collector;
use crate::collectors::common::shell_cd_command;
use crate::models::{SourceResult, SOURCE_OPENCODE};

pub struct OpenCodeCollector {
    primary: PathBuf,
    fallback: PathBuf,
}

impl OpenCodeCollector {
    pub fn new(primary: PathBuf, fallback: PathBuf) -> Self {
        Self { primary, fallback }
    }

    fn db_path(&self) -> Option<PathBuf> {
        crate::paths::SourcePaths::first_existing(&[self.primary.clone(), self.fallback.clone()])
    }
}

impl Collector for OpenCodeCollector {
    fn source(&self) -> &'static str {
        SOURCE_OPENCODE
    }

    fn scan(&self) -> SourceResult {
        let Some(path) = self.db_path() else {
            return SourceResult::unavailable(
                SOURCE_OPENCODE,
                format!("未找到 OpenCode 数据库：{}", self.primary.display()),
            );
        };
        scan_session_db(SOURCE_OPENCODE, &path, resume as ResumeBuilder)
    }
}

fn resume(directory: Option<&str>, session_id: &str) -> Option<String> {
    shell_cd_command(directory, &format!("opencode --session {session_id}"))
}
