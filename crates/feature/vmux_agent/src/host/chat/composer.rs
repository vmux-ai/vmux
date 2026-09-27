use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use super::AgentChatView;
use crate::strategy::{acp_agent_kind, kind_supports_cross_runtime};
use vmux_api::chat::SlashCommand;
use vmux_api::mcp::McpServersRequest;
use vmux_chat::composer::{ComposerQueriesChanged, ComposerState};
use vmux_chat::event::{ChatDraftChanged, ChatPickFiles, ChatSlashCommandRequest};
use vmux_core::agent::SwapStackSession;
use vmux_session::AcpSession;

pub(super) struct ChatComposerPlugin;

impl Plugin for ChatComposerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(ChatDraftChanged, ChatSlashCommandRequest)>::default())
            .add_observer(on_draft_changed)
            .add_observer(on_slash_command)
            .add_observer(dispatch_composer_queries);
    }
}

fn on_draft_changed(
    trigger: On<UiInput<ChatDraftChanged>>,
    mut composers: Query<&mut ComposerState, With<AgentChatView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(mut composer) = composers.get_mut(webview) else {
        return;
    };
    let changes = composer.update(trigger.event().payload.text.clone());
    if let Some(changed) = ComposerQueriesChanged::new(webview, changes) {
        commands.trigger(changed);
    }
}

fn on_slash_command(
    trigger: On<UiInput<ChatSlashCommandRequest>>,
    mut composers: Query<&mut ComposerState, With<AgentChatView>>,
    child_of: Query<&ChildOf>,
    acp_sessions: Query<&AcpSession>,
    mut swap: MessageWriter<SwapStackSession>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let command = trigger.event().payload.command;
    let Ok(mut composer) = composers.get_mut(webview) else {
        return;
    };
    let (effect, changes) = composer.effect(command.draft(), true);
    commands.trigger(
        vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
            webview, &effect,
        ),
    );
    if let Some(changed) = ComposerQueriesChanged::new(webview, changes) {
        commands.trigger(changed);
    }
    match command {
        SlashCommand::Upload => commands.trigger(UiInput {
            webview,
            payload: ChatPickFiles,
        }),
        SlashCommand::Cli => switch_to_cli(webview, &child_of, &acp_sessions, &mut swap),
        SlashCommand::Resume | SlashCommand::Mcp | SlashCommand::Model => {}
    }
}

fn dispatch_composer_queries(trigger: On<ComposerQueriesChanged>, mut commands: Commands) {
    let webview = trigger.event_target();
    if let Some(query) = trigger.event().media() {
        commands.trigger(vmux_chat::media::ChatMediaQuery::new(
            webview,
            query.to_string(),
        ));
    }
    if let Some(query) = trigger.event().resume() {
        commands.trigger(super::resume::ChatResumeQuery::new(
            webview,
            query.active,
            query.query.clone(),
        ));
    }
    if trigger.event().opens_mcp() {
        commands.trigger(UiInput {
            webview,
            payload: McpServersRequest,
        });
    }
}

fn switch_to_cli(
    webview: Entity,
    child_of: &Query<&ChildOf>,
    acp_sessions: &Query<&AcpSession>,
    swap: &mut MessageWriter<SwapStackSession>,
) {
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    let stack = parent.parent();
    let Ok(acp) = acp_sessions.get(stack) else {
        bevy::log::warn!("runtime switch: current pane is not an ACP session");
        return;
    };
    let Some((target_url, cwd)) = cli_target(&acp.agent_id, acp.resume.as_deref(), &acp.cwd) else {
        bevy::log::warn!(
            "runtime switch to CLI unavailable for ACP agent '{}' (no shared session id yet)",
            acp.agent_id
        );
        return;
    };
    swap.write(SwapStackSession {
        stack,
        target_url,
        cwd,
        handoff: None,
    });
}

fn cli_target(
    agent_id: &str,
    resume: Option<&str>,
    cwd: &std::path::Path,
) -> Option<(String, std::path::PathBuf)> {
    let kind = acp_agent_kind(agent_id)?;
    if !kind_supports_cross_runtime(kind) {
        return None;
    }
    let sid = resume?;
    let target = crate::AgentUrl::Cli {
        kind,
        sid: sid.to_string(),
    };
    Some((target.format(), cwd.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn builtin_acp_agents_switch_to_cli() {
        let cases = [
            ("claude", "claude"),
            ("claude-acp", "claude"),
            ("codex", "codex"),
            ("codex-acp", "codex"),
            ("vibe", "vibe"),
            ("mistral-vibe", "vibe"),
        ];
        for (agent_id, cli_segment) in cases {
            let got = cli_target(agent_id, Some("sid-9"), Path::new("/w"));
            assert_eq!(
                got,
                Some((
                    format!("vmux://sessions/{cli_segment}/cli/sid-9"),
                    std::path::PathBuf::from("/w")
                ))
            );
        }
    }

    #[test]
    fn cli_switch_requires_shared_session() {
        assert_eq!(cli_target("claude", None, Path::new("/w")), None);
        assert_eq!(cli_target("custom", Some("s"), Path::new("/w")), None);
    }
}
