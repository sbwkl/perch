use std::path::Path;
use std::process::Command;

use crate::models::{
    TaskSnapshot, SOURCE_CLAUDE, SOURCE_CLINE, SOURCE_CODEX, SOURCE_KILO, SOURCE_OPENCODE,
    SOURCE_QODER, SOURCE_TRAE_CN,
};

pub const QODER_BUNDLE_ID: &str = "com.qoder.app";
pub const CODEX_BUNDLE_ID: &str = "com.openai.codex";
pub const OPENCODE_BUNDLE_ID: &str = "ai.opencode.desktop";
pub const KILO_BUNDLE_ID: &str = "ai.kilo.desktop";
pub const TRAE_CN_BUNDLE_ID: &str = "cn.trae.app";
pub const CLINE_BUNDLE_ID: &str = "bot.cline.app";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinueChannel {
    App,
    Cli,
}

impl ContinueChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Cli => "cli",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContinueAction {
    OpenUrl(String),
    ActivateApp(String),
    Terminal {
        cwd: Option<String>,
        command: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuePlan {
    pub channel: ContinueChannel,
    pub action: ContinueAction,
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliPresence {
    None,
    Agent,
    Session,
}

pub trait ChannelProbe {
    fn app_running(&self, bundle_id: &str) -> bool;
    fn cli_presence(&self, source: &str, session_id: &str) -> CliPresence;
}

/// 无法从数据判断执行通道时：App 在运行优先 App；否则有 CLI 进程走终端；
/// 两者都没有时仍按用户偏好优先打开 App。
pub fn plan_continue(task: &TaskSnapshot, probe: &dyn ChannelProbe) -> crate::Result<ContinuePlan> {
    if !valid_session_id(&task.source_id) {
        return Err(crate::PerchError::message(format!(
            "会话 ID 含有不支持的字符：{}",
            task.source_id
        )));
    }
    match task.source.as_str() {
        // Qoder App 不解析 qoder://chat/<id>（main.log 报 DEEP_LINK_ROUTE_UNSUPPORTED），只能激活窗口。
        SOURCE_QODER => Ok(ContinuePlan {
            channel: ContinueChannel::App,
            action: ContinueAction::ActivateApp(QODER_BUNDLE_ID.to_owned()),
            target: "Qoder App".to_owned(),
        }),
        // Claude Code 的 App 只负责打开新会话，不能恢复指定会话，只能走终端。
        SOURCE_CLAUDE => Ok(terminal_plan(
            task,
            format!("claude --resume {}", task.source_id),
        )),
        SOURCE_CODEX => match task.agent_kind.as_deref() {
            Some("desktop" | "automation") => Ok(app_url_plan(
                format!("codex://threads/{}", task.source_id),
                "Codex App",
            )),
            Some("cli") => Ok(terminal_plan(
                task,
                format!("codex resume {}", task.source_id),
            )),
            _ => Ok(probed_plan(
                task,
                probe,
                CODEX_BUNDLE_ID,
                ContinueAction::OpenUrl(format!("codex://threads/{}", task.source_id)),
                format!("codex resume {}", task.source_id),
                "Codex App",
            )),
        },
        SOURCE_OPENCODE => Ok(probed_plan(
            task,
            probe,
            OPENCODE_BUNDLE_ID,
            ContinueAction::ActivateApp(OPENCODE_BUNDLE_ID.to_owned()),
            format!("opencode --session {}", task.source_id),
            "OpenCode App",
        )),
        SOURCE_KILO => Ok(probed_plan(
            task,
            probe,
            KILO_BUNDLE_ID,
            ContinueAction::ActivateApp(KILO_BUNDLE_ID.to_owned()),
            "kilo".to_owned(),
            "Kilo App",
        )),
        // Trae CN 的 hook 只记录项目名，且没有确认的会话级深链，只能激活窗口。
        SOURCE_TRAE_CN => Ok(ContinuePlan {
            channel: ContinueChannel::App,
            action: ContinueAction::ActivateApp(TRAE_CN_BUNDLE_ID.to_owned()),
            target: "Trae CN App".to_owned(),
        }),
        // Cline 桌面版没有会话级深链；CLI 会话可用 `cline --id` 精确恢复。
        SOURCE_CLINE => match task.agent_kind.as_deref() {
            Some("desktop") => Ok(ContinuePlan {
                channel: ContinueChannel::App,
                action: ContinueAction::ActivateApp(CLINE_BUNDLE_ID.to_owned()),
                target: "Cline App".to_owned(),
            }),
            Some("cli") => Ok(terminal_plan(
                task,
                format!("cline --id {}", task.source_id),
            )),
            _ => Ok(probed_plan(
                task,
                probe,
                CLINE_BUNDLE_ID,
                ContinueAction::ActivateApp(CLINE_BUNDLE_ID.to_owned()),
                format!("cline --id {}", task.source_id),
                "Cline App",
            )),
        },
        other => Err(crate::PerchError::message(format!(
            "未知来源，无法继续工作：{other}"
        ))),
    }
}

fn probed_plan(
    task: &TaskSnapshot,
    probe: &dyn ChannelProbe,
    bundle_id: &str,
    app_action: ContinueAction,
    cli_command: String,
    app_target: &str,
) -> ContinuePlan {
    if probe.app_running(bundle_id) {
        return ContinuePlan {
            channel: ContinueChannel::App,
            action: app_action,
            target: app_target.to_owned(),
        };
    }
    match probe.cli_presence(&task.source, &task.source_id) {
        CliPresence::Session | CliPresence::Agent => terminal_plan(task, cli_command),
        CliPresence::None => ContinuePlan {
            channel: ContinueChannel::App,
            action: app_action,
            target: app_target.to_owned(),
        },
    }
}

fn app_url_plan(url: String, target: &str) -> ContinuePlan {
    ContinuePlan {
        channel: ContinueChannel::App,
        action: ContinueAction::OpenUrl(url),
        target: target.to_owned(),
    }
}

fn terminal_plan(task: &TaskSnapshot, command: String) -> ContinuePlan {
    ContinuePlan {
        channel: ContinueChannel::Cli,
        action: ContinueAction::Terminal {
            cwd: task.project_path.clone(),
            command,
        },
        target: "终端".to_owned(),
    }
}

fn valid_session_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

pub struct SystemProbe;

impl SystemProbe {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelProbe for SystemProbe {
    fn app_running(&self, bundle_id: &str) -> bool {
        Command::new("/usr/bin/lsappinfo")
            .args(["find", &format!("bundleID={bundle_id}")])
            .output()
            .map(|output| {
                output.status.success()
                    && !String::from_utf8_lossy(&output.stdout).trim().is_empty()
            })
            .unwrap_or(false)
    }

    fn cli_presence(&self, source: &str, session_id: &str) -> CliPresence {
        let Ok(output) = Command::new("/bin/ps")
            .args(["-axo", "pid=,ppid=,command="])
            .output()
        else {
            return CliPresence::None;
        };
        if !output.status.success() {
            return CliPresence::None;
        }
        parse_ps_presence(source, session_id, &String::from_utf8_lossy(&output.stdout))
    }
}

pub fn parse_ps_presence(source: &str, session_id: &str, listing: &str) -> CliPresence {
    let mut agent_running = false;
    for line in listing.lines() {
        let mut fields = line.split_whitespace();
        if fields.next().is_none() || fields.next().is_none() {
            continue;
        }
        let command = fields.collect::<Vec<_>>().join(" ");
        if command.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = command.split_whitespace().collect();
        let Some(executable) = tokens.first() else {
            continue;
        };
        let name = Path::new(executable)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let is_cli = match source {
            SOURCE_OPENCODE => {
                matches!(name, "opencode" | "opencode-cli")
                    // App 内嵌服务也叫 opencode-cli，但它以 serve --service 启动。
                    && !(tokens.contains(&"serve") && tokens.contains(&"--service"))
            }
            SOURCE_KILO => name == "kilo",
            SOURCE_CODEX => name == "codex",
            SOURCE_CLAUDE => name == "claude",
            SOURCE_CLINE => {
                // npm 版 CLI 以 `node .../bin/cline` 启动，可执行文件名不是 cline。
                let cline_token = tokens.iter().any(|token| {
                    matches!(
                        Path::new(token)
                            .file_name()
                            .and_then(|value| value.to_str()),
                        Some("cline" | "cline-cli")
                    )
                });
                cline_token && !tokens.contains(&"--cline-hub-daemon")
            }
            _ => false,
        };
        if !is_cli {
            continue;
        }
        if tokens.contains(&session_id) {
            return CliPresence::Session;
        }
        agent_running = true;
    }
    if agent_running {
        CliPresence::Agent
    } else {
        CliPresence::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeProbe {
        app_running: bool,
        cli_presence: CliPresence,
    }

    impl ChannelProbe for FakeProbe {
        fn app_running(&self, _bundle_id: &str) -> bool {
            self.app_running
        }

        fn cli_presence(&self, _source: &str, _session_id: &str) -> CliPresence {
            self.cli_presence
        }
    }

    fn task(source: &str, agent_kind: Option<&str>) -> TaskSnapshot {
        let mut task = TaskSnapshot::new(source, "ses_abc123");
        task.project_path = Some("/Users/x/work/gamma".to_owned());
        task.agent_kind = agent_kind.map(str::to_owned);
        task
    }

    fn probe(app_running: bool, cli_presence: CliPresence) -> FakeProbe {
        FakeProbe {
            app_running,
            cli_presence,
        }
    }

    #[test]
    fn qoder_activates_app_without_session_target() {
        let plan =
            plan_continue(&task(SOURCE_QODER, None), &probe(false, CliPresence::None)).unwrap();
        assert_eq!(plan.channel, ContinueChannel::App);
        assert_eq!(
            plan.action,
            ContinueAction::ActivateApp(QODER_BUNDLE_ID.to_owned())
        );
    }

    #[test]
    fn claude_uses_terminal_resume() {
        let plan = plan_continue(
            &task(SOURCE_CLAUDE, Some("cli")),
            &probe(true, CliPresence::Agent),
        )
        .unwrap();
        assert_eq!(plan.channel, ContinueChannel::Cli);
        assert_eq!(
            plan.action,
            ContinueAction::Terminal {
                cwd: Some("/Users/x/work/gamma".to_owned()),
                command: "claude --resume ses_abc123".to_owned(),
            }
        );
    }

    #[test]
    fn codex_channel_comes_from_agent_kind() {
        let desktop = plan_continue(
            &task(SOURCE_CODEX, Some("desktop")),
            &probe(false, CliPresence::None),
        )
        .unwrap();
        assert_eq!(
            desktop.action,
            ContinueAction::OpenUrl("codex://threads/ses_abc123".to_owned())
        );

        let cli = plan_continue(
            &task(SOURCE_CODEX, Some("cli")),
            &probe(true, CliPresence::None),
        )
        .unwrap();
        assert_eq!(cli.channel, ContinueChannel::Cli);
        assert_eq!(
            cli.action,
            ContinueAction::Terminal {
                cwd: Some("/Users/x/work/gamma".to_owned()),
                command: "codex resume ses_abc123".to_owned(),
            }
        );
    }

    #[test]
    fn codex_unknown_falls_back_to_probe() {
        let app =
            plan_continue(&task(SOURCE_CODEX, None), &probe(true, CliPresence::None)).unwrap();
        assert_eq!(
            app.action,
            ContinueAction::OpenUrl("codex://threads/ses_abc123".to_owned())
        );

        let cli =
            plan_continue(&task(SOURCE_CODEX, None), &probe(false, CliPresence::Agent)).unwrap();
        assert_eq!(cli.channel, ContinueChannel::Cli);

        let fallback =
            plan_continue(&task(SOURCE_CODEX, None), &probe(false, CliPresence::None)).unwrap();
        assert_eq!(
            fallback.action,
            ContinueAction::OpenUrl("codex://threads/ses_abc123".to_owned())
        );
    }

    #[test]
    fn opencode_prefers_running_app_then_cli_then_app() {
        let app = plan_continue(
            &task(SOURCE_OPENCODE, None),
            &probe(true, CliPresence::Session),
        )
        .unwrap();
        assert_eq!(app.channel, ContinueChannel::App);
        assert_eq!(
            app.action,
            ContinueAction::ActivateApp(OPENCODE_BUNDLE_ID.to_owned())
        );

        let cli = plan_continue(
            &task(SOURCE_OPENCODE, None),
            &probe(false, CliPresence::Session),
        )
        .unwrap();
        assert_eq!(cli.channel, ContinueChannel::Cli);
        assert_eq!(
            cli.action,
            ContinueAction::Terminal {
                cwd: Some("/Users/x/work/gamma".to_owned()),
                command: "opencode --session ses_abc123".to_owned(),
            }
        );

        let agent = plan_continue(
            &task(SOURCE_OPENCODE, None),
            &probe(false, CliPresence::Agent),
        )
        .unwrap();
        assert_eq!(agent.channel, ContinueChannel::Cli);

        let fallback = plan_continue(
            &task(SOURCE_OPENCODE, None),
            &probe(false, CliPresence::None),
        )
        .unwrap();
        assert_eq!(
            fallback.action,
            ContinueAction::ActivateApp(OPENCODE_BUNDLE_ID.to_owned())
        );
    }

    #[test]
    fn kilo_uses_probe_and_plain_cli_command() {
        let cli =
            plan_continue(&task(SOURCE_KILO, None), &probe(false, CliPresence::Agent)).unwrap();
        assert_eq!(
            cli.action,
            ContinueAction::Terminal {
                cwd: Some("/Users/x/work/gamma".to_owned()),
                command: "kilo".to_owned(),
            }
        );

        let app = plan_continue(&task(SOURCE_KILO, None), &probe(true, CliPresence::None)).unwrap();
        assert_eq!(
            app.action,
            ContinueAction::ActivateApp(KILO_BUNDLE_ID.to_owned())
        );
    }

    #[test]
    fn trae_cn_activates_app_without_session_target() {
        let plan = plan_continue(
            &task(SOURCE_TRAE_CN, Some("desktop")),
            &probe(true, CliPresence::None),
        )
        .unwrap();
        assert_eq!(plan.channel, ContinueChannel::App);
        assert_eq!(
            plan.action,
            ContinueAction::ActivateApp(TRAE_CN_BUNDLE_ID.to_owned())
        );
    }

    #[test]
    fn cline_channels_follow_session_origin() {
        let desktop = plan_continue(
            &task(SOURCE_CLINE, Some("desktop")),
            &probe(false, CliPresence::None),
        )
        .unwrap();
        assert_eq!(desktop.channel, ContinueChannel::App);
        assert_eq!(
            desktop.action,
            ContinueAction::ActivateApp(CLINE_BUNDLE_ID.to_owned())
        );

        let cli = plan_continue(
            &task(SOURCE_CLINE, Some("cli")),
            &probe(true, CliPresence::None),
        )
        .unwrap();
        assert_eq!(cli.channel, ContinueChannel::Cli);
        assert_eq!(
            cli.action,
            ContinueAction::Terminal {
                cwd: Some("/Users/x/work/gamma".to_owned()),
                command: "cline --id ses_abc123".to_owned(),
            }
        );

        let app =
            plan_continue(&task(SOURCE_CLINE, None), &probe(true, CliPresence::None)).unwrap();
        assert_eq!(
            app.action,
            ContinueAction::ActivateApp(CLINE_BUNDLE_ID.to_owned())
        );

        let detected =
            plan_continue(&task(SOURCE_CLINE, None), &probe(false, CliPresence::Agent)).unwrap();
        assert_eq!(detected.channel, ContinueChannel::Cli);
    }

    #[test]
    fn rejects_unsafe_session_ids() {
        let mut bad = task(SOURCE_QODER, None);
        bad.source_id = "abc; rm -rf /".to_owned();
        assert!(plan_continue(&bad, &probe(false, CliPresence::None)).is_err());
    }

    #[test]
    fn ps_parser_ignores_opencode_app_server() {
        let listing = "\
  100     1 /Applications/OpenCode.app/Contents/MacOS/OpenCode
  101   100 /Users/x/Library/Application Support/ai.opencode.desktop/cli/2.2.5-arm64/opencode-cli serve --service
  102     1 /Applications/Cline.app/Contents/MacOS/cline-cli --cline-hub-daemon --cwd / --port 25463
  103     1 /bin/zsh -lc ls
";
        assert_eq!(
            parse_ps_presence(SOURCE_OPENCODE, "ses_abc123", listing),
            CliPresence::None
        );
        assert_eq!(
            parse_ps_presence(SOURCE_CLINE, "ses_abc123", listing),
            CliPresence::None
        );
    }

    #[test]
    fn ps_parser_finds_session_and_agent_processes() {
        let listing = "\
  100     1 /Applications/OpenCode.app/Contents/MacOS/OpenCode
  200     1 /Applications/OpenCode.app/Contents/Resources/opencode-cli --session ses_abc123
  300     1 /opt/homebrew/Cellar/kilo/7.8.15/libexec/kilo
  400     1 /opt/homebrew/bin/codex resume 01a0f16e
  500     1 /Users/x/.nvm/versions/node/v24/bin/cline --id ses_abc123
";
        assert_eq!(
            parse_ps_presence(SOURCE_OPENCODE, "ses_abc123", listing),
            CliPresence::Session
        );
        assert_eq!(
            parse_ps_presence(SOURCE_OPENCODE, "ses_other", listing),
            CliPresence::Agent
        );
        assert_eq!(
            parse_ps_presence(SOURCE_KILO, "ses_abc123", listing),
            CliPresence::Agent
        );
        assert_eq!(
            parse_ps_presence(SOURCE_CODEX, "01a0f16e", listing),
            CliPresence::Session
        );
        assert_eq!(
            parse_ps_presence(SOURCE_CLINE, "ses_abc123", listing),
            CliPresence::Session
        );
        assert_eq!(
            parse_ps_presence(SOURCE_CLINE, "ses_other", listing),
            CliPresence::Agent
        );
    }
}
