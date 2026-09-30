use bevy_ecs::prelude::*;
use vmux_api::chat::SlashCommand;
use vmux_api::prompt_media::{inline_media_query, replace_inline_media_query};

#[cfg(host)]
use bevy_app::{App, Plugin};
#[cfg(host)]
use bevy_cef::prelude::{UiEventPlugin, UiInput};

#[cfg(host)]
use super::session::ChatView;
use crate::event::ChatComposerEffect;
#[cfg(host)]
use crate::event::ChatDraftChanged;
#[cfg(host)]
use crate::event::{ChatPickFiles, ChatResumeQueryRequest, ChatSlashCommandRequest};
use crate::selector::SelectorMode;
use vmux_ui::prompt_recall::{PromptHistoryDirection, move_prompt_history};

#[derive(Component, Default)]
pub(super) struct ComposerState {
    revision: u64,
    draft: String,
    history_cursor: Option<usize>,
    history_scratch: String,
}

impl ComposerState {
    pub(super) fn update(&mut self, draft: impl Into<String>) {
        self.history_cursor = None;
        self.history_scratch.clear();
        self.draft = draft.into();
    }

    pub(super) fn effect(&mut self, draft: impl Into<String>, focus: bool) -> ChatComposerEffect {
        self.update(draft);
        self.revision = self.revision.wrapping_add(1).max(1);
        ChatComposerEffect {
            revision: self.revision,
            draft: self.draft.clone(),
            focus,
        }
    }

    fn slash_effect(&mut self, command: SlashCommand) -> ChatComposerEffect {
        let draft = match command {
            SlashCommand::Resume => "/resume ",
            SlashCommand::Mcp => "/mcp ",
            SlashCommand::Model => "/model ",
            SlashCommand::Upload => "",
        };
        self.effect(draft, true)
    }

    pub(super) fn recall(
        &mut self,
        history: &[String],
        direction: PromptHistoryDirection,
    ) -> ChatComposerEffect {
        let (draft, cursor, scratch) = move_prompt_history(
            history,
            self.history_cursor,
            &self.history_scratch,
            &self.draft,
            direction,
        );
        self.history_cursor = cursor;
        self.history_scratch = scratch;
        self.draft = draft;
        self.revision = self.revision.wrapping_add(1).max(1);
        ChatComposerEffect {
            revision: self.revision,
            draft: self.draft.clone(),
            focus: true,
        }
    }

    pub(super) fn dismiss_selector(&mut self) -> Option<ChatComposerEffect> {
        let draft = if let Some(query) = inline_media_query(&self.draft) {
            replace_inline_media_query(&self.draft, query, "")
        } else if SelectorMode::from_draft(&self.draft) != SelectorMode::None {
            String::new()
        } else {
            return None;
        };
        Some(self.effect(draft, true))
    }

    pub(super) fn draft(&self) -> &str {
        &self.draft
    }
}

#[derive(Component, Default)]
pub(super) struct ComposerSelectors {
    media_query: String,
    resume_active: bool,
    resume_query: String,
    mcp_open: bool,
}

#[derive(EntityEvent)]
pub(super) struct ComposerChanged {
    #[event_target]
    target: Entity,
}

impl ComposerChanged {
    pub(super) fn new(target: Entity) -> Self {
        Self { target }
    }
}

#[cfg(host)]
pub struct ChatComposerPlugin;

#[cfg(host)]
impl Plugin for ChatComposerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(ChatDraftChanged, ChatSlashCommandRequest)>::default())
            .add_observer(on_draft_changed)
            .add_observer(on_slash_command)
            .add_observer(project_queries);
    }
}

#[cfg(host)]
fn on_draft_changed(
    trigger: On<UiInput<ChatDraftChanged>>,
    mut composers: Query<&mut ComposerState, With<ChatView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(mut composer) = composers.get_mut(webview) else {
        return;
    };
    composer.update(trigger.event().payload.text.clone());
    commands.trigger(ComposerChanged::new(webview));
}

#[cfg(host)]
fn on_slash_command(
    trigger: On<UiInput<ChatSlashCommandRequest>>,
    mut composers: Query<&mut ComposerState, With<ChatView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let command = trigger.event().payload.command;
    let Ok(mut composer) = composers.get_mut(webview) else {
        return;
    };
    let effect = composer.slash_effect(command);
    commands.trigger(
        vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(webview, &effect),
    );
    commands.trigger(ComposerChanged::new(webview));
    match command {
        SlashCommand::Upload => commands.trigger(UiInput {
            webview,
            payload: ChatPickFiles,
        }),
        SlashCommand::Resume | SlashCommand::Mcp | SlashCommand::Model => {}
    }
}

#[cfg(host)]
fn project_queries(
    trigger: On<ComposerChanged>,
    mut composers: Query<(&ComposerState, &mut ComposerSelectors)>,
    mut commands: Commands,
) {
    let webview = trigger.event_target();
    let Ok((composer, mut selectors)) = composers.get_mut(webview) else {
        return;
    };
    let media_query = inline_media_query(&composer.draft)
        .map(|query| query.query.to_string())
        .unwrap_or_default();
    if selectors.media_query != media_query {
        selectors.media_query.clone_from(&media_query);
        commands.trigger(super::media::ChatMediaQuery::new(webview, media_query));
    }
    let (resume_active, resume_query) = match SelectorMode::from_draft(&composer.draft) {
        SelectorMode::Resume(query) => (true, query.to_string()),
        SelectorMode::Commands(query)
            if !query.is_empty() && "resume".starts_with(&query.to_lowercase()) =>
        {
            (true, String::new())
        }
        _ => (false, String::new()),
    };
    if selectors.resume_active != resume_active || selectors.resume_query != resume_query {
        selectors.resume_active = resume_active;
        selectors.resume_query.clone_from(&resume_query);
        commands.trigger(UiInput {
            webview,
            payload: ChatResumeQueryRequest {
                active: resume_active,
                query: resume_query,
            },
        });
    }
    let mcp_open = matches!(
        SelectorMode::from_draft(&composer.draft),
        SelectorMode::Mcp(_)
    );
    if !selectors.mcp_open && mcp_open {
        commands.trigger(UiInput {
            webview,
            payload: vmux_api::mcp::McpServersRequest,
        });
    }
    selectors.mcp_open = mcp_open;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_are_revisioned() {
        let mut state = ComposerState::default();
        let resume = state.slash_effect(SlashCommand::Resume);
        let upload = state.slash_effect(SlashCommand::Upload);
        assert_eq!(resume.revision, 1);
        assert_eq!(resume.draft, "/resume ");
        assert_eq!(upload.revision, 2);
        assert!(upload.draft.is_empty());
        assert!(upload.focus);
    }

    #[test]
    fn selector_projection_is_entity_state() {
        let mut app = App::new();
        app.add_observer(project_queries);
        let composer = app
            .world_mut()
            .spawn((ComposerState::default(), ComposerSelectors::default()))
            .id();

        app.world_mut()
            .get_mut::<ComposerState>(composer)
            .unwrap()
            .update("/res");
        app.world_mut().trigger(ComposerChanged::new(composer));
        let selectors = app.world().get::<ComposerSelectors>(composer).unwrap();
        assert!(selectors.resume_active);
        assert!(selectors.resume_query.is_empty());

        app.world_mut()
            .get_mut::<ComposerState>(composer)
            .unwrap()
            .update("show @src");
        app.world_mut().trigger(ComposerChanged::new(composer));
        let selectors = app.world().get::<ComposerSelectors>(composer).unwrap();
        assert_eq!(selectors.media_query, "src");
        assert!(!selectors.resume_active);

        app.world_mut()
            .get_mut::<ComposerState>(composer)
            .unwrap()
            .update("/mcp ");
        app.world_mut().trigger(ComposerChanged::new(composer));
        assert!(
            app.world()
                .get::<ComposerSelectors>(composer)
                .unwrap()
                .mcp_open
        );
    }

    #[test]
    fn prompt_history_is_owned_by_the_composer() {
        let mut state = ComposerState::default();
        state.update("unfinished");
        let history = vec!["first".to_string(), "second".to_string()];

        let older = state.recall(&history, PromptHistoryDirection::Older);
        let oldest = state.recall(&history, PromptHistoryDirection::Older);
        let newer = state.recall(&history, PromptHistoryDirection::Newer);
        let scratch = state.recall(&history, PromptHistoryDirection::Newer);

        assert_eq!(older.draft, "second");
        assert_eq!(oldest.draft, "first");
        assert_eq!(newer.draft, "second");
        assert_eq!(scratch.draft, "unfinished");
    }

    #[test]
    fn selector_dismissal_is_owned_by_the_composer() {
        let mut state = ComposerState::default();
        state.update("open @src");

        let media = state.dismiss_selector().unwrap();
        state.update("/model sonnet");
        let model = state.dismiss_selector().unwrap();

        assert_eq!(media.draft, "open ");
        assert!(model.draft.is_empty());
    }
}
