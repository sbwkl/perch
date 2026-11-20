use std::env;
use std::path::{Path, PathBuf};

fn env_path(key: &str) -> Option<PathBuf> {
    env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn home_dir() -> PathBuf {
    env::var_os("PERCH_SOURCE_HOME")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[derive(Debug, Clone)]
pub struct SourcePaths {
    pub home: PathBuf,
    pub qoder_db: PathBuf,
    pub claude_dir: PathBuf,
    pub codex_dir: PathBuf,
    pub opencode_db: PathBuf,
    pub opencode_stable_db: PathBuf,
    pub kilo_db: PathBuf,
    pub trae_cn_events_dir: PathBuf,
    pub trae_cn_memory_dir: PathBuf,
    pub cline_db: PathBuf,
    pub antigravity_ide_root: PathBuf,
    pub antigravity_cli_root: PathBuf,
    pub minimax_dir: PathBuf,
    pub codearts_db: PathBuf,
    pub codebuddy_cn_logs_dir: PathBuf,
    pub codebuddy_ide_logs_dir: PathBuf,
    pub codebuddy_genie_dir: PathBuf,
}

impl SourcePaths {
    pub fn detect() -> Self {
        let home = home_dir();
        let data_dir = home.join(".local/share");
        Self {
            qoder_db: env_path("PERCH_QODER_DB").unwrap_or_else(|| {
                home.join("Library/Application Support/com.qoder.app.stable/main.sqlite")
            }),
            claude_dir: env_path("PERCH_CLAUDE_DIR").unwrap_or_else(|| home.join(".claude")),
            codex_dir: env_path("PERCH_CODEX_DIR").unwrap_or_else(|| home.join(".codex")),
            opencode_db: env_path("PERCH_OPENCODE_DB")
                .unwrap_or_else(|| data_dir.join("opencode/opencode.db")),
            opencode_stable_db: env_path("PERCH_OPENCODE_STABLE_DB")
                .unwrap_or_else(|| data_dir.join("opencode/opencode-stable.db")),
            kilo_db: env_path("PERCH_KILO_DB").unwrap_or_else(|| data_dir.join("kilo/kilo.db")),
            trae_cn_events_dir: env_path("PERCH_TRAE_CN_EVENTS_DIR")
                .or_else(|| env_path("USAGE_LEDGER_HOME").map(|home| home.join("trae-cn")))
                .unwrap_or_else(|| data_dir.join("usage-ledger/trae-cn")),
            trae_cn_memory_dir: env_path("PERCH_TRAE_CN_MEMORY_DIR")
                .unwrap_or_else(|| home.join(".trae-cn/memory/projects")),
            cline_db: env_path("PERCH_CLINE_DB")
                .unwrap_or_else(|| home.join(".cline/data/db/sessions.db")),
            antigravity_ide_root: env_path("PERCH_ANTIGRAVITY_IDE_DIR")
                .unwrap_or_else(|| home.join(".gemini/antigravity")),
            antigravity_cli_root: env_path("PERCH_ANTIGRAVITY_CLI_DIR")
                .unwrap_or_else(|| home.join(".gemini/antigravity-cli")),
            minimax_dir: env_path("PERCH_MINIMAX_DIR").unwrap_or_else(|| home.join(".minimax")),
            codearts_db: env_path("PERCH_CODEARTS_DB")
                .unwrap_or_else(|| home.join(".codeartsdoer/codearts-data/opencode.db")),
            codebuddy_cn_logs_dir: env_path("PERCH_CODEBUDDY_CN_LOGS_DIR")
                .unwrap_or_else(|| home.join("Library/Application Support/CodeBuddy CN/logs")),
            codebuddy_ide_logs_dir: env_path("PERCH_CODEBUDDY_IDE_LOGS_DIR").unwrap_or_else(|| {
                home.join("Library/Application Support/CodeBuddyExtension/Logs/CodeBuddyIDE")
            }),
            codebuddy_genie_dir: env_path("PERCH_CODEBUDDY_GENIE_DIR").unwrap_or_else(|| {
                home.join(
                    "Library/Application Support/CodeBuddy CN/User/globalStorage/\
                     tencent-cloud.coding-copilot/genie-history",
                )
            }),
            home,
        }
    }

    pub fn first_existing(candidates: &[PathBuf]) -> Option<PathBuf> {
        candidates
            .iter()
            .find(|candidate| candidate.is_file())
            .cloned()
    }
}

pub fn env_path_override(key: &str) -> Option<PathBuf> {
    env_path(key)
}

pub fn cache_db_override() -> Option<PathBuf> {
    env_path("PERCH_CACHE_DB")
}

/// usage-ledger's archive is an optional read-only input, not an agent source.
/// Precedence mirrors usage-ledger itself so both tools agree on the location.
pub fn usage_db_path() -> PathBuf {
    if let Some(path) = env_path("PERCH_USAGE_DB").or_else(|| env_path("USAGE_LEDGER_DB")) {
        return path;
    }
    if let Some(home) = env_path("USAGE_LEDGER_HOME") {
        return home.join("usage.db");
    }
    home_dir().join(".local/share/usage-ledger/usage.db")
}

pub fn ensure_parent(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}
