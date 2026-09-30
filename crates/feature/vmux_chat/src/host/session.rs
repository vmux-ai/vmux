#[cfg(host)]
use bevy_app::{App, Plugin, Update};
#[cfg(host)]
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;

use super::composer::{ComposerSelectors, ComposerState};
use super::key::{ChatListSelection, ChatSelectorProjection};
use crate::activity::ActivityIcon;
use crate::event::{
    CHAT_HISTORY_MAX_PAGE_SIZE, CHAT_HISTORY_PAGE_SIZE, ChatAttachment, ChatBranch,
    ChatBranchesState, ChatItem, ChatMediaState, ChatOpenPage, ChatResumeState, ChatSnapshot,
    ChatTranscriptState, ComposerContext, ResumableSessions,
};
use crate::state::ChatUiState;
use vmux_api::ProcessId;
use vmux_api::command_bar::PromptRequest;
use vmux_api::protocol::AgentCommandResult;
use vmux_command::command_bar::CommandBarDismiss;
use vmux_command::snapshot::{CommandBarProjection, ContributedPages};
use vmux_core::agent::{
    AgentCommandResponse, AgentContinuationRequest, AgentRequestAppExt, AgentRequestMessage,
    AgentRequestRouteSet, AgentSessionRoot,
};
use vmux_core::chat::group_turns_tail;
use vmux_core::chat_projection::{activity_counts, current_activity};
use vmux_core::host::UiState;
use vmux_core::launcher::{HostsLauncher, InlineTransitionRequested};
use vmux_core::team::Profile;
use vmux_core::{
    PageIcon, PageIdentity, PageOpenRequest, PageOpenTarget, PendingPrompt,
    PendingPromptAttachments,
};
use vmux_layout::stack::OpenRequest;
use vmux_session::{AcpSession, AgentConversationTitle, AgentMessages, AgentRunState};

type ChatUiStateUpdates = UiState<ChatUiState>;

#[vmux_api::agent]
pub(crate) struct AgentRequestUserChoice {
    pub anchor: ProcessId,
    pub question: String,
    pub options: Vec<String>,
}

#[vmux_api::agent]
pub(crate) struct AgentSetConversationTitle {
    pub anchor: ProcessId,
    pub title: String,
}

#[cfg(host)]
pub struct ChatPlugin;

#[cfg(host)]
impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ChatAgentPlugin,
            super::ChatStatePlugin,
            super::key::ChatKeyPlugin,
            super::media::ChatMediaPlugin,
            super::tool::ChatToolPlugin,
            super::composer::ChatComposerPlugin,
            super::prompt::ChatPromptInputPlugin,
        ))
        .add_plugins(UiEventPlugin::<(ChatOpenPage, PromptRequest)>::default())
        .add_observer(open_page)
        .add_observer(submit_from_command_bar)
        .add_systems(Update, report_tab_identity);
    }
}

fn submit_from_command_bar(
    trigger: On<UiInput<PromptRequest>>,
    launcher_hosts: Query<(), With<HostsLauncher>>,
    child_of: Query<&ChildOf>,
    contributed_pages: ContributedPages,
    command_bar: Single<&CommandBarProjection>,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut inline_transition: MessageWriter<InlineTransitionRequested>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let request = &trigger.event().payload;
    let prompt = request.text.trim();
    let attachments = request
        .attachments
        .iter()
        .filter(|attachment| !attachment.path.is_empty())
        .map(|attachment| vmux_api::protocol::AgentAttachment {
            path: attachment.path.clone(),
            name: attachment.name.clone(),
            mime_type: attachment.mime_type.clone(),
            size: attachment.size,
        })
        .collect::<Vec<_>>();
    let inline_stack = launcher_hosts
        .contains(webview)
        .then(|| child_of.get(webview).ok().map(|parent| parent.0))
        .flatten();
    let mut opened = false;
    if (!prompt.is_empty() || !attachments.is_empty())
        && let Some(stack) = command_bar.workspace.stack
        && let Some(url) = contributed_pages.prompt_url(request.target_url.as_deref())
    {
        if inline_stack == Some(stack) && vmux_api::agent::supports_inline_agent_transition(&url) {
            inline_transition.write(InlineTransitionRequested { stack, webview });
            if let Some(proxy) = proxy.as_deref() {
                let _ = (**proxy).send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
        }
        commands
            .entity(stack)
            .insert(PendingPrompt(prompt.to_string()));
        if attachments.is_empty() {
            commands.entity(stack).remove::<PendingPromptAttachments>();
        } else {
            commands
                .entity(stack)
                .insert(PendingPromptAttachments(attachments));
        }
        page_open_requests.write(PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url,
            request_id: None,
        });
        opened = true;
    }
    commands.trigger(CommandBarDismiss::new(webview, !opened));
}

struct ChatAgentPlugin;

impl Plugin for ChatAgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_agent_request::<AgentRequestUserChoice>()
            .add_agent_request::<AgentSetConversationTitle>()
            .add_message::<AgentContinuationRequest>()
            .add_observer(resume_agent_choice)
            .add_systems(
                Update,
                (request_user_choice, set_conversation_title).after(AgentRequestRouteSet),
            );
    }
}

pub const USER_CHOICE_REQUESTED: &str = "User choice requested. Stop this turn and wait. vmux will resume this same conversation with the selected option.";

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct PendingAgentChoice {
    pub session_entity: Entity,
    pub question: String,
    pub options: Vec<String>,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResumeAgentChoice;

#[derive(SystemParam)]
struct AgentChatTarget<'w, 's> {
    anchors: Query<'w, 's, (Entity, &'static ProcessId)>,
    child_of: Query<'w, 's, &'static ChildOf>,
    session_roots: Query<'w, 's, (), With<AgentSessionRoot>>,
}

impl AgentChatTarget<'_, '_> {
    fn resolve(&self, anchor: ProcessId) -> Option<(Entity, Entity)> {
        let agent = self
            .anchors
            .iter()
            .find_map(|(entity, process_id)| (*process_id == anchor).then_some(entity))?;
        let mut current = agent;
        loop {
            if self.session_roots.contains(current) {
                return Some((agent, current));
            }
            current = self.child_of.get(current).ok()?.parent();
        }
    }
}

fn request_user_choice(
    mut requests: MessageReader<AgentRequestMessage<AgentRequestUserChoice>>,
    targets: AgentChatTarget,
    choices: Query<(), With<PendingAgentChoice>>,
    views: Query<(), With<ChatView>>,
    mut responses: MessageWriter<AgentCommandResponse>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let result = match targets.resolve(request.payload.anchor) {
            None => AgentCommandResult::Error("agent pane not found".to_string()),
            Some((agent, _)) if choices.contains(agent) => {
                AgentCommandResult::Text(USER_CHOICE_REQUESTED.to_string())
            }
            Some((agent, session)) if views.contains(agent) => {
                commands
                    .entity(agent)
                    .insert((
                        PendingAgentChoice {
                            session_entity: session,
                            question: request.payload.question.clone(),
                            options: request.payload.options.clone(),
                        },
                        ResumeAgentChoice,
                    ))
                    .remove::<ChatSynced>();
                AgentCommandResult::Text(USER_CHOICE_REQUESTED.to_string())
            }
            Some(_) => AgentCommandResult::Error(
                "Native choice prompts require the chat agent view; ask the user with the same numbered options in the current terminal session."
                    .to_string(),
            ),
        };
        responses.write(request.reply.response(result));
    }
}

fn set_conversation_title(
    mut requests: MessageReader<AgentRequestMessage<AgentSetConversationTitle>>,
    targets: AgentChatTarget,
    mut titles: Query<&mut AgentConversationTitle>,
    mut responses: MessageWriter<AgentCommandResponse>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let title = request.payload.title.trim();
        let result = if title.is_empty() {
            AgentCommandResult::Error("conversation title is empty".to_string())
        } else {
            match targets.resolve(request.payload.anchor) {
                None => AgentCommandResult::Error("agent pane not found".to_string()),
                Some((_, session)) => {
                    if let Ok(mut current) = titles.get_mut(session) {
                        current.0 = title.to_string();
                    } else {
                        commands
                            .entity(session)
                            .insert(AgentConversationTitle(title.to_string()));
                    }
                    AgentCommandResult::Ok
                }
            }
        };
        responses.write(request.reply.response(result));
    }
}

fn resume_agent_choice(
    trigger: On<UiInput<crate::event::ChatChoiceSelected>>,
    choices: Query<&PendingAgentChoice, With<ResumeAgentChoice>>,
    mut continuations: MessageWriter<AgentContinuationRequest>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok(choice) = choices.get(event.webview) else {
        return;
    };
    let Some(selected) = choice.options.get(event.payload.index as usize) else {
        return;
    };
    continuations.write(AgentContinuationRequest {
        session: choice.session_entity,
        context: format!(
            "VMUX USER CHOICE: For \"{}\", the user selected \"{}\". Continue the original request in this same conversation.",
            choice.question, selected
        ),
    });
    commands
        .entity(event.webview)
        .remove::<(PendingAgentChoice, ResumeAgentChoice)>()
        .remove::<ChatSynced>();
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

type ChangedChatSessions<'w, 's> = Query<
    'w,
    's,
    (
        &'static Children,
        Option<&'static AgentConversationTitle>,
        &'static AgentMessages,
        &'static AgentRunState,
        Option<&'static Profile>,
        &'static AcpSession,
    ),
    Or<(
        Changed<AgentConversationTitle>,
        Changed<AgentMessages>,
        Changed<AgentRunState>,
        Changed<Profile>,
    )>,
>;

fn report_tab_identity(
    sessions: ChangedChatSessions,
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
    session: &AcpSession,
) -> Option<PageIcon> {
    let running = matches!(state, AgentRunState::Streaming);
    let page = group_turns_tail(&[], &messages.0, &[], &[], running, TAB_ACTIVITY_TAIL_ITEMS);
    let activity = current_activity(&page.items, state.status())?;
    let accent = crate::tab::Accent::for_agent(
        profile
            .map(|profile| profile.avatar.color.as_str())
            .unwrap_or_default(),
        &session.agent_id,
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
    ComposerSelectors,
    super::prompt::ChatPromptFocusRevision,
    super::key::ActiveComposerMenu,
    ChatListSelection,
    ChatSelectorProjection
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
    pub fn prompt_history(&self, snapshot: &ChatSnapshotProjection) -> Vec<String> {
        let mut history = Vec::new();
        for item in &self.state.items {
            let ChatItem::User { text, .. } = item else {
                continue;
            };
            if !text.trim().is_empty() {
                history.push(text.clone());
            }
        }
        for prompt in &snapshot.0.queued {
            if !prompt.text.trim().is_empty() {
                history.push(prompt.text.clone());
            }
        }
        history
    }

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
        self.0.rows.clear();
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
        self.0.rows = vmux_core::chat_projection::ResumeRows::all(&sessions.sessions);
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
    use vmux_api::protocol::{AgentRequest, AgentRequestId};
    use vmux_core::ProcessId;
    use vmux_core::agent::{AgentRequestInput, CommandOrigin};

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
                    AcpSession {
                        agent_id: "mock".into(),
                        sid: "session".into(),
                        cwd: std::path::PathBuf::from("/tmp"),
                        anchor: ProcessId::new(),
                        resume: None,
                    },
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

    #[test]
    fn agent_title_request_updates_the_session_root() {
        let mut app = App::new();
        app.add_plugins(ChatAgentPlugin);
        let anchor = ProcessId::new();
        let session = app.world_mut().spawn(AgentSessionRoot).id();
        app.world_mut().spawn((anchor, ChatView, ChildOf(session)));
        app.world_mut().write_message(AgentRequestInput {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::Agent {
                sid: None,
                anchor: Some(anchor),
            },
            request: AgentRequest::encode(&AgentSetConversationTitle {
                anchor,
                title: "Feature-owned title".to_string(),
            })
            .unwrap(),
        });

        app.update();

        assert_eq!(
            app.world().get::<AgentConversationTitle>(session),
            Some(&AgentConversationTitle("Feature-owned title".to_string()))
        );
    }

    #[test]
    fn agent_choice_request_and_selection_emit_a_continuation() {
        let mut app = App::new();
        app.add_plugins(ChatAgentPlugin);
        let anchor = ProcessId::new();
        let session = app.world_mut().spawn(AgentSessionRoot).id();
        let webview = app
            .world_mut()
            .spawn((anchor, ChatView, ChatSynced, ChildOf(session)))
            .id();
        app.world_mut().write_message(AgentRequestInput {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::Agent {
                sid: None,
                anchor: Some(anchor),
            },
            request: AgentRequest::encode(&AgentRequestUserChoice {
                anchor,
                question: "Mode?".to_string(),
                options: vec!["Fast".to_string(), "Safe".to_string()],
            })
            .unwrap(),
        });

        app.update();
        assert!(app.world().get::<PendingAgentChoice>(webview).is_some());
        assert!(app.world().get::<ChatSynced>(webview).is_none());

        app.world_mut().trigger(UiInput {
            webview,
            payload: crate::event::ChatChoiceSelected { index: 1 },
        });
        app.world_mut().flush();

        let continuations = app
            .world_mut()
            .resource_mut::<Messages<AgentContinuationRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(continuations.len(), 1);
        assert_eq!(continuations[0].session, session);
        assert!(continuations[0].context.contains("Safe"));
        assert!(app.world().get::<PendingAgentChoice>(webview).is_none());
    }
}
