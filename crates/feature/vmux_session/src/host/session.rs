#[cfg(host)]
use bevy_app::{App, Plugin, Update};
#[cfg(host)]
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;

use super::composer::{ComposerSelectors, ComposerState};
use super::key::{ChatListSelection, ChatSelectorProjection};
use super::projection::ChatTurnProjection;
use crate::event::{
    CHAT_HISTORY_MAX_PAGE_SIZE, CHAT_HISTORY_PAGE_SIZE, ChatAttachment, ChatBranch,
    ChatBranchesState, ChatItem, ChatMediaState, ChatResumeState, ChatSnapshot,
    ChatTranscriptState, ComposerContext, ResumableSessions,
};
use crate::state::UiChat;
use vmux_api::avatar::IdenticonSpec;
use vmux_api::command_bar::PromptRequest;
use vmux_api::protocol::AgentCommandResult;
use vmux_api::{PageIcon, ProcessId};
use vmux_command::CommandBarDismiss;
use vmux_command::{CommandBarWorkspaceSnapshot, ContributedPages};
use vmux_ecs::UiState;
use vmux_ecs::agent::{
    AgentCommandResponse, AgentContinuationRequest, AgentRequestAppExt, AgentRequestMessage,
    AgentRequestRouteSet, AgentSessionRoot,
};
use vmux_ecs::launcher::{HostsLauncher, InlineTransitionRequested};
use vmux_ecs::{
    Description, EntityTarget, PageIdentity, PageOpenRequest, PageOpenTarget, PendingPrompt,
    PendingPromptAttachments,
};
use vmux_session::{
    AgentConversationTitle, AgentId, CreateRequest, Created, PromptQueue, Route, Session, SessionId,
};

type UiChatUpdates = UiState<UiChat>;

#[derive(SystemParam)]
pub struct SessionViews<'w, 's> {
    child_of: Query<'w, 's, &'static ChildOf>,
    targets: Query<'w, 's, &'static EntityTarget<Session>>,
}

impl SessionViews<'_, '_> {
    pub fn session(&self, webview: Entity) -> Option<Entity> {
        let stack = self.child_of.get(webview).ok()?.parent();
        self.targets.get(stack).ok().map(EntityTarget::entity)
    }
}

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
    pub summary: String,
}

#[cfg(host)]
pub(super) struct ChatHostPlugin;

#[cfg(host)]
impl Plugin for ChatHostPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingSessionStarts>()
            .add_message::<StartRequest>()
            .add_plugins((
                AgentProjectionPlugin,
                super::key::ChatKeyPlugin,
                super::media::ChatMediaPlugin,
                super::tool::ChatToolPlugin,
                super::composer::ChatComposerPlugin,
                super::prompt::ChatPromptInputPlugin,
            ))
            .add_plugins(UiEventPlugin::<(PromptRequest,)>::default())
            .add_observer(open_started)
            .add_observer(submit_from_command_bar)
            .add_systems(
                Update,
                (start.before(crate::model::MutationSet), report_tab_identity),
            );
    }
}

#[derive(Message, Clone, Debug)]
pub struct StartRequest {
    create: CreateRequest,
    target: PageOpenTarget,
    prompt: String,
    attachments: Vec<vmux_api::protocol::AgentAttachment>,
}

impl StartRequest {
    pub fn from_prompt(
        prompt: impl Into<String>,
        cwd: std::path::PathBuf,
        agent: AgentId,
        target: PageOpenTarget,
        attachments: Vec<vmux_api::protocol::AgentAttachment>,
    ) -> Self {
        let prompt = prompt.into();
        let name = AgentConversationTitle::from_prompt(&prompt)
            .map(|title| title.0)
            .unwrap_or_else(|| vmux_ui::i18n::translate("sessions-new"));
        Self {
            create: CreateRequest::new(name, String::new(), cwd, Some(agent)),
            target,
            prompt,
            attachments,
        }
    }

    pub fn id(&self) -> &SessionId {
        self.create.id()
    }
}

#[derive(Resource, Default)]
struct PendingSessionStarts(std::collections::HashMap<SessionId, PendingSessionStart>);

struct PendingSessionStart {
    target: PageOpenTarget,
    prompt: String,
    attachments: Vec<vmux_api::protocol::AgentAttachment>,
}

fn start(
    mut requests: MessageReader<StartRequest>,
    mut creates: MessageWriter<CreateRequest>,
    mut pending: ResMut<PendingSessionStarts>,
) {
    for request in requests.read() {
        pending.0.insert(
            request.create.id().clone(),
            PendingSessionStart {
                target: request.target.clone(),
                prompt: request.prompt.clone(),
                attachments: request.attachments.clone(),
            },
        );
        creates.write(request.create.clone());
    }
}

fn open_started(
    trigger: On<Created>,
    mut pending: ResMut<PendingSessionStarts>,
    mut requests: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    let Some(created) = pending.0.remove(&trigger.event().id) else {
        return;
    };
    let mut queue = PromptQueue::default();
    queue.enqueue_with_attachments(created.prompt.clone(), created.attachments);
    commands.entity(trigger.event().entity).insert(queue);
    if let Some(title) = AgentConversationTitle::from_prompt(&created.prompt) {
        commands
            .entity(trigger.event().entity)
            .insert((title, vmux_session::TitlePending));
    }
    requests.write(PageOpenRequest {
        target: created.target,
        url: Route::Session(trigger.event().id.clone()).url(),
        request_id: None,
    });
}

fn submit_from_command_bar(
    trigger: On<UiInput<PromptRequest>>,
    target: PromptTarget,
    mut submission: PromptSubmission,
) {
    let webview = trigger.event().webview;
    let request = &trigger.event().payload;
    submission.submit(webview, request, &target);
}

#[derive(SystemParam)]
struct PromptSubmission<'w, 's> {
    session_starts: MessageWriter<'w, StartRequest>,
    page_open_requests: MessageWriter<'w, PageOpenRequest>,
    inline_transition: MessageWriter<'w, InlineTransitionRequested>,
    proxy: Option<Res<'w, bevy::winit::EventLoopProxyWrapper>>,
    commands: Commands<'w, 's>,
}

impl PromptSubmission<'_, '_> {
    fn submit(&mut self, webview: Entity, request: &PromptRequest, target: &PromptTarget) {
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
        let inline_stack = target
            .launcher_hosts
            .contains(webview)
            .then(|| target.child_of.get(webview).ok().map(|parent| parent.0))
            .flatten();
        let mut opened = false;
        if (!prompt.is_empty() || !attachments.is_empty())
            && let Some(stack) = target.workspace.stack
            && let Some(url) = target
                .contributed_pages
                .prompt_url(request.target_url.as_deref())
        {
            let mut created = false;
            if let Some(agent) = Route::requested_agent(&url) {
                self.session_starts.write(StartRequest::from_prompt(
                    prompt,
                    target.cwd(stack),
                    agent,
                    PageOpenTarget::Stack(stack),
                    attachments.clone(),
                ));
                created = true;
            }
            if inline_stack == Some(stack)
                && vmux_api::VmuxRoute::parse(&url)
                    .is_some_and(|route| route.supports_inline_transition())
            {
                self.inline_transition
                    .write(InlineTransitionRequested { stack, webview });
                if let Some(proxy) = self.proxy.as_deref() {
                    let _ = (**proxy).send_event(bevy::winit::WinitUserEvent::WakeUp);
                }
            }
            if !created {
                self.commands
                    .entity(stack)
                    .insert(PendingPrompt(prompt.to_string()));
                if attachments.is_empty() {
                    self.commands
                        .entity(stack)
                        .remove::<PendingPromptAttachments>();
                } else {
                    self.commands
                        .entity(stack)
                        .insert(PendingPromptAttachments(attachments));
                }
                self.page_open_requests.write(PageOpenRequest {
                    target: PageOpenTarget::Stack(stack),
                    url,
                    request_id: None,
                });
            }
            opened = true;
        }
        self.commands
            .trigger(CommandBarDismiss::new(webview, !opened));
    }
}

#[derive(SystemParam)]
struct PromptTarget<'w, 's> {
    launcher_hosts: Query<'w, 's, (), With<HostsLauncher>>,
    child_of: Query<'w, 's, &'static ChildOf>,
    contributed_pages: ContributedPages<'w, 's>,
    work_directories: Query<'w, 's, &'static vmux_command::CommandBarWorkDirectory>,
    workspace: Single<'w, 's, &'static CommandBarWorkspaceSnapshot>,
}

impl PromptTarget<'_, '_> {
    fn cwd(&self, stack: Entity) -> std::path::PathBuf {
        self.work_directories
            .get(stack)
            .map(|cwd| std::path::PathBuf::from(&cwd.0))
            .ok()
            .or_else(|| self.workspace.project_root.as_deref().map(Into::into))
            .unwrap_or_default()
    }
}

struct AgentProjectionPlugin;

impl Plugin for AgentProjectionPlugin {
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
    targets: Query<'w, 's, &'static EntityTarget<Session>>,
    session_roots: Query<'w, 's, (), With<AgentSessionRoot>>,
}

impl AgentChatTarget<'_, '_> {
    fn resolve(&self, anchor: ProcessId) -> Option<(Entity, Entity)> {
        let agent = self
            .anchors
            .iter()
            .find_map(|(entity, process_id)| (*process_id == anchor).then_some(entity))?;
        let stack = self.child_of.get(agent).ok()?.parent();
        let session = self.targets.get(stack).ok()?.entity();
        self.session_roots
            .contains(session)
            .then_some((agent, session))
    }
}

fn request_user_choice(
    mut requests: MessageReader<AgentRequestMessage<AgentRequestUserChoice>>,
    targets: AgentChatTarget,
    choices: Query<(), With<PendingAgentChoice>>,
    views: Query<(), With<SessionView>>,
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
    mut metadata: Query<(Option<&mut AgentConversationTitle>, &mut Description)>,
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
                    let summary = request.payload.summary.trim();
                    if summary.is_empty() {
                        AgentCommandResult::Error("conversation summary is empty".to_string())
                    } else if let Ok((current, mut description)) = metadata.get_mut(session) {
                        if let Some(mut current) = current {
                            current.0 = title.to_string();
                        } else {
                            commands
                                .entity(session)
                                .insert(AgentConversationTitle(title.to_string()));
                        }
                        description.0 = summary.to_string();
                        commands
                            .entity(session)
                            .remove::<vmux_session::TitlePending>();
                        AgentCommandResult::Ok
                    } else {
                        AgentCommandResult::Error("session metadata not found".to_string())
                    }
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

type ChatSessions<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static AgentConversationTitle>,
        &'static SessionId,
    ),
    With<Session>,
>;

type ChangedChatSessions<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<Session>,
        Or<(
            Changed<AgentConversationTitle>,
            Changed<SessionId>,
            Changed<Children>,
        )>,
    ),
>;

fn report_tab_identity(
    sessions: ChatSessions,
    changed_sessions: ChangedChatSessions,
    stacks: Query<(&EntityTarget<Session>, &Children)>,
    views: Query<Option<&PageIdentity>, With<SessionView>>,
    mut commands: Commands,
) {
    for session_entity in &changed_sessions {
        let Ok((_, title, id)) = sessions.get(session_entity) else {
            continue;
        };
        for (target, children) in &stacks {
            if target.entity() != session_entity {
                continue;
            }
            for child in children.iter() {
                let Ok(current) = views.get(child) else {
                    continue;
                };
                let mut reported = current.cloned().unwrap_or_default();
                if let Some(title) = title {
                    reported.title = Some(title.0.clone());
                }
                reported.icon = Some(PageIcon::favicon(IdenticonSpec::favicon(&id.0)));
                if current != Some(&reported) {
                    commands.entity(child).insert(reported);
                }
            }
        }
    }
}

#[derive(Component)]
#[require(
    UiChatUpdates,
    ChatAttachmentProjection,
    ChatSnapshotProjection,
    ChatTranscriptProjection,
    ChatMediaProjection,
    ChatBranchesProjection,
    ChatResumeProjection,
    ChatComposerContext,
    ComposerState,
    ComposerSelectors,
    super::prompt::ChatPromptFocusRevision,
    super::key::ActiveComposerMenu,
    ChatListSelection,
    ChatSelectorProjection
)]
pub struct SessionView;

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
        let (subagents, tasks) = ChatTurnProjection::activity_counts(&self.state.items);
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
        self.0.rows = super::command_bar::ResumeRows::project(&sessions.sessions);
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
    use vmux_api::protocol::{AgentRequest, AgentRequestId};
    use vmux_ecs::agent::{AgentRequestInput, CommandOrigin};

    #[test]
    fn created_session_owns_the_initial_prompt_before_opening() {
        let mut app = App::new();
        app.init_resource::<PendingSessionStarts>()
            .add_message::<StartRequest>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_observer(open_started)
            .add_systems(Update, start);
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<Messages<StartRequest>>()
            .write(StartRequest::from_prompt(
                "Fix session startup",
                std::path::PathBuf::new(),
                AgentId("mock".into()),
                PageOpenTarget::Stack(stack),
                vec![vmux_api::protocol::AgentAttachment {
                    path: "/tmp/screenshot.png".into(),
                    name: "screenshot.png".into(),
                    mime_type: "image/png".into(),
                    size: 42,
                }],
            ));
        app.update();
        let create = app
            .world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .drain()
            .next()
            .unwrap();
        let id = create.id().clone();
        let session = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(Created {
            entity: session,
            id: id.clone(),
        });
        app.update();

        let queue = app.world().get::<PromptQueue>(session).unwrap();
        assert_eq!(queue.items.len(), 1);
        assert_eq!(queue.items.front().unwrap().text, "Fix session startup");
        assert_eq!(queue.items.front().unwrap().attachments.len(), 1);
        assert_eq!(
            app.world().get::<AgentConversationTitle>(session),
            Some(&AgentConversationTitle("Fix session startup".into()))
        );
        assert!(
            app.world()
                .entity(session)
                .contains::<vmux_session::TitlePending>()
        );
        let requests = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, Route::Session(id).url());
        assert!(matches!(requests[0].target, PageOpenTarget::Stack(target) if target == stack));
    }

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
                    Session,
                    vmux_session::SessionId("session".into()),
                    AgentId("mock".into()),
                    vmux_session::RunState::default(),
                ))
                .id();
            let stack = app
                .world_mut()
                .spawn(vmux_ecs::EntityTarget::<Session>::new(session))
                .id();
            let view = app.world_mut().spawn((SessionView, ChildOf(stack))).id();
            Self { view, session }
        }

        fn rename(&self, app: &mut App, title: &str) {
            app.world_mut()
                .entity_mut(self.session)
                .insert(vmux_session::AgentConversationTitle(title.to_string()));
            app.update();
        }

        fn run(&self, app: &mut App, state: vmux_session::RunState) {
            app.world_mut().entity_mut(self.session).insert(state);
            app.update();
        }

        fn reported(&self, app: &App) -> vmux_ecs::PageIdentity {
            app.world()
                .get::<vmux_ecs::PageIdentity>(self.view)
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
                .get::<vmux_ecs::PageIdentity>(conversation.session)
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
    fn agent_activity_keeps_the_session_identicon_visible() {
        let mut app = App::new();
        let conversation = Conversation::start(&mut app);
        conversation.rename(&mut app, "working session");
        let favicon = conversation.reported(&app).icon;
        conversation.run(
            &mut app,
            vmux_session::RunState::AwaitingApproval {
                call_id: "1".into(),
                name: "run".into(),
                args: serde_json::Value::Null,
            },
        );
        assert_eq!(conversation.reported(&app).icon, favicon);
        assert_eq!(
            favicon,
            Some(vmux_api::PageIcon::favicon(IdenticonSpec::favicon(
                "session"
            )))
        );
    }

    #[test]
    fn agent_title_request_updates_the_session_root() {
        let mut app = App::new();
        app.add_plugins(AgentProjectionPlugin);
        let anchor = ProcessId::new();
        let session = app
            .world_mut()
            .spawn((Session, AgentSessionRoot, vmux_session::TitlePending))
            .id();
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
            .id();
        app.world_mut().spawn((anchor, SessionView, ChildOf(stack)));
        app.world_mut().write_message(AgentRequestInput {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::Agent {
                sid: None,
                anchor: Some(anchor),
            },
            request: AgentRequest::encode(&AgentSetConversationTitle {
                anchor,
                title: "Feature-owned title".to_string(),
                summary: "Tracks the current feature-owned session outcome.".to_string(),
            })
            .unwrap(),
        });

        app.update();

        assert_eq!(
            app.world().get::<AgentConversationTitle>(session),
            Some(&AgentConversationTitle("Feature-owned title".to_string()))
        );
        assert_eq!(
            app.world().get::<Description>(session),
            Some(&Description(
                "Tracks the current feature-owned session outcome.".to_string()
            ))
        );
        assert!(
            !app.world()
                .entity(session)
                .contains::<vmux_session::TitlePending>()
        );
    }

    #[test]
    fn agent_choice_request_and_selection_emit_a_continuation() {
        let mut app = App::new();
        app.add_plugins(AgentProjectionPlugin);
        let anchor = ProcessId::new();
        let session = app.world_mut().spawn((Session, AgentSessionRoot)).id();
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
            .id();
        let webview = app
            .world_mut()
            .spawn((anchor, SessionView, ChatSynced, ChildOf(stack)))
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
