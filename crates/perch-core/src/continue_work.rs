use crate::models::{
    TaskSnapshot, SOURCE_ANTIGRAVITY, SOURCE_CLAUDE, SOURCE_CLINE, SOURCE_CODEARTS,
    SOURCE_CODEBUDDY, SOURCE_CODEX, SOURCE_KILO, SOURCE_MINIMAX, SOURCE_OPENCODE, SOURCE_QODER,
    SOURCE_TRAE_CN,
};

pub const QODER_BUNDLE_ID: &str = "com.qoder.app";
pub const CLAUDE_BUNDLE_ID: &str = "com.anthropic.claudefordesktop";
pub const CODEX_BUNDLE_ID: &str = "com.openai.codex";
pub const OPENCODE_BUNDLE_ID: &str = "ai.opencode.desktop";
pub const KILO_BUNDLE_ID: &str = "ai.kilo.desktop";
pub const TRAE_CN_BUNDLE_ID: &str = "cn.trae.app";
pub const CLINE_BUNDLE_ID: &str = "bot.cline.app";
pub const ANTIGRAVITY_BUNDLE_ID: &str = "com.google.antigravity";
pub const MINIMAX_BUNDLE_ID: &str = "com.minimax.agent.cn";
pub const CODEARTS_BUNDLE_ID: &str = "com.huawei.codearts.agent";
pub const CODEBUDDY_BUNDLE_ID: &str = "com.tencent.codebuddycn";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContinueAction {
    OpenUrl(String),
    ActivateApp(String),
}

/// 继续工作已收敛为单一 App 通道：深链与激活 App 都通过 `open` 交给 macOS 处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinueChannel {
    App,
}

impl ContinueChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuePlan {
    pub channel: ContinueChannel,
    pub action: ContinueAction,
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinueHintChannel {
    Deeplink,
    App,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContinueHint {
    pub channel: ContinueHintChannel,
    pub label: String,
    pub detail: String,
    pub locates_session: bool,
}

fn continue_hint(
    channel: ContinueHintChannel,
    label: &str,
    detail: String,
    locates_session: bool,
) -> ContinueHint {
    ContinueHint {
        channel,
        label: label.to_owned(),
        detail,
        locates_session,
    }
}

fn deeplink_hint(app_label: &str) -> ContinueHint {
    continue_hint(
        ContinueHintChannel::Deeplink,
        "深链打开",
        format!("通过会话深链打开 {app_label}"),
        true,
    )
}

fn app_hint(source_label: &str) -> ContinueHint {
    continue_hint(
        ContinueHintChannel::App,
        "仅激活 App",
        format!("激活 {source_label}，无法定位到具体会话"),
        false,
    )
}

/// Describes the fixed continue-work decision using snapshot metadata only.
/// The actual action stays in Rust; the UI must never send a command or URL.
pub fn describe_continue(task: &TaskSnapshot) -> Option<ContinueHint> {
    let label = crate::models::source_label(&task.source);
    let hint = match task.source.as_str() {
        SOURCE_QODER if valid_qoder_session_id(&task.source_id) => deeplink_hint("Qoder"),
        SOURCE_CODEX if codex_deeplink_id(task).is_some() => deeplink_hint("Codex App"),
        SOURCE_CLAUDE | SOURCE_CODEX | SOURCE_QODER | SOURCE_OPENCODE | SOURCE_KILO
        | SOURCE_TRAE_CN | SOURCE_CLINE | SOURCE_ANTIGRAVITY | SOURCE_MINIMAX | SOURCE_CODEARTS
        | SOURCE_CODEBUDDY => app_hint(label),
        _ => return None,
    };
    Some(hint)
}

/// 继续工作只使用固定模板：Qoder 与 Codex 桌面会话走会话级深链，其余来源只激活对应 App。
/// 终端恢复不在此范围内；前端只传任务 key，命令 / URL 不能来自前端。
/// 会话 ID 不满足深链格式时不构造 URL，回退激活 App。
pub fn plan_continue(task: &TaskSnapshot) -> crate::Result<ContinuePlan> {
    match task.source.as_str() {
        // Qoder App 只解析 chat/open；路由会把 sessionId 限制为 [A-Za-z0-9_-]{1,128}。
        SOURCE_QODER if valid_qoder_session_id(&task.source_id) => Ok(app_url_plan(
            format!("qoder://chat/open?sessionId={}", task.source_id),
            "Qoder",
        )),
        SOURCE_QODER => Ok(activate_plan(QODER_BUNDLE_ID, "Qoder App")),
        SOURCE_CLAUDE => Ok(activate_plan(CLAUDE_BUNDLE_ID, "Claude App")),
        SOURCE_CODEX => match codex_deeplink_id(task) {
            Some(id) => Ok(app_url_plan(format!("codex://threads/{id}"), "Codex App")),
            None => Ok(activate_plan(CODEX_BUNDLE_ID, "Codex App")),
        },
        SOURCE_OPENCODE => Ok(activate_plan(OPENCODE_BUNDLE_ID, "OpenCode App")),
        SOURCE_KILO => Ok(activate_plan(KILO_BUNDLE_ID, "Kilo App")),
        // Trae CN 的 hook 只记录项目名，且没有确认的会话级深链，只能激活窗口。
        SOURCE_TRAE_CN => Ok(activate_plan(TRAE_CN_BUNDLE_ID, "Trae CN App")),
        // Cline 桌面 / CLI 都没有确认的会话级深链；按要求只激活 App。
        SOURCE_CLINE => Ok(activate_plan(CLINE_BUNDLE_ID, "Cline App")),
        // Antigravity IDE / agy CLI 均无确认的会话级恢复方式，只能激活窗口。
        SOURCE_ANTIGRAVITY => Ok(activate_plan(ANTIGRAVITY_BUNDLE_ID, "Antigravity App")),
        // MiniMax Code 无确认的会话级深链，只能激活窗口。
        SOURCE_MINIMAX => Ok(activate_plan(MINIMAX_BUNDLE_ID, "MiniMax Code App")),
        // CodeArts（码道）桌面 Agent 无会话级恢复命令，只能激活窗口。
        SOURCE_CODEARTS => Ok(activate_plan(CODEARTS_BUNDLE_ID, "CodeArts App")),
        // CodeBuddy CN（VSCode 系 IDE）无会话级深链，只能激活窗口。
        SOURCE_CODEBUDDY => Ok(activate_plan(CODEBUDDY_BUNDLE_ID, "CodeBuddy App")),
        other => Err(crate::PerchError::message(format!(
            "未知来源，无法继续工作：{other}"
        ))),
    }
}

/// Codex 桌面深链只接受可安全拼进 URL 的 `source_id`；其余 `agent_kind` 不构造 URL。
fn codex_deeplink_id(task: &TaskSnapshot) -> Option<&str> {
    let is_desktop = matches!(task.agent_kind.as_deref(), Some("desktop" | "automation"));
    (is_desktop && valid_session_id(&task.source_id)).then_some(task.source_id.as_str())
}

fn app_url_plan(url: String, target: &str) -> ContinuePlan {
    ContinuePlan {
        channel: ContinueChannel::App,
        action: ContinueAction::OpenUrl(url),
        target: target.to_owned(),
    }
}

fn activate_plan(bundle_id: &str, target: &str) -> ContinuePlan {
    ContinuePlan {
        channel: ContinueChannel::App,
        action: ContinueAction::ActivateApp(bundle_id.to_owned()),
        target: target.to_owned(),
    }
}

fn valid_qoder_session_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn valid_session_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(source: &str, agent_kind: Option<&str>) -> TaskSnapshot {
        let mut task = TaskSnapshot::new(source, "ses_abc123");
        task.project_path = Some("/Users/x/work/gamma".to_owned());
        task.agent_kind = agent_kind.map(str::to_owned);
        task
    }

    #[test]
    fn qoder_uses_session_deeplink() {
        let hint = describe_continue(&task(SOURCE_QODER, None)).unwrap();
        assert_eq!(hint.channel, ContinueHintChannel::Deeplink);
        assert_eq!(hint.label, "深链打开");
        assert!(hint.detail.contains("Qoder"));
        assert!(hint.locates_session);

        let plan = plan_continue(&task(SOURCE_QODER, None)).unwrap();
        assert_eq!(plan.channel, ContinueChannel::App);
        assert_eq!(
            plan.action,
            ContinueAction::OpenUrl("qoder://chat/open?sessionId=ses_abc123".to_owned())
        );
        assert_eq!(plan.target, "Qoder");
    }

    #[test]
    fn qoder_accepts_uuid_style_session_ids() {
        let mut snapshot = TaskSnapshot::new(SOURCE_QODER, "0b53f3a2-7f1c-4c6b-9d3e-2f5c8a1b7e40");
        assert_eq!(
            plan_continue(&snapshot).unwrap().action,
            ContinueAction::OpenUrl(
                "qoder://chat/open?sessionId=0b53f3a2-7f1c-4c6b-9d3e-2f5c8a1b7e40".to_owned()
            )
        );

        snapshot.source_id = "s_abc_123".to_owned();
        assert_eq!(
            plan_continue(&snapshot).unwrap().action,
            ContinueAction::OpenUrl("qoder://chat/open?sessionId=s_abc_123".to_owned())
        );
    }

    #[test]
    fn qoder_session_id_over_limit_falls_back_to_app() {
        let mut task = task(SOURCE_QODER, None);
        task.source_id = "a".repeat(129);

        let hint = describe_continue(&task).unwrap();
        assert_eq!(hint.channel, ContinueHintChannel::App);
        assert!(!hint.locates_session);
        assert!(hint.detail.contains("无法定位"));

        let plan = plan_continue(&task).unwrap();
        assert_eq!(plan.channel, ContinueChannel::App);
        assert_eq!(
            plan.action,
            ContinueAction::ActivateApp(QODER_BUNDLE_ID.to_owned())
        );
        assert_eq!(plan.target, "Qoder App");
    }

    #[test]
    fn app_only_sources_activate_their_desktop_apps() {
        let cases = [
            (SOURCE_CLAUDE, CLAUDE_BUNDLE_ID, "Claude App"),
            (SOURCE_OPENCODE, OPENCODE_BUNDLE_ID, "OpenCode App"),
            (SOURCE_KILO, KILO_BUNDLE_ID, "Kilo App"),
            (SOURCE_CLINE, CLINE_BUNDLE_ID, "Cline App"),
            (SOURCE_ANTIGRAVITY, ANTIGRAVITY_BUNDLE_ID, "Antigravity App"),
            (SOURCE_MINIMAX, MINIMAX_BUNDLE_ID, "MiniMax Code App"),
            (SOURCE_CODEARTS, CODEARTS_BUNDLE_ID, "CodeArts App"),
            (SOURCE_CODEBUDDY, CODEBUDDY_BUNDLE_ID, "CodeBuddy App"),
        ];
        for (source, bundle_id, target) in cases {
            for kind in [None, Some("desktop"), Some("cli")] {
                let plan = plan_continue(&task(source, kind))
                    .unwrap_or_else(|error| panic!("{source}: {error}"));
                assert_eq!(plan.channel, ContinueChannel::App, "{source}");
                assert_eq!(
                    plan.action,
                    ContinueAction::ActivateApp(bundle_id.to_owned()),
                    "{source}"
                );
                assert_eq!(plan.target, target, "{source}");
            }
        }
    }

    #[test]
    fn codex_desktop_uses_deeplink_other_kinds_activate_app() {
        for kind in ["desktop", "automation"] {
            let plan = plan_continue(&task(SOURCE_CODEX, Some(kind))).unwrap();
            assert_eq!(
                plan.action,
                ContinueAction::OpenUrl("codex://threads/ses_abc123".to_owned()),
                "{kind}"
            );
            assert_eq!(plan.target, "Codex App");
        }

        for kind in [None, Some("cli")] {
            let plan = plan_continue(&task(SOURCE_CODEX, kind)).unwrap();
            assert_eq!(plan.channel, ContinueChannel::App);
            assert_eq!(
                plan.action,
                ContinueAction::ActivateApp(CODEX_BUNDLE_ID.to_owned())
            );
        }
    }

    #[test]
    fn continue_hints_expose_action_before_click() {
        let claude = describe_continue(&task(SOURCE_CLAUDE, Some("cli"))).unwrap();
        assert_eq!(claude.channel, ContinueHintChannel::App);
        assert_eq!(claude.label, "仅激活 App");
        assert!(!claude.locates_session);

        let qoder = describe_continue(&task(SOURCE_QODER, None)).unwrap();
        assert_eq!(qoder.channel, ContinueHintChannel::Deeplink);
        assert!(qoder.locates_session);

        for kind in ["desktop", "automation"] {
            let codex = describe_continue(&task(SOURCE_CODEX, Some(kind))).unwrap();
            assert_eq!(codex.channel, ContinueHintChannel::Deeplink, "{kind}");
            assert!(codex.locates_session, "{kind}");
        }
        for kind in [None, Some("cli")] {
            let codex = describe_continue(&task(SOURCE_CODEX, kind)).unwrap();
            assert_eq!(codex.channel, ContinueHintChannel::App);
            assert!(!codex.locates_session);
        }

        for source in [
            SOURCE_OPENCODE,
            SOURCE_KILO,
            SOURCE_TRAE_CN,
            SOURCE_CLINE,
            SOURCE_ANTIGRAVITY,
            SOURCE_MINIMAX,
            SOURCE_CODEARTS,
            SOURCE_CODEBUDDY,
        ] {
            let hint = describe_continue(&task(source, None)).unwrap();
            assert_eq!(hint.channel, ContinueHintChannel::App, "{source}");
            assert!(!hint.locates_session, "{source}");
        }

        let mut unknown = task(SOURCE_CLAUDE, None);
        unknown.source = "synthetic-unknown".to_owned();
        assert!(describe_continue(&unknown).is_none());
        assert!(plan_continue(&unknown).is_err());
    }

    #[test]
    fn unsafe_session_ids_never_reach_a_deeplink() {
        for source_id in ["abc; rm -rf /", "abc/../def", "会话 id", ""] {
            let mut qoder = task(SOURCE_QODER, None);
            qoder.source_id = source_id.to_owned();
            assert_eq!(
                plan_continue(&qoder).unwrap().action,
                ContinueAction::ActivateApp(QODER_BUNDLE_ID.to_owned()),
                "{source_id}"
            );
            let hint = describe_continue(&qoder).unwrap();
            assert_eq!(hint.channel, ContinueHintChannel::App, "{source_id}");
            assert!(!hint.locates_session, "{source_id}");

            let mut codex = task(SOURCE_CODEX, Some("desktop"));
            codex.source_id = source_id.to_owned();
            assert_eq!(
                plan_continue(&codex).unwrap().action,
                ContinueAction::ActivateApp(CODEX_BUNDLE_ID.to_owned()),
                "{source_id}"
            );
            assert_eq!(
                describe_continue(&codex).unwrap().channel,
                ContinueHintChannel::App,
                "{source_id}"
            );
        }
    }
}
