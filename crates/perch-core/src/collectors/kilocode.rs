use std::path::PathBuf;

use super::session_db::{scan_session_db, ResumeBuilder};
use super::Collector;
use crate::collectors::common::shell_cd_command;
use crate::models::{SourceResult, SOURCE_KILO};

pub struct KiloCollector {
    db_path: PathBuf,
}

impl KiloCollector {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

impl Collector for KiloCollector {
    fn source(&self) -> &'static str {
        SOURCE_KILO
    }

    fn scan(&self) -> SourceResult {
        scan_session_db(SOURCE_KILO, &self.db_path, resume as ResumeBuilder)
    }
}

fn resume(directory: Option<&str>, _session_id: &str) -> Option<String> {
    shell_cd_command(directory, "kilo")
}
