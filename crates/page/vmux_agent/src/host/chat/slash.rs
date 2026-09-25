use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use super::AgentChatView;
use crate::strategy::{acp_agent_kind, kind_supports_cross_runtime};
use vmux_api::chat::SlashCommand;
use vmux_api::mcp::McpServersRequest;
use vmux_chat::event::{ChatComposerEffect, ChatPickFiles, ChatSlashCommandRequest};
use vmux_core::agent::SwapStackSession;
use vmux_session::AcpSession;

pub(super) struct ChatSlashPlugin;

impl Plugin for ChatSlashPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(ChatSlashCommandRequest,)>::default())
            .add_observer(on_slash_command);
    }
}

#[derive(Component, Default)]
pub(super) struct ChatComposerProjection {
    revision: u64,
}

impl ChatComposerProjection {
    fn effect(&mut self, command: SlashCommand) -> ChatComposerEffect {
        self.revision = self.revision.wrapping_add(1).max(1);
        ChatComposerEffect {
            revision: self.revision,
            draft: command.draft().to_string(),
            focus: true,
        }
    }
}

fn on_slash_command(
    trigger: On<UiInput<ChatSlashCommandRequest>>,
    mut projections: Query<&mut ChatComposerProjection, With<AgentChatView>>,
    child_of: Query<&ChildOf>,
    acp_sessions: Query<&AcpSession>,
    mut swap: MessageWriter<SwapStackSession>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let command = trigger.event().payload.command;
    let Ok(mut projection) = projections.get_mut(webview) else {
        return;
    };
    let effect = projection.effect(command);
    commands.trigger(
        vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
            webview, &effect,
        ),
    );
    match command {
        SlashCommand::Upload => commands.trigger(UiInput {
            webview,
            payload: ChatPickFiles,
        }),
        SlashCommand::Mcp => commands.trigger(UiInput {
            webview,
            payload: McpServersRequest,
        }),
        SlashCommand::Cli => switch_to_cli(webview, &child_of, &acp_sessions, &mut swap),
        SlashCommand::Resume | SlashCommand::Model => {}
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

    #[test]
    fn composer_effects_are_revisioned() {
        let mut projection = ChatComposerProjection::default();
        let resume = projection.effect(SlashCommand::Resume);
        let upload = projection.effect(SlashCommand::Upload);
        assert_eq!(resume.revision, 1);
        assert_eq!(resume.draft, "/resume ");
        assert_eq!(upload.revision, 2);
        assert!(upload.draft.is_empty());
        assert!(upload.focus);
    }
}
