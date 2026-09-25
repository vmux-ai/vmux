use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use super::AgentChatView;
use crate::strategy::{acp_agent_kind, kind_supports_cross_runtime};
use vmux_api::chat::SlashCommand;
use vmux_api::mcp::McpServersRequest;
use vmux_api::prompt_media::inline_media_query;
use vmux_chat::event::{
    ChatComposerEffect, ChatDraftChanged, ChatPickFiles, ChatSlashCommandRequest,
};
use vmux_chat::format::{SelectorMode, selector_mode};
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

#[derive(Component, Default)]
pub(super) struct ChatComposerProjection {
    revision: u64,
    draft: String,
    media_query: String,
    resume_active: bool,
    resume_query: String,
    mcp_open: bool,
}

impl ChatComposerProjection {
    fn update(&mut self, draft: String) -> ChatComposerQueries {
        self.draft = draft;
        let media_query = inline_media_query(&self.draft)
            .map(|query| query.query.to_string())
            .unwrap_or_default();
        let (resume_active, resume_query) = match selector_mode(&self.draft) {
            SelectorMode::Resume(query) => (true, query.to_string()),
            SelectorMode::Commands(query)
                if !query.is_empty() && "resume".starts_with(&query.to_lowercase()) =>
            {
                (true, String::new())
            }
            _ => (false, String::new()),
        };
        let mcp_open = matches!(selector_mode(&self.draft), SelectorMode::Mcp(_));
        let queries = ChatComposerQueries {
            media: (self.media_query != media_query).then_some(media_query.clone()),
            resume: (self.resume_active != resume_active || self.resume_query != resume_query)
                .then_some((resume_active, resume_query.clone())),
            open_mcp: !self.mcp_open && mcp_open,
        };
        self.media_query = media_query;
        self.resume_active = resume_active;
        self.resume_query = resume_query;
        self.mcp_open = mcp_open;
        queries
    }

    pub(super) fn effect(
        &mut self,
        draft: impl Into<String>,
        focus: bool,
    ) -> (ChatComposerEffect, ChatComposerQueries) {
        let queries = self.update(draft.into());
        self.revision = self.revision.wrapping_add(1).max(1);
        (
            ChatComposerEffect {
                revision: self.revision,
                draft: self.draft.clone(),
                focus,
            },
            queries,
        )
    }

    pub(super) fn draft(&self) -> &str {
        &self.draft
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct ChatComposerQueries {
    media: Option<String>,
    resume: Option<(bool, String)>,
    open_mcp: bool,
}

impl ChatComposerQueries {
    fn changed(&self) -> bool {
        self.media.is_some() || self.resume.is_some() || self.open_mcp
    }
}

#[derive(EntityEvent)]
pub(super) struct ChatComposerQueriesChanged {
    #[event_target]
    webview: Entity,
    queries: ChatComposerQueries,
}

impl ChatComposerQueriesChanged {
    pub(super) fn new(webview: Entity, queries: ChatComposerQueries) -> Option<Self> {
        queries.changed().then_some(Self { webview, queries })
    }
}

fn on_draft_changed(
    trigger: On<UiInput<ChatDraftChanged>>,
    mut projections: Query<&mut ChatComposerProjection, With<AgentChatView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(mut projection) = projections.get_mut(webview) else {
        return;
    };
    let queries = projection.update(trigger.event().payload.text.clone());
    if let Some(changed) = ChatComposerQueriesChanged::new(webview, queries) {
        commands.trigger(changed);
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
    let (effect, queries) = projection.effect(command.draft(), true);
    commands.trigger(
        vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
            webview, &effect,
        ),
    );
    if let Some(changed) = ChatComposerQueriesChanged::new(webview, queries) {
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

fn dispatch_composer_queries(trigger: On<ChatComposerQueriesChanged>, mut commands: Commands) {
    let webview = trigger.event_target();
    let queries = &trigger.event().queries;
    if let Some(query) = &queries.media {
        commands.trigger(super::media::ChatMediaQuery::new(webview, query.clone()));
    }
    if let Some((active, query)) = &queries.resume {
        commands.trigger(super::resume::ChatResumeQuery::new(
            webview,
            *active,
            query.clone(),
        ));
    }
    if queries.open_mcp {
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

    #[test]
    fn composer_effects_are_revisioned() {
        let mut projection = ChatComposerProjection::default();
        let (resume, resume_queries) = projection.effect(SlashCommand::Resume.draft(), true);
        let (upload, upload_queries) = projection.effect(SlashCommand::Upload.draft(), true);
        assert_eq!(resume.revision, 1);
        assert_eq!(resume.draft, "/resume ");
        assert_eq!(resume_queries.resume, Some((true, String::new())));
        assert_eq!(upload.revision, 2);
        assert!(upload.draft.is_empty());
        assert!(upload.focus);
        assert_eq!(upload_queries.resume, Some((false, String::new())));
    }

    #[test]
    fn draft_changes_drive_media_resume_and_mcp_queries() {
        let mut projection = ChatComposerProjection::default();

        let resume = projection.update("/res".into());
        assert_eq!(resume.resume, Some((true, String::new())));

        let media = projection.update("show @src".into());
        assert_eq!(media.media, Some("src".into()));
        assert_eq!(media.resume, Some((false, String::new())));

        let mcp = projection.update("/mcp ".into());
        assert!(mcp.open_mcp);
        assert_eq!(mcp.media, Some(String::new()));
    }
}
