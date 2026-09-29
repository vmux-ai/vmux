#[cfg(host)]
use bevy_app::{App, Plugin, Update};
#[cfg(host)]
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;

use crate::activity::ActivityIcon;
use crate::composer::ComposerState;
use crate::event::{
    CHAT_HISTORY_MAX_PAGE_SIZE, CHAT_HISTORY_PAGE_SIZE, ChatAttachment, ChatBranch,
    ChatBranchesState, ChatItem, ChatMediaState, ChatOpenPage, ChatResumeState, ChatSnapshot,
    ChatTranscriptState, ComposerContext, ResumableSessions,
};
use crate::state::ChatUiState;
use vmux_core::chat::group_turns_tail;
use vmux_core::chat_projection::{activity_counts, current_activity};
use vmux_core::host::UiState;
use vmux_core::team::Profile;
use vmux_core::{PageIcon, PageIdentity};
use vmux_layout::stack::OpenRequest;
use vmux_session::{AgentConversationTitle, AgentMessages, AgentRunState, AgentSession};

type ChatUiStateUpdates = UiState<ChatUiState>;

#[cfg(host)]
pub struct ChatPlugin;

#[cfg(host)]
impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::room::ChatRoomPlugin,
            crate::ChatKeyPlugin,
            crate::ChatMediaPlugin,
            crate::composer::ChatComposerPlugin,
            crate::prompt::ChatPromptPlugin,
            crate::prompt::ChatPromptInputPlugin,
        ))
        .add_plugins(UiEventPlugin::<(ChatOpenPage,)>::default())
        .add_observer(open_page)
        .add_systems(Update, report_tab_identity);
    }
}

#[cfg(host)]
fn open_page(trigger: On<UiInput<ChatOpenPage>>, mut requests: MessageWriter<OpenRequest>) {
    let url = trigger.event().payload.url.clone();
    if url.is_empty() {
        return;
    }
    requests.write(OpenRequest { url: Some(url) });
}

const TAB_ACTIVITY_TAIL_ITEMS: usize = 1;

fn report_tab_identity(
    sessions: Query<
        (
            &Children,
            Option<&AgentConversationTitle>,
            &AgentMessages,
            &AgentRunState,
            Option<&Profile>,
            Option<&AgentSession>,
        ),
        Or<(
            Changed<AgentConversationTitle>,
            Changed<AgentMessages>,
            Changed<AgentRunState>,
            Changed<Profile>,
        )>,
    >,
    views: Query<Option<&PageIdentity>, With<ChatView>>,
    mut commands: Commands,
) {
    for (children, title, messages, state, profile, session) in &sessions {
        for child in children.iter() {
            let Ok(reported) = views.get(child) else {
                continue;
            };
            let mut reported = reported.cloned().unwrap_or_default();
            if let Some(title) = title {
                reported.title = Some(title.0.clone());
            }
            reported.icon = tab_activity_icon(messages, state, profile, session);
            commands.entity(child).insert(reported);
        }
    }
}

fn tab_activity_icon(
    messages: &AgentMessages,
    state: &AgentRunState,
    profile: Option<&Profile>,
    session: Option<&AgentSession>,
) -> Option<PageIcon> {
    let running = matches!(state, AgentRunState::Streaming);
    let page = group_turns_tail(&[], &messages.0, &[], &[], running, TAB_ACTIVITY_TAIL_ITEMS);
    let activity = current_activity(&page.items, state.status())?;
    let provider = session
        .map(|session| session.provider.as_str())
        .unwrap_or_default();
    let accent = crate::tab::Accent::for_agent(
        profile
            .map(|profile| profile.avatar.color.as_str())
            .unwrap_or_default(),
        provider,
    );
    Some(PageIcon::favicon(
        ActivityIcon::from(activity).favicon(&accent.css),
    ))
}

#[derive(Component)]
#[require(
    ChatUiStateUpdates,
    ChatAttachmentProjection,
    ChatSnapshotProjection,
    ChatMediaProjection,
    ChatComposerContext,
    ComposerState,
    crate::prompt::ChatPromptFocusRevision,
    crate::key::ChatKeyEffectRevision
)]
pub struct ChatView;

#[derive(Component, Default)]
pub struct ChatComposerContext(pub ComposerContext);

#[derive(Component, Default)]
pub struct ChatSnapshotProjection(pub ChatSnapshot);

#[derive(Component, Default)]
pub struct ChatAttachmentProjection {
    pub selected: Vec<ChatAttachment>,
    pub previews: std::collections::HashMap<String, ChatAttachment>,
    pub pending: std::collections::HashSet<String>,
    pub resolved: std::collections::HashSet<String>,
}

#[derive(Component, Default)]
pub struct ChatMediaProjection(pub ChatMediaState);

#[derive(Component, Default)]
pub struct ChatTranscriptProjection {
    pub state: ChatTranscriptState,
    pub tail: Vec<ChatItem>,
    pub tail_start: u32,
}

pub struct TranscriptTail {
    pub items: Vec<ChatItem>,
    pub start: u32,
    pub total: u32,
}

pub struct TranscriptPage {
    pub items: Vec<ChatItem>,
    pub start: u32,
    pub end: u32,
    pub total: u32,
}

#[derive(Component)]
pub struct ChatHistoryQuery {
    pub webview: Entity,
    pub session: Entity,
    pub generation: u64,
    pub request_id: u64,
    pub before: u32,
    pub limit: u32,
}

#[derive(Component)]
pub struct ChatHistoryResult {
    pub webview: Entity,
    pub generation: u64,
    pub request_id: u64,
    pub page: Option<TranscriptPage>,
}

impl ChatTranscriptProjection {
    pub fn merge_tail(&mut self, tail: TranscriptTail) -> bool {
        if self.tail_start == tail.start
            && self.tail == tail.items
            && self.state.total == tail.total
        {
            return false;
        }
        let initialized = self.state.generation != 0;
        let compatible = initialized
            && tail.total >= self.state.total
            && self.state.loaded_start <= tail.start
            && tail.start.saturating_sub(self.state.loaded_start) as usize
                <= self.state.items.len();
        self.tail.clone_from(&tail.items);
        self.tail_start = tail.start;
        if compatible {
            let keep = tail.start.saturating_sub(self.state.loaded_start) as usize;
            self.state.items.truncate(keep);
            self.state.items.extend(tail.items);
        } else {
            self.state.generation = self.state.generation.wrapping_add(1).max(1);
            self.state.request_id = 0;
            self.state.prepend_revision = 0;
            self.state.items = tail.items;
            self.state.loaded_start = tail.start;
            self.state.loading = false;
        }
        self.state.total = tail.total;
        if self.state.loaded_start == 0 {
            self.state.loading = false;
        }
        self.refresh_activity();
        true
    }

    pub fn start_history_query(
        &mut self,
        webview: Entity,
        session: Entity,
    ) -> Option<ChatHistoryQuery> {
        if self.state.loaded_start == 0 || self.state.loading {
            return None;
        }
        self.state.request_id = self.state.request_id.wrapping_add(1).max(1);
        self.state.loading = true;
        Some(ChatHistoryQuery {
            webview,
            session,
            generation: self.state.generation,
            request_id: self.state.request_id,
            before: self.state.loaded_start,
            limit: CHAT_HISTORY_PAGE_SIZE.min(CHAT_HISTORY_MAX_PAGE_SIZE),
        })
    }

    pub fn finish_history_query(&mut self, result: &ChatHistoryResult) -> bool {
        if result.generation != self.state.generation
            || result.request_id != self.state.request_id
            || !self.state.loading
        {
            return false;
        }
        self.state.loading = false;
        let Some(page) = result.page.as_ref() else {
            return true;
        };
        if page.end != self.state.loaded_start || page.start > page.end || page.end > page.total {
            return true;
        }
        self.state.items.splice(0..0, page.items.iter().cloned());
        self.state.loaded_start = page.start;
        self.state.total = self.state.total.max(page.total);
        self.state.prepend_revision = self.state.prepend_revision.wrapping_add(1).max(1);
        self.refresh_activity();
        true
    }

    fn refresh_activity(&mut self) {
        let (subagents, tasks) = activity_counts(&self.state.items);
        self.state.active_subagents = subagents;
        self.state.active_tasks = tasks;
    }
}

#[derive(Component, Default)]
pub struct ChatResumeProjection(pub ChatResumeState);

impl ChatResumeProjection {
    pub fn start(&mut self, active: bool, query: String) -> Option<u64> {
        if self.0.active == active && self.0.query == query {
            return None;
        }
        self.0.request_id = self.0.request_id.wrapping_add(1).max(1);
        self.0.active = active;
        self.0.query = query;
        self.0.sessions.clear();
        self.0.total = 0;
        self.0.loading = active;
        Some(self.0.request_id)
    }

    pub fn finish(&mut self, sessions: &ResumableSessions) -> bool {
        if self.0.request_id != sessions.request_id
            || self.0.query != sessions.query
            || !self.0.active
        {
            return false;
        }
        self.0.sessions.clone_from(&sessions.sessions);
        self.0.total = sessions.total;
        self.0.loading = false;
        true
    }
}

#[derive(Component, Default)]
pub struct ChatBranchesProjection(pub ChatBranchesState);

impl ChatBranchesProjection {
    pub fn start(&mut self, project: String) -> u64 {
        self.0.request_id = self.0.request_id.wrapping_add(1).max(1);
        self.0.project = project;
        self.0.branches.clear();
        self.0.loading = true;
        self.0.request_id
    }

    pub fn finish(&mut self, request_id: u64, project: &str, branches: Vec<ChatBranch>) -> bool {
        if self.0.request_id != request_id || self.0.project != project {
            return false;
        }
        self.0.branches = branches;
        self.0.loading = false;
        true
    }
}

#[derive(Component)]
pub struct ChatSynced;

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::chat::{ChatBlock, ChatTurn};

    struct Conversation {
        view: Entity,
        session: Entity,
    }

    impl Conversation {
        fn start(app: &mut App) -> Self {
            app.add_systems(Update, report_tab_identity);
            let session = app
                .world_mut()
                .spawn((
                    vmux_session::AgentMessages::default(),
                    vmux_session::AgentRunState::default(),
                ))
                .id();
            let view = app.world_mut().spawn((ChatView, ChildOf(session))).id();
            Self { view, session }
        }

        fn rename(&self, app: &mut App, title: &str) {
            app.world_mut()
                .entity_mut(self.session)
                .insert(vmux_session::AgentConversationTitle(title.to_string()));
            app.update();
        }

        fn run(&self, app: &mut App, state: vmux_session::AgentRunState) {
            app.world_mut().entity_mut(self.session).insert(state);
            app.update();
        }

        fn reported(&self, app: &App) -> vmux_core::PageIdentity {
            app.world()
                .get::<vmux_core::PageIdentity>(self.view)
                .cloned()
                .unwrap_or_default()
        }
    }

    #[test]
    fn naming_a_conversation_renames_the_view_not_the_session() {
        let mut app = App::new();
        let conversation = Conversation::start(&mut app);
        conversation.rename(&mut app, "ship the relay");

        assert_eq!(
            conversation.reported(&app).title.as_deref(),
            Some("ship the relay")
        );
        assert!(
            app.world()
                .get::<vmux_core::PageIdentity>(conversation.session)
                .is_none()
        );
    }

    #[test]
    fn renaming_again_replaces_the_reported_title() {
        let mut app = App::new();
        let conversation = Conversation::start(&mut app);
        conversation.rename(&mut app, "first guess");
        conversation.rename(&mut app, "what it turned out to be");

        assert_eq!(
            conversation.reported(&app).title.as_deref(),
            Some("what it turned out to be")
        );
    }

    #[test]
    fn an_idle_agent_reports_no_icon_so_its_own_shows_through() {
        let mut app = App::new();
        let conversation = Conversation::start(&mut app);
        conversation.run(&mut app, vmux_session::AgentRunState::Idle);

        assert_eq!(conversation.reported(&app).icon, None);
    }

    #[test]
    fn the_icon_tracks_what_the_agent_is_doing() {
        let mut app = App::new();
        let conversation = Conversation::start(&mut app);

        conversation.run(
            &mut app,
            vmux_session::AgentRunState::AwaitingApproval {
                call_id: "1".into(),
                name: "run".into(),
                args: serde_json::Value::Null,
            },
        );
        let awaiting = conversation.reported(&app).icon;
        assert!(awaiting.is_some());

        conversation.run(
            &mut app,
            vmux_session::AgentRunState::Errored("boom".into()),
        );
        assert_ne!(conversation.reported(&app).icon, awaiting);

        conversation.run(&mut app, vmux_session::AgentRunState::Idle);
        assert_eq!(conversation.reported(&app).icon, None);
    }

    #[test]
    fn a_streaming_agent_is_read_from_the_last_block_of_the_running_turn() {
        let mut thinking_turn = ChatTurn {
            running: true,
            blocks: vec![ChatBlock::Thinking(String::new())],
            ..Default::default()
        };
        vmux_core::chat_projection::project_turn(&mut thinking_turn);
        let thinking = vmux_core::chat_projection::current_activity(
            &[vmux_api::chat::ChatItem::Turn(thinking_turn)],
            "streaming",
        )
        .map(ActivityIcon::from);
        let mut writing_turn = ChatTurn {
            running: true,
            blocks: vec![
                ChatBlock::Thinking(String::new()),
                ChatBlock::Text(String::new()),
            ],
            ..Default::default()
        };
        vmux_core::chat_projection::project_turn(&mut writing_turn);
        let writing = vmux_core::chat_projection::current_activity(
            &[vmux_api::chat::ChatItem::Turn(writing_turn)],
            "streaming",
        )
        .map(ActivityIcon::from);

        assert_eq!(thinking, Some(ActivityIcon::Thinking));
        assert_eq!(writing, Some(ActivityIcon::Writing));
    }
}
