use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{NaiveDateTime, TimeZone, Timelike};
use serde_json::Value;
use walkdir::WalkDir;

use super::common::{fallback_title, truncate_chars};
use super::Collector;
use crate::models::{SourceResult, TaskSnapshot, SOURCE_CODEBUDDY};

const CN_COPILOT_DIR: &str = "Tencent-Cloud.coding-copilot";

/// 腾讯 CodeBuddy CN：会话信息来自两处镜像日志——
/// `CodeBuddy CN/logs/**/Tencent-Cloud.coding-copilot/*.log` 与
/// `CodeBuddyExtension/Logs/CodeBuddyIDE/**/*.log`。trace → conversationId
/// 来自 onAgentStart 行，标题取同行的 userInput，模型来自 Preparing model，
/// 活动时间取 Step execution metrics 的本地时间戳；项目维度来自
/// genie-history 目录名的 base64url 解码。
pub struct CodeBuddyCollector {
    cn_logs_dir: PathBuf,
    ide_logs_dir: PathBuf,
    genie_dir: PathBuf,
}

impl CodeBuddyCollector {
    pub fn new(cn_logs_dir: PathBuf, ide_logs_dir: PathBuf, genie_dir: PathBuf) -> Self {
        Self {
            cn_logs_dir,
            ide_logs_dir,
            genie_dir,
        }
    }

    fn log_files(&self) -> (Vec<PathBuf>, Vec<String>) {
        let mut files = Vec::new();
        let mut warnings = Vec::new();
        for (root, copilot_only) in [(&self.cn_logs_dir, true), (&self.ide_logs_dir, false)] {
            if !root.is_dir() {
                warnings.push(format!("CodeBuddy 日志目录不存在：{}", root.display()));
                continue;
            }
            for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
                if !entry.file_type().is_file() {
                    continue;
                }
                let path = entry.path();
                let is_log = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("log"));
                if !is_log {
                    continue;
                }
                if copilot_only
                    && !path.ancestors().any(|dir| {
                        dir.file_name().and_then(|name| name.to_str()) == Some(CN_COPILOT_DIR)
                    })
                {
                    continue;
                }
                files.push(path.to_path_buf());
            }
        }
        files.sort();
        (files, warnings)
    }

    fn scan_inner(&self) -> crate::Result<SourceResult> {
        let (files, mut warnings) = self.log_files();
        if files.is_empty() {
            return Ok(SourceResult::unavailable(
                SOURCE_CODEBUDDY,
                if warnings.is_empty() {
                    "两处 CodeBuddy 日志目录均无日志文件".to_owned()
                } else {
                    warnings.join("；")
                },
            ));
        }
        warnings.clear();

        let projects = read_genie_projects(&self.genie_dir);
        let mut sessions: HashMap<String, SessionEntry> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        let mut trace_map: HashMap<String, String> = HashMap::new();

        let mut texts = Vec::new();
        for path in &files {
            match std::fs::read_to_string(path) {
                Ok(text) => texts.push(text),
                Err(error) => {
                    warnings.push(format!("读取 {} 失败：{error}", path.display()));
                }
            }
        }

        // 第一遍：trace → conversationId（onAgentStart 可能晚于同 trace 的
        // Preparing model / metrics 行，必须先建好映射再归组）。
        for text in &texts {
            for line in text.lines() {
                if let Some((trace, conversation)) = parse_agent_start(line) {
                    let key = sanitize_session_id(&conversation);
                    trace_map.insert(trace, key.clone());
                    let entry = entry_for(&mut sessions, &mut order, key);
                    if entry.first_user.is_none() {
                        if let Some(user_input) = parse_user_input(line) {
                            entry.first_user = Some(user_input);
                        }
                    }
                }
            }
        }

        // 第二遍：模型与 step 活动时间。
        for text in &texts {
            for line in text.lines() {
                if let Some((trace, model)) = parse_preparing_model(line) {
                    let key = resolve_key(&trace_map, &trace);
                    let entry = entry_for(&mut sessions, &mut order, key);
                    if entry.model.is_none() {
                        entry.model = Some(model);
                    }
                }
                if line.contains("Step execution metrics:") && line.contains("[AgentReporter]") {
                    let Some(trace) = bracket_trace_after(line, "[AgentReporter]") else {
                        continue;
                    };
                    let Some(ts) = line_timestamp(line) else {
                        continue;
                    };
                    let key = resolve_key(&trace_map, &trace);
                    let entry = entry_for(&mut sessions, &mut order, key);
                    entry.created_at = Some(entry.created_at.map_or(ts, |value| value.min(ts)));
                    entry.updated_at = Some(entry.updated_at.map_or(ts, |value| value.max(ts)));
                }
            }
        }

        let mut tasks: Vec<TaskSnapshot> = Vec::new();
        for key in &order {
            let entry = &sessions[key];
            let mut task = TaskSnapshot::new(SOURCE_CODEBUDDY, key);
            task.title = match entry.first_user.as_deref() {
                Some(user_input) => {
                    task.title_source = "source".to_owned();
                    truncate_chars(user_input, 240)
                }
                None => {
                    task.title_source = "fallback".to_owned();
                    fallback_title()
                }
            };
            if let Some(workspace) = projects.get(key) {
                task.project_path = Some(workspace.clone());
                task.project_name = super::common::project_name(workspace);
            }
            task.created_at = entry.created_at;
            task.updated_at = entry.updated_at;
            task.model = entry.model.clone();
            task.status_source = Some("idle".to_owned());
            task.agent_kind = Some("desktop".to_owned());
            task.refresh_fingerprint();
            tasks.push(task);
        }

        let mut result = if warnings.is_empty() {
            SourceResult::ok(SOURCE_CODEBUDDY, tasks, Vec::new())
        } else {
            SourceResult::partial(SOURCE_CODEBUDDY, tasks, Vec::new(), warnings.join("；"))
        };
        result.files_seen = files.len() as i64;
        result.probe_kind = "file_tree";
        Ok(result)
    }
}

impl Collector for CodeBuddyCollector {
    fn source(&self) -> &'static str {
        SOURCE_CODEBUDDY
    }

    fn scan(&self) -> SourceResult {
        match self.scan_inner() {
            Ok(result) => result,
            Err(error) => SourceResult::error(SOURCE_CODEBUDDY, error.to_string()),
        }
    }
}

struct SessionEntry {
    first_user: Option<String>,
    model: Option<String>,
    created_at: Option<f64>,
    updated_at: Option<f64>,
}

impl SessionEntry {
    fn new() -> Self {
        Self {
            first_user: None,
            model: None,
            created_at: None,
            updated_at: None,
        }
    }
}

fn entry_for<'a>(
    sessions: &'a mut HashMap<String, SessionEntry>,
    order: &mut Vec<String>,
    key: String,
) -> &'a mut SessionEntry {
    if !sessions.contains_key(&key) {
        order.push(key.clone());
    }
    sessions.entry(key).or_insert_with(SessionEntry::new)
}

/// 已知 trace 映射到 conversationId；未知 trace 按自身归组。
fn resolve_key(trace_map: &HashMap<String, String>, trace: &str) -> String {
    trace_map
        .get(trace)
        .cloned()
        .unwrap_or_else(|| sanitize_session_id(trace))
}

fn sanitize_session_id(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect();
    if cleaned.is_empty() {
        "unknown".to_owned()
    } else {
        cleaned
    }
}

/// `... [AgentReporter] [trace] onAgentStart: ... conversationId=<hex>`
fn parse_agent_start(line: &str) -> Option<(String, String)> {
    if !line.contains("onAgentStart:") || !line.contains("[AgentReporter]") {
        return None;
    }
    let trace = bracket_trace_after(line, "[AgentReporter]")?;
    let index = line.find("conversationId=")? + "conversationId=".len();
    let rest = &line[index..];
    let conversation: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric())
        .collect();
    if conversation.is_empty() {
        None
    } else {
        Some((trace, conversation))
    }
}

/// `onAgentStart: userInput=<文本> agent=craft ...` → 用户输入文本。
fn parse_user_input(line: &str) -> Option<String> {
    let index = line.find("userInput=")? + "userInput=".len();
    let rest = &line[index..];
    let end = rest.find(" agent=").unwrap_or(rest.len());
    let text = rest[..end].trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_owned())
    }
}

/// `[CraftInvokableAgent] [trace] Preparing model: Hy4 flash (hy4flash-free)`
fn parse_preparing_model(line: &str) -> Option<(String, String)> {
    let index = line.find("Preparing model:")?;
    let trace = bracket_trace_before(line, index)?;
    let rest = &line[index + "Preparing model:".len()..];
    let open = rest.rfind('(')?;
    let close = rest[open..].find(')')? + open;
    let model = rest[open + 1..close].trim();
    if model.is_empty() {
        None
    } else {
        Some((trace, model.to_owned()))
    }
}

/// 取 marker 之后第一个 `[...]` 括号内的 token（即 trace）。
fn bracket_trace_after(line: &str, marker: &str) -> Option<String> {
    let index = line.find(marker)? + marker.len();
    let rest = &line[index..];
    let start = rest.find('[')? + 1;
    let end = rest[start..].find(']')? + start;
    bracket_token(&rest[start - 1..end + 1])
}

/// 取 marker 之前紧邻的 `[...]` 括号内的 token（Preparing model 行的 trace 在前）。
fn bracket_trace_before(line: &str, marker_index: usize) -> Option<String> {
    let before = &line[..marker_index];
    let end = before.rfind(']')?;
    let start = before[..end].rfind('[')?;
    bracket_token(&before[start..=end])
}

fn bracket_token(token: &str) -> Option<String> {
    let inner = token
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))?;
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

/// 两种本地时间戳：`2026-11-20 15:28:04.652` 与
/// `[11/20/2026, 3:28:08 PM.672]`；按本地时区换算 epoch 秒。
fn line_timestamp(line: &str) -> Option<f64> {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix('[') {
        // 方括号格式：MM/DD/YYYY, h:MM:SS AM.mmm]，小数部分是毫秒。
        let end = rest.find(']')?;
        let raw = &rest[..end];
        let dot = raw.rfind('.')?;
        let (base, fraction) = raw.split_at(dot);
        let digits: String = fraction[1..]
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect();
        let mut padded = digits;
        while padded.len() < 6 {
            padded.push('0');
        }
        padded.truncate(6);
        let micros: u32 = padded.parse().ok()?;
        let naive = NaiveDateTime::parse_from_str(base, "%m/%d/%Y, %I:%M:%S %p").ok()?;
        local_epoch(naive.with_nanosecond(micros * 1_000)?)
    } else {
        let raw = trimmed.chars().take(23).collect::<String>();
        let naive = NaiveDateTime::parse_from_str(&raw, "%Y-%m-%d %H:%M:%S%.f").ok()?;
        local_epoch(naive)
    }
}

fn local_epoch(naive: NaiveDateTime) -> Option<f64> {
    chrono::Local
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.timestamp() as f64 + dt.timestamp_subsec_nanos() as f64 / 1e9)
}

/// genie-history/<base64url(workspace)>/current.json 的 conversationId
/// 指向一个会话；目录名解码得到工作区路径。
fn read_genie_projects(genie_dir: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if !genie_dir.is_dir() {
        return map;
    }
    for entry in WalkDir::new(genie_dir).into_iter().filter_map(Result::ok) {
        if entry.file_type().is_file()
            && entry.path().file_name().and_then(|name| name.to_str()) == Some("current.json")
        {
            let Some(encoded) = entry
                .path()
                .parent()
                .and_then(|dir| dir.file_name())
                .and_then(|name| name.to_str())
            else {
                continue;
            };
            let Some(workspace) = decode_workspace(encoded) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let Some(conversation) = value.get("conversationId").and_then(Value::as_str) else {
                continue;
            };
            if conversation.is_empty() {
                continue;
            }
            map.entry(conversation.to_owned())
                .or_insert_with(|| workspace.clone());
        }
    }
    map
}

/// 厂商把 padding 的 `=` 写成 `_`，部分目录还追加 `__`；先剥尾部下划线
/// 再补回 padding（与 usage-ledger CHG-0019 的解码规则一致）。
fn decode_workspace(encoded: &str) -> Option<String> {
    let candidate = encoded.trim_end_matches('_');
    if candidate.is_empty() {
        return None;
    }
    let decoded = super::minimax::urlsafe_b64_decode(candidate)?;
    if decoded.starts_with('/') {
        Some(decoded)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CN_LINE: &str = "2026-11-20 15:28:04.660 [info] [AgentReporter] [a8e5acce] \
         onAgentStart: userInput=@/tmp/x  修复登录页 agent=craft mode=craft \
         conversationId=2fc5374e3cfc49958bae3fc026b30d98 requestId=409";

    #[test]
    fn agent_start_extracts_trace_conversation_and_user_input() {
        let (trace, conversation) = parse_agent_start(CN_LINE).unwrap();
        assert_eq!(trace, "a8e5acce");
        assert_eq!(conversation, "2fc5374e3cfc49958bae3fc026b30d98");
        assert_eq!(
            parse_user_input(CN_LINE).as_deref(),
            Some("@/tmp/x  修复登录页")
        );
    }

    #[test]
    fn preparing_model_takes_innermost_parentheses() {
        let line = "2026-11-20 15:28:04.652 [info] [CraftInvokableAgent] [a8e5acce] \
             Preparing model: Hy4 flash (hy4flash-free)";
        let (trace, model) = parse_preparing_model(line).unwrap();
        assert_eq!(trace, "a8e5acce");
        assert_eq!(model, "hy4flash-free");
    }

    #[test]
    fn iso_timestamp_parses_local_time() {
        let line = "2026-11-20 15:28:04.660 [info] [AgentReporter] [a] Step execution metrics: \
             step=1, duration=3997ms, usage={}";
        let ts = line_timestamp(line).unwrap();
        let naive =
            NaiveDateTime::parse_from_str("2026-11-20 15:28:04.660", "%Y-%m-%d %H:%M:%S%.f")
                .unwrap();
        let expected = chrono::Local
            .from_local_datetime(&naive)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(ts, expected as f64 + 0.66);
    }

    #[test]
    fn bracket_timestamp_parses_ide_log_time() {
        let line = "[11/20/2026, 3:28:08 PM.672] [Info] [AgentReporter] [a]  \
             Step execution metrics: step=1, duration=1ms, usage={}";
        let ts = line_timestamp(line).unwrap();
        let naive =
            NaiveDateTime::parse_from_str("11/20/2026, 3:28:08 PM", "%m/%d/%Y, %I:%M:%S %p")
                .unwrap();
        let expected = chrono::Local
            .from_local_datetime(&naive.with_nanosecond(672_000_000).unwrap())
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(ts, expected as f64 + 0.672);
    }

    #[test]
    fn workspace_decode_requires_absolute_path() {
        let encoded = "L1VzZXJzL3dheW5lL0NvZGVCdWRkeS8yMDI2MTEyMDE1MDk0Nw__";
        assert_eq!(
            decode_workspace(encoded).as_deref(),
            Some("/Users/wayne/CodeBuddy/20261120150947")
        );
        assert_eq!(decode_workspace("aGVsbG8_"), None, "relative path rejected");
    }
}
