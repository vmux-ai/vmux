use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use super::group::ChatMessages;
use super::presentation::ChatPresentation;
#[cfg(test)]
use crate::event::ChatItem;
use crate::event::{
    CHAT_INITIAL_ITEM_LIMIT, ChatHistoryMoreRequest, ChatSnapshot, ChatViewport,
    ChatViewportChanged, ChatViewportState, PendingApproval, QueuedPromptSnapshot,
};
use crate::host::{
    ChatAttachmentHydrationRequest, HandoffEvent, HandoffMessages, ImportedConversation,
};
use crate::host::{
    ChatAttachmentProjection, ChatHistoryQuery, ChatHistoryResult, ChatSnapshotProjection,
    ChatSynced, ChatTranscriptProjection, PendingAgentChoice, SessionView, TranscriptPage,
    TranscriptTail,
};
use vmux_api::open_target::{OpenTarget, PaneDirection, PaneOpenMode, PaneTarget};
use vmux_ecs::service::ServiceMessageSet;
use vmux_ecs::team::{Profile, User};
use vmux_ecs::{CreatedAt, EntityTarget, PageMetadata};
use vmux_session::{
    AgentConversationTitle, AgentId, ConversationEvent, EventIdentity, MessageContent, PromptQueue,
    QueuedPromptReads, Route, Session, SessionId, Transcripts,
};
use vmux_session::{AgentTurnMeta, RunState};

pub(super) struct TranscriptPlugin;

impl bevy::prelude::Plugin for TranscriptPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            ChatHistoryMoreRequest,
            ChatViewportChanged,
            crate::event::ChatToolFileOpen,
        )>::default())
            .add_observer(request_more)
            .add_observer(open_file)
            .add_observer(remember_viewport)
            .add_observer(reset_synced)
            .add_observer(unbind)
            .add_observer(retire_handoff_events)
            .add_systems(
                Update,
                (
                    track_turn_duration,
                    reset_navigated,
                    attach,
                    bevy::ecs::schedule::ApplyDeferred,
                    push_to_page,
                    sync_ready_views,
                    resolve_queries,
                    bevy::ecs::schedule::ApplyDeferred,
                    apply_results,
                    sync_history,
                )
                    .chain()
                    .after(ServiceMessageSet)
                    .after(vmux_ecs::PageOpenSet::HandleKnownPages)
                    .after(vmux_session::TranscriptReadySet)
                    .after(vmux_session::history::Ready),
            );
    }
}

#[derive(Component, Default)]
struct PushState {
    context: [bool; 7],
    last: Option<std::time::Instant>,
    pending: bool,
}

impl PushState {
    const STREAM_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

    fn due(streaming: bool, urgent: bool, elapsed: Option<std::time::Duration>) -> bool {
        urgent || !streaming || elapsed.is_none_or(|elapsed| elapsed >= Self::STREAM_INTERVAL)
    }
}

#[derive(Component)]
struct Binding;

#[derive(SystemParam)]
struct ViewBindings<'w, 's> {
    entries:
        Query<'w, 's, (Entity, &'static ChildOf, &'static EntityTarget<Session>), With<Binding>>,
    commands: Commands<'w, 's>,
}

impl ViewBindings<'_, '_> {
    fn ensure(&mut self, webview: Entity, session: Entity) {
        if self
            .entries
            .iter()
            .any(|(_, parent, target)| parent.parent() == webview && target.entity() == session)
        {
            return;
        }
        for (binding, parent, _) in &self.entries {
            if parent.parent() == webview {
                self.commands.entity(binding).despawn();
            }
        }
        self.commands.spawn((
            EntityTarget::<Session>::new(session),
            ChildOf(webview),
            Binding,
        ));
    }
}

fn attach(sessions: Query<Entity, (With<Session>, Without<PushState>)>, mut commands: Commands) {
    for session in &sessions {
        commands.entity(session).insert(PushState::default());
    }
}

fn unbind(
    event: On<Remove, SessionView>,
    bindings: Query<(Entity, &ChildOf), With<Binding>>,
    mut commands: Commands,
) {
    for (binding, parent) in &bindings {
        if parent.parent() == event.entity {
            commands.entity(binding).despawn();
        }
    }
}

fn retire_handoff_events(
    event: On<Remove, ImportedConversation>,
    handoffs: Query<(Entity, &ChildOf), With<HandoffEvent>>,
    mut commands: Commands,
) {
    for (entity, parent) in &handoffs {
        if parent.parent() == event.entity {
            commands.entity(entity).despawn();
        }
    }
}

fn open_file(
    event: On<UiInput<crate::event::ChatToolFileOpen>>,
    views: Query<&PageMetadata, With<SessionView>>,
    mut commands: Commands,
) {
    let request = &event.payload;
    let Ok(metadata) = views.get(event.webview) else {
        return;
    };
    if !matches!(Route::parse(&metadata.url), Some(Route::Session(_))) {
        return;
    }
    let Ok(url) = url::Url::parse(&request.url) else {
        return;
    };
    if url.to_file_path().is_err() {
        return;
    }
    let target = match request.open {
        Some(target) if target != OpenTarget::InPlace => target,
        _ if request.width.is_finite() && request.width >= 1280.0 => OpenTarget::InPane {
            direction: PaneDirection::Right,
            target: PaneTarget::Existing,
            mode: PaneOpenMode::NewStack,
        },
        _ => OpenTarget::InNewStack,
    };
    commands.trigger(UiInput {
        webview: event.webview,
        payload: vmux_api::command_bar::OpenRequest {
            value: request.url.clone(),
            open: Some(target),
        },
    });
}

struct ChatProjection {
    snapshot: ChatSnapshot,
    transcript: TranscriptTail,
}

type ChangedSession = (
    Entity,
    Ref<'static, AgentId>,
    Ref<'static, Name>,
    Ref<'static, RunState>,
    Option<Ref<'static, AgentTurnMeta>>,
    Option<Ref<'static, Profile>>,
    Ref<'static, PromptQueue>,
    Option<Ref<'static, ImportedConversation>>,
    Option<Ref<'static, AgentConversationTitle>>,
    Has<vmux_session::TranscriptLoading>,
    Option<Ref<'static, Children>>,
);

type ChangedConversationEvents<'w, 's> = Query<
    'w,
    's,
    &'static ChildOf,
    (
        With<ConversationEvent>,
        Or<(
            Added<ConversationEvent>,
            Changed<EventIdentity>,
            Changed<MessageContent>,
            Changed<CreatedAt>,
            Changed<vmux_ecs::Order>,
        )>,
    ),
>;

type ProjectionView = (
    &'static mut ChatTranscriptProjection,
    &'static mut ChatSnapshotProjection,
    &'static ChatAttachmentProjection,
);

type ChangedHandoffEvents<'w, 's> = Query<
    'w,
    's,
    &'static ChildOf,
    (
        With<HandoffEvent>,
        Or<(
            Added<HandoffEvent>,
            Changed<MessageContent>,
            Changed<vmux_ecs::Order>,
        )>,
    ),
>;

#[derive(SystemParam)]
struct PushWorld<'w, 's> {
    sessions: Query<'w, 's, ChangedSession>,
    authentication: Query<'w, 's, Ref<'static, crate::Authentication>>,
    preparation: Query<'w, 's, Ref<'static, crate::Preparation>>,
    stacks: Query<
        'w,
        's,
        (
            &'static EntityTarget<Session>,
            &'static Children,
            Option<Ref<'static, PageMetadata>>,
        ),
    >,
    chat_views: Query<'w, 's, ProjectionView, With<SessionView>>,
    bindings:
        Query<'w, 's, (Entity, &'static ChildOf, &'static EntityTarget<Session>), With<Binding>>,
    session_ids: Query<'w, 's, &'static SessionId, With<Session>>,
    viewports: Query<'w, 's, &'static ChatViewport, With<Session>>,
    choices: Query<'w, 's, Ref<'static, PendingAgentChoice>>,
    synced: Query<'w, 's, (), (With<SessionView>, With<ChatSynced>)>,
    user_profiles: Query<'w, 's, Ref<'static, Profile>, With<User>>,
    transcripts: Transcripts<'w, 's>,
    handoffs: HandoffMessages<'w, 's>,
    changed_events: ChangedConversationEvents<'w, 's>,
    changed_handoffs: ChangedHandoffEvents<'w, 's>,
    loading_started: Query<'w, 's, Entity, Added<vmux_session::TranscriptLoading>>,
    loading_finished: RemovedComponents<'w, 's, vmux_session::TranscriptLoading>,
    queued: QueuedPromptReads<'w, 's>,
    setup: Query<'w, 's, (), With<vmux_session::SetupPending>>,
    setup_started: Query<'w, 's, Entity, Added<vmux_session::SetupPending>>,
    setup_finished: RemovedComponents<'w, 's, vmux_session::SetupPending>,
}

impl PushWorld<'_, '_> {
    fn find_view(&self, session: Entity) -> Option<(Entity, Option<Ref<'_, PageMetadata>>)> {
        self.stacks.iter().find_map(|(target, children, meta)| {
            if target.entity() != session {
                return None;
            }
            children
                .iter()
                .find(|&entity| self.chat_views.contains(entity))
                .map(|webview| (webview, meta))
        })
    }

    fn has_binding(&self, webview: Entity, session: Entity) -> bool {
        self.bindings
            .iter()
            .any(|(_, parent, target)| parent.parent() == webview && target.entity() == session)
    }

    fn rebind(&self, webview: Entity, session: Entity, commands: &mut Commands) {
        for (binding, parent, _) in &self.bindings {
            if parent.parent() == webview {
                commands.entity(binding).despawn();
            }
        }
        commands.spawn((
            EntityTarget::<Session>::new(session),
            ChildOf(webview),
            Binding,
        ));
    }

    fn changed_sessions(
        &self,
    ) -> (
        std::collections::HashSet<Entity>,
        std::collections::HashSet<Entity>,
    ) {
        let mut transcripts = std::collections::HashSet::new();
        for parent in &self.changed_events {
            transcripts.insert(parent.parent());
        }
        for parent in &self.changed_handoffs {
            transcripts.insert(parent.parent());
        }
        let mut loading = std::collections::HashSet::new();
        for session in &self.loading_started {
            loading.insert(session);
        }
        for session in &self.setup_started {
            loading.insert(session);
        }
        (transcripts, loading)
    }

    #[allow(clippy::too_many_arguments)]
    fn push_queue_ready(
        &self,
        webview: Entity,
        initialized: bool,
        context_changed: bool,
        conversation_changed: bool,
        push: &PushState,
        context: [bool; 7],
        meta: Option<&Ref<'_, PageMetadata>>,
        choice: Option<&Ref<'_, PendingAgentChoice>>,
    ) -> bool {
        !context_changed
            && !conversation_changed
            && !push.pending
            && initialized
            && self.synced.contains(webview)
            && context == push.context
            && !meta.is_some_and(|meta| meta.is_changed())
            && !choice.is_some_and(|choice| choice.is_changed())
    }

    #[allow(clippy::too_many_arguments)]
    fn project_session(
        &self,
        session: Entity,
        agent_id: &AgentId,
        name: &Name,
        state: &RunState,
        turn_meta: Option<&AgentTurnMeta>,
        profile: Option<&Profile>,
        user_profile: Option<&Profile>,
        meta: Option<&PageMetadata>,
        queue: &PromptQueue,
        imported: Option<&ImportedConversation>,
        title: Option<&AgentConversationTitle>,
        transcript_loading: bool,
        choice: Option<&PendingAgentChoice>,
        initialized: bool,
    ) -> ChatProjection {
        let viewport = (!initialized)
            .then(|| self.viewports.get(session).ok())
            .flatten();
        let imported_messages = self.handoffs.messages(session);
        let mut projection = self.transcripts.read(session, |conversation| {
            ChatProjection::new(
                agent_id,
                &conversation.messages,
                &conversation.created_at,
                state,
                turn_meta,
                profile,
                user_profile,
                meta,
                queue,
                self.queued.snapshot(session),
                imported,
                &imported_messages,
                title,
                name.as_str(),
                transcript_loading,
                choice,
                viewport,
                &conversation.event_ids,
            )
        });
        if let Ok(preparation) = self.preparation.get(session) {
            projection.prepare(&preparation);
        }
        if let Ok(authentication) = self.authentication.get(session) {
            projection.authenticate(&authentication);
        }
        projection.snapshot.setup_pending = self.setup.contains(session);
        projection
    }

    #[allow(clippy::type_complexity)]
    fn collect_updates(
        &self,
        pushes: &mut Query<&mut PushState>,
        changed_transcripts: &std::collections::HashSet<Entity>,
        loading_changes: &std::collections::HashSet<Entity>,
    ) -> Vec<(
        Entity,
        Entity,
        Option<bool>,
        Option<ChatProjection>,
        bool,
        [bool; 7],
        std::time::Instant,
    )> {
        let user_profile = self.user_profiles.single().ok();
        let user_moved = user_profile
            .as_ref()
            .is_some_and(|profile| profile.is_changed());
        let mut updates = Vec::new();
        for item in &self.sessions {
            if let Some(update) = self.prepare_session_update(
                item,
                user_profile.as_deref(),
                user_moved,
                pushes,
                changed_transcripts,
                loading_changes,
            ) {
                updates.push(update);
            }
        }
        updates
    }

    fn session_context(
        turn_meta: Option<&Ref<'_, AgentTurnMeta>>,
        profile: Option<&Ref<'_, Profile>>,
        imported: Option<&Ref<'_, ImportedConversation>>,
        title: Option<&Ref<'_, AgentConversationTitle>>,
        user_profile: Option<&Profile>,
        meta: Option<&Ref<'_, PageMetadata>>,
        choice: Option<&Ref<'_, PendingAgentChoice>>,
    ) -> [bool; 7] {
        [
            turn_meta.is_some(),
            profile.is_some(),
            imported.is_some(),
            title.is_some(),
            user_profile.is_some(),
            meta.is_some(),
            choice.is_some(),
        ]
    }

    #[allow(clippy::too_many_arguments)]
    fn queue_only_update(
        &self,
        session: Entity,
        webview: Entity,
        queue: &Ref<'_, PromptQueue>,
        initialized: bool,
        context_changed: bool,
        conversation_changed: bool,
        push: &PushState,
        context: [bool; 7],
        meta: Option<&Ref<'_, PageMetadata>>,
        choice: Option<&Ref<'_, PendingAgentChoice>>,
        now: std::time::Instant,
    ) -> Option<(
        Entity,
        Entity,
        Option<bool>,
        Option<ChatProjection>,
        bool,
        [bool; 7],
        std::time::Instant,
    )> {
        if queue.is_changed()
            && self.push_queue_ready(
                webview,
                initialized,
                context_changed,
                conversation_changed,
                push,
                context,
                meta,
                choice,
            )
        {
            Some((
                session,
                webview,
                Some(queue.paused),
                None,
                initialized,
                context,
                now,
            ))
        } else {
            None
        }
    }

    #[allow(clippy::type_complexity)]
    fn prepare_session_update(
        &self,
        item: (
            Entity,
            Ref<'_, AgentId>,
            Ref<'_, Name>,
            Ref<'_, RunState>,
            Option<Ref<'_, AgentTurnMeta>>,
            Option<Ref<'_, Profile>>,
            Ref<'_, PromptQueue>,
            Option<Ref<'_, ImportedConversation>>,
            Option<Ref<'_, AgentConversationTitle>>,
            bool,
            Option<Ref<'_, Children>>,
        ),
        user_profile: Option<&Profile>,
        user_moved: bool,
        pushes: &mut Query<&mut PushState>,
        changed_transcripts: &std::collections::HashSet<Entity>,
        loading_changes: &std::collections::HashSet<Entity>,
    ) -> Option<(
        Entity,
        Entity,
        Option<bool>,
        Option<ChatProjection>,
        bool,
        [bool; 7],
        std::time::Instant,
    )> {
        let (
            session,
            agent_id,
            name,
            state,
            turn_meta,
            profile,
            queue,
            imported,
            title,
            loading,
            children,
        ) = item;
        let Ok(mut push) = pushes.get_mut(session) else {
            return None;
        };
        let conversation_changed = children
            .as_ref()
            .is_some_and(|children| children.is_changed())
            || changed_transcripts.contains(&session);
        let context_changed = self.context_changed(
            session,
            user_moved,
            loading_changes.contains(&session),
            &agent_id,
            &name,
            &state,
            turn_meta.as_ref(),
            profile.as_ref(),
            imported.as_ref(),
            title.as_ref(),
        );
        let moved = context_changed || queue.is_changed();
        if !moved && !conversation_changed && !push.pending {
            return None;
        }
        let Some((webview, meta)) = self.find_view(session) else {
            push.pending = true;
            return None;
        };
        let now = std::time::Instant::now();
        let choice = self.choices.get(webview).ok();
        let context = Self::session_context(
            turn_meta.as_ref(),
            profile.as_ref(),
            imported.as_ref(),
            title.as_ref(),
            user_profile,
            meta.as_ref(),
            choice.as_ref(),
        );
        let initialized = self.has_binding(webview, session);
        if let Some(update) = self.queue_only_update(
            session,
            webview,
            &queue,
            initialized,
            context_changed,
            conversation_changed,
            &push,
            context,
            meta.as_ref(),
            choice.as_ref(),
            now,
        ) {
            return Some(update);
        }
        let elapsed = push.last.map(|last| now.saturating_duration_since(last));
        if !PushState::due(matches!(*state, RunState::Streaming), moved, elapsed) {
            push.pending = true;
            return None;
        }
        push.pending = false;
        let projection = self.project_session(
            session,
            &agent_id,
            &name,
            &state,
            turn_meta.as_deref(),
            profile.as_deref(),
            user_profile,
            meta.as_deref(),
            &queue,
            imported.as_deref(),
            title.as_deref(),
            loading,
            choice.as_deref(),
            initialized,
        );
        Some((
            session,
            webview,
            None,
            Some(projection),
            initialized,
            context,
            now,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn context_changed(
        &self,
        session: Entity,
        user_moved: bool,
        loading_changed: bool,
        agent_id: &Ref<'_, AgentId>,
        name: &Ref<'_, Name>,
        state: &Ref<'_, RunState>,
        turn_meta: Option<&Ref<'_, AgentTurnMeta>>,
        profile: Option<&Ref<'_, Profile>>,
        imported: Option<&Ref<'_, ImportedConversation>>,
        title: Option<&Ref<'_, AgentConversationTitle>>,
    ) -> bool {
        user_moved
            || loading_changed
            || agent_id.is_changed()
            || name.is_changed()
            || state.is_changed()
            || self
                .preparation
                .get(session)
                .is_ok_and(|preparation| preparation.is_changed())
            || self
                .authentication
                .get(session)
                .is_ok_and(|authentication| authentication.is_changed())
            || turn_meta.is_some_and(|meta| meta.is_changed())
            || profile.is_some_and(|profile| profile.is_changed())
            || imported.is_some_and(|imported| imported.is_changed())
            || title.is_some_and(|title| title.is_changed())
    }

    fn publish_updates(
        &mut self,
        updates: Vec<(
            Entity,
            Entity,
            Option<bool>,
            Option<ChatProjection>,
            bool,
            [bool; 7],
            std::time::Instant,
        )>,
        pushes: &mut Query<&mut PushState>,
        commands: &mut Commands,
    ) {
        for (session, webview, queue_paused, projection, initialized, context, now) in updates {
            let Ok(mut push) = pushes.get_mut(session) else {
                continue;
            };
            if let Some(paused) = queue_paused {
                if self.publish_queue_only(webview, session, paused, commands) {
                    push.last = Some(now);
                }
                continue;
            }
            if let Some(projection) = projection {
                if self.publish_full(webview, session, initialized, projection, commands) {
                    push.context = context;
                    push.last = Some(now);
                } else {
                    push.pending = true;
                }
            }
        }
    }

    fn publish_queue_only(
        &mut self,
        webview: Entity,
        session: Entity,
        paused: bool,
        commands: &mut Commands,
    ) -> bool {
        let Ok((transcript, mut snapshot, attachments)) = self.chat_views.get_mut(webview) else {
            return false;
        };
        snapshot.0.queued = self.queued.snapshot(session);
        snapshot.0.paused = paused;
        attachments.hydrate_snapshot(&mut snapshot.0);
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(webview, &snapshot.0),
        );
        let paths = attachments.hydration_paths(&transcript.state, &snapshot.0);
        if !paths.is_empty() {
            commands.trigger(ChatAttachmentHydrationRequest { webview, paths });
        }
        true
    }

    fn publish_full(
        &mut self,
        webview: Entity,
        session: Entity,
        initialized: bool,
        mut projection: ChatProjection,
        commands: &mut Commands,
    ) -> bool {
        let Ok((mut transcript, mut snapshot, attachments)) = self.chat_views.get_mut(webview)
        else {
            return false;
        };
        let mut transcript_changed = transcript.merge_tail(projection.transcript);
        transcript_changed |= attachments.hydrate_transcript(&mut transcript.state);
        attachments.hydrate_snapshot(&mut projection.snapshot);
        snapshot.0 = projection.snapshot;
        if !initialized && let Ok(session_id) = self.session_ids.get(session) {
            commands.trigger(
                vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
                    webview,
                    &crate::state::ChatUiStatePatch {
                        viewport: Some(ChatViewportState {
                            session: session_id.0.clone(),
                            position: self.viewports.get(session).ok().copied(),
                        }),
                        ..Default::default()
                    },
                ),
            );
        }
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(webview, &snapshot.0),
        );
        if transcript_changed {
            commands.trigger(
                vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
                    webview,
                    &transcript.state,
                ),
            );
        }
        let paths = attachments.hydration_paths(&transcript.state, &snapshot.0);
        if !paths.is_empty() {
            commands.trigger(ChatAttachmentHydrationRequest { webview, paths });
        }
        if !initialized {
            self.rebind(webview, session, commands);
        }
        true
    }
}

type ReadyViewFilter = (
    With<SessionView>,
    With<vmux_ecs::page::PageReady>,
    Without<ChatSynced>,
);

#[derive(SystemParam)]
struct ReadyWorld<'w, 's> {
    setup: Query<'w, 's, (), With<vmux_session::SetupPending>>,
    targets: super::session::SessionViews<'w, 's>,
    child_of: Query<'w, 's, &'static ChildOf>,
    metadata: Query<'w, 's, &'static PageMetadata>,
    sessions: Query<'w, 's, SessionProjection>,
    authentication: Query<'w, 's, &'static crate::Authentication>,
    preparation: Query<'w, 's, &'static crate::Preparation>,
    transcripts: Transcripts<'w, 's>,
    handoffs: HandoffMessages<'w, 's>,
    choices: Query<'w, 's, &'static PendingAgentChoice>,
    user_profiles: Query<'w, 's, &'static Profile, With<User>>,
    viewports: Query<'w, 's, &'static ChatViewport, With<Session>>,
    session_ids: Query<'w, 's, &'static SessionId, With<Session>>,
    queued: QueuedPromptReads<'w, 's>,
}

impl ReadyWorld<'_, '_> {
    fn project(
        &self,
        webview: Entity,
        session_entity: Entity,
    ) -> Option<(ChatProjection, Option<ChatViewport>)> {
        let user_profile = self.user_profiles.single().ok();
        let stack = self
            .child_of
            .get(webview)
            .map(ChildOf::parent)
            .unwrap_or(Entity::PLACEHOLDER);
        let (agent_id, name, state, turn_meta, profile, queue, imported, title, transcript_loading) =
            self.sessions.get(session_entity).ok()?;
        let meta = self.metadata.get(stack).ok();
        let choice = self.choices.get(webview).ok();
        let viewport = self.viewports.get(session_entity).ok().copied();
        let imported_messages = self.handoffs.messages(session_entity);
        let mut projection = self.transcripts.read(session_entity, |conversation| {
            ChatProjection::new(
                agent_id,
                &conversation.messages,
                &conversation.created_at,
                state,
                turn_meta,
                profile,
                user_profile,
                meta,
                queue,
                self.queued.snapshot(session_entity),
                imported,
                &imported_messages,
                title,
                name.as_str(),
                transcript_loading,
                choice,
                viewport.as_ref(),
                &conversation.event_ids,
            )
        });
        if let Ok(preparation) = self.preparation.get(session_entity) {
            projection.prepare(preparation);
        }
        if let Ok(authentication) = self.authentication.get(session_entity) {
            projection.authenticate(authentication);
        }
        projection.snapshot.setup_pending = self.setup.contains(session_entity);
        Some((projection, viewport))
    }
}

type SessionProjection = (
    &'static AgentId,
    &'static Name,
    &'static RunState,
    Option<&'static AgentTurnMeta>,
    Option<&'static Profile>,
    &'static PromptQueue,
    Option<&'static ImportedConversation>,
    Option<&'static AgentConversationTitle>,
    Has<vmux_session::TranscriptLoading>,
);

fn track_turn_duration(
    time: Res<Time>,
    mut sessions: Query<(&RunState, &mut AgentTurnMeta), Changed<RunState>>,
) {
    for (state, mut meta) in &mut sessions {
        match state {
            RunState::Streaming => {
                if meta.turn_start.is_none() {
                    meta.turn_start = Some(time.elapsed());
                }
            }
            RunState::Idle | RunState::Errored(_) => {
                if let Some(start) = meta.turn_start.take() {
                    meta.durations
                        .push(time.elapsed().saturating_sub(start).as_secs() as u32);
                }
            }
            RunState::AwaitingApproval { .. } => {}
        }
    }
}

fn push_to_page(mut world: PushWorld, mut pushes: Query<&mut PushState>, mut commands: Commands) {
    let (changed_transcripts, mut loading_changes) = world.changed_sessions();
    for session in world.loading_finished.read() {
        loading_changes.insert(session);
    }
    for session in world.setup_finished.read() {
        loading_changes.insert(session);
    }
    let updates = world.collect_updates(&mut pushes, &changed_transcripts, &loading_changes);
    world.publish_updates(updates, &mut pushes, &mut commands);
}

impl QueuedPromptReads<'_, '_> {
    fn snapshot(&self, session: Entity) -> Vec<QueuedPromptSnapshot> {
        let mut queued = Vec::new();
        for (id, text, media) in self.visible(session) {
            let mut attachments = Vec::with_capacity(media.len());
            for attachment in media {
                attachments.push(crate::event::ChatAttachment {
                    path: attachment.path.clone(),
                    name: attachment.name.clone(),
                    mime_type: attachment.mime_type.clone(),
                    size: attachment.size,
                    ..Default::default()
                });
            }
            queued.push(QueuedPromptSnapshot {
                id,
                text: text.to_string(),
                attachments,
            });
        }
        queued
    }
}

impl ChatProjection {
    fn viewport(
        session: Option<&SessionId>,
        position: Option<ChatViewport>,
    ) -> crate::state::ChatUiStatePatch {
        crate::state::ChatUiStatePatch {
            viewport: Some(ChatViewportState {
                session: session.map(|id| id.0.clone()).unwrap_or_default(),
                position,
            }),
            ..Default::default()
        }
    }

    fn prepare(&mut self, preparation: &crate::Preparation) {
        if !preparation.active() {
            return;
        }
        self.snapshot.status = if preparation.phase == crate::PreparationPhase::Installing {
            "installing"
        } else {
            "preparing"
        }
        .into();
        self.snapshot.error = preparation.detail.clone();
        self.snapshot.install_percent = preparation.percent;
    }

    fn authenticate(&mut self, authentication: &crate::Authentication) {
        if !authentication.active {
            return;
        }
        self.snapshot.status = "authentication".into();
        self.snapshot.authentication_methods = authentication.methods.clone();
        self.snapshot.authentication_pending = authentication.pending;
    }

    fn approval(state: &RunState) -> Option<PendingApproval> {
        match state {
            RunState::AwaitingApproval {
                call_id,
                name,
                args,
            } => {
                let args = vmux_api::json::JsonValue::from(args.clone());
                Some(PendingApproval::new(call_id.clone(), name.clone(), &args))
            }
            _ => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        agent_id: &AgentId,
        messages: &[vmux_api::session::Message],
        message_times: &[u64],
        state: &RunState,
        turn_meta: Option<&AgentTurnMeta>,
        profile: Option<&Profile>,
        user_profile: Option<&Profile>,
        meta: Option<&PageMetadata>,
        queue: &PromptQueue,
        queued: Vec<QueuedPromptSnapshot>,
        imported: Option<&ImportedConversation>,
        imported_messages: &[vmux_api::session::Message],
        conversation_title: Option<&AgentConversationTitle>,
        fallback_title: &str,
        transcript_loading: bool,
        choice: Option<&PendingAgentChoice>,
        viewport: Option<&ChatViewport>,
        event_ids: &[vmux_api::session::EventId],
    ) -> Self {
        let durations: &[u32] = turn_meta.map(|m| m.durations.as_slice()).unwrap_or(&[]);
        let running = matches!(state, RunState::Streaming);
        let messages = ChatMessages::new(
            imported_messages,
            messages,
            message_times,
            durations,
            running,
        )
        .with_event_ids(event_ids);
        let limit = viewport.map_or(CHAT_INITIAL_ITEM_LIMIT as usize, |viewport| {
            viewport.item_limit(messages.item_count())
        });
        let page = messages.tail_with_prompt(limit);
        let error = match state {
            RunState::Errored(message) => message.clone(),
            _ => String::new(),
        };
        let approval = Self::approval(state);
        let (agent_name, accent_color) = profile
            .map(|p| (p.name.clone(), p.avatar.color.clone()))
            .unwrap_or_default();
        let (user_name, user_initials, user_color) = user_profile
            .map(|profile| {
                (
                    profile.name.clone(),
                    profile.avatar.initials.clone(),
                    profile.avatar.color.clone(),
                )
            })
            .unwrap_or_else(|| {
                let profile = Profile::user();
                (profile.name, profile.avatar.initials, profile.avatar.color)
            });
        let agent_icon = meta
            .map(|m| m.icon.favicon_url().to_string())
            .unwrap_or_default();
        let transcript_empty = page.items.is_empty();
        let handoff_message_count = if imported.is_some() {
            u32::try_from(ChatMessages::new(imported_messages, &[], &[], &[], false).item_count())
                .unwrap_or(u32::MAX)
        } else {
            0
        };
        let mut snapshot = ChatSnapshot {
            status: state.status().to_string(),
            error,
            approval,
            agent_id: agent_id.0.clone(),
            agent_name,
            conversation_title: conversation_title
                .map(|title| title.0.clone())
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| fallback_title.to_string()),
            transcript_loading,
            agent_icon,
            accent_color,
            user_name,
            user_initials,
            user_color,
            handoff_source: imported
                .map(|imported| imported.source_agent.clone())
                .unwrap_or_default(),
            handoff_truncated: imported.is_some_and(|imported| imported.truncated),
            handoff_message_count,
            choice_question: choice
                .map(|choice| choice.question.clone())
                .unwrap_or_default(),
            choice_options: choice
                .map(|choice| choice.options.clone())
                .unwrap_or_default(),
            queued,
            paused: queue.paused,
            ..Default::default()
        };
        ChatPresentation::apply(&mut snapshot, transcript_empty);
        Self {
            snapshot,
            transcript: TranscriptTail {
                items: page.items,
                start: u32::try_from(page.start).unwrap_or(u32::MAX),
                total: u32::try_from(page.total).unwrap_or(u32::MAX),
            },
        }
    }
}

fn sync_ready_views(
    mut pending: Query<
        (
            Entity,
            &mut ChatTranscriptProjection,
            &mut ChatSnapshotProjection,
            &ChatAttachmentProjection,
        ),
        ReadyViewFilter,
    >,
    world: ReadyWorld,
    mut bindings: ViewBindings,
    mut commands: Commands,
) {
    for (webview, mut transcript, mut snapshot, attachments) in &mut pending {
        let Some(session_entity) = world.targets.session(webview) else {
            continue;
        };
        let Some((mut projection, viewport)) = world.project(webview, session_entity) else {
            continue;
        };
        transcript.merge_tail(projection.transcript);
        attachments.hydrate_transcript(&mut transcript.state);
        attachments.hydrate_snapshot(&mut projection.snapshot);
        snapshot.0 = projection.snapshot;
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
                webview,
                &ChatProjection::viewport(world.session_ids.get(session_entity).ok(), viewport),
            ),
        );
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(webview, &snapshot.0),
        );
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
                webview,
                &transcript.state,
            ),
        );
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
                webview,
                &attachments.state(),
            ),
        );
        let paths = attachments.hydration_paths(&transcript.state, &snapshot.0);
        if !paths.is_empty() {
            commands.trigger(ChatAttachmentHydrationRequest { webview, paths });
        }
        commands.entity(webview).insert(ChatSynced);
        bindings.ensure(webview, session_entity);
    }
}

fn remember_viewport(
    event: On<UiInput<ChatViewportChanged>>,
    views: Query<&PageMetadata, With<SessionView>>,
    sessions: Query<(Entity, &SessionId, Option<&ChatViewport>), With<Session>>,
    mut commands: Commands,
) {
    let Ok(metadata) = views.get(event.webview) else {
        return;
    };
    let Some(Route::Session(id)) = Route::parse(&metadata.url) else {
        return;
    };
    if id.0 != event.payload.session {
        return;
    }
    let Some((entity, _, current)) = sessions.iter().find(|(_, candidate, _)| **candidate == id)
    else {
        return;
    };
    let mut position = event.payload.viewport;
    position.top = position.top.max(0);
    if current == Some(&position) {
        return;
    }
    commands.entity(entity).insert(position);
}

fn reset_synced(
    trigger: On<UiInput<vmux_ecs::page::PageReady>>,
    chat_views: Query<(), With<SessionView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if chat_views.get(webview).is_ok() {
        commands.entity(webview).remove::<ChatSynced>();
    }
}

fn reset_navigated(
    views: Query<Entity, (With<SessionView>, With<ChatSynced>, Changed<PageMetadata>)>,
    mut commands: Commands,
) {
    for view in &views {
        commands.entity(view).remove::<ChatSynced>();
    }
}

fn request_more(
    trigger: On<UiInput<ChatHistoryMoreRequest>>,
    targets: super::session::SessionViews,
    mut views: Query<&mut ChatTranscriptProjection, With<SessionView>>,
    sources: Query<(&SessionId, &vmux_session::ConversationHistory)>,
    mut history_requests: MessageWriter<vmux_session::ConversationHistoryMoreRequest>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Some(session) = targets.session(webview) else {
        return;
    };
    let Ok(mut transcript) = views.get_mut(webview) else {
        return;
    };
    if transcript.state.loaded_start == 0 {
        if let Ok((id, history)) = sources.get(session)
            && history.has_older
            && !history.loading
            && !transcript.state.loading
        {
            history_requests.write(vmux_session::ConversationHistoryMoreRequest {
                session: id.clone(),
                generation: history.generation,
            });
        }
        return;
    }
    let Some(query) = transcript.start_history_query(webview, session) else {
        return;
    };
    commands.spawn(query);
    commands.trigger(
        vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(webview, &transcript.state),
    );
}

fn sync_history(
    targets: super::session::SessionViews,
    sources: Query<(&SessionId, &vmux_session::ConversationHistory)>,
    mut views: Query<
        (Entity, &mut ChatTranscriptProjection),
        (With<SessionView>, With<vmux_ecs::page::PageReady>),
    >,
    mut commands: Commands,
) {
    for (view, mut transcript) in &mut views {
        let Some(session) = targets.session(view) else {
            continue;
        };
        let Ok((id, history)) = sources.get(session) else {
            continue;
        };
        let loading = if transcript.state.loaded_start == 0 {
            history.loading
        } else {
            transcript.state.loading
        };
        let has_older = transcript.state.loaded_start > 0 || history.has_older;
        if transcript.state.has_older == has_older && transcript.state.loading == loading {
            continue;
        }
        transcript.state.has_older = has_older;
        transcript.state.loading = loading;
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
                view,
                &crate::state::ChatUiStatePatch {
                    history_status: Some(crate::event::ChatHistoryStatus {
                        session: id.0.clone(),
                        has_older,
                        loading,
                    }),
                    ..Default::default()
                },
            ),
        );
    }
}

fn resolve_queries(
    queries: Query<(Entity, &ChatHistoryQuery)>,
    sessions: Query<(&RunState, Option<&AgentTurnMeta>)>,
    transcripts: Transcripts,
    handoffs: HandoffMessages,
    mut commands: Commands,
) {
    for (entity, query) in &queries {
        let page = sessions.get(query.session).ok().map(|(state, turn_meta)| {
            let imported_messages = handoffs.messages(query.session);
            let durations = turn_meta
                .map(|meta| meta.durations.as_slice())
                .unwrap_or(&[]);
            let page = transcripts.read(query.session, |transcript| {
                ChatMessages::new(
                    &imported_messages,
                    &transcript.messages,
                    &transcript.created_at,
                    durations,
                    matches!(state, RunState::Streaming),
                )
                .with_event_ids(&transcript.event_ids)
                .before(query.before as usize, query.limit as usize)
            });
            TranscriptPage {
                items: page.items,
                start: u32::try_from(page.start).unwrap_or(u32::MAX),
                end: u32::try_from(page.end).unwrap_or(u32::MAX),
                total: u32::try_from(page.total).unwrap_or(u32::MAX),
            }
        });
        commands
            .entity(entity)
            .remove::<ChatHistoryQuery>()
            .insert(ChatHistoryResult {
                webview: query.webview,
                generation: query.generation,
                request_id: query.request_id,
                page,
            });
    }
}

fn apply_results(
    results: Query<(Entity, &ChatHistoryResult)>,
    mut views: Query<
        (
            &mut ChatTranscriptProjection,
            &ChatSnapshotProjection,
            &ChatAttachmentProjection,
        ),
        With<SessionView>,
    >,
    mut commands: Commands,
) {
    for (entity, result) in &results {
        let Ok((mut transcript, snapshot, attachments)) = views.get_mut(result.webview) else {
            commands.entity(entity).despawn();
            continue;
        };
        let mut changed = transcript.finish_history_query(result);
        changed |= attachments.hydrate_transcript(&mut transcript.state);
        commands.entity(entity).despawn();
        if changed {
            commands.trigger(
                vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
                    result.webview,
                    &transcript.state,
                ),
            );
        }
        let paths = attachments.hydration_paths(&transcript.state, &snapshot.0);
        if !paths.is_empty() {
            commands.trigger(ChatAttachmentHydrationRequest {
                webview: result.webview,
                paths,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_ecs::page::PageReady;

    #[derive(Resource, Default)]
    struct Published(crate::state::ChatUiState);

    impl Published {
        fn record(
            trigger: On<vmux_ecs::UiStateWrite<crate::state::ChatUiState>>,
            mut published: ResMut<Self>,
        ) {
            vmux_api::UiStateProjection::apply(&mut published.0, trigger.event().update().clone());
        }
    }

    #[test]
    fn queue_only_updates_publish_without_mutating_the_current_transcript() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_session::QueuePlugin, TranscriptPlugin))
            .init_resource::<Published>()
            .add_observer(Published::record);
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId("queue".into()),
                AgentId("codex".into()),
                Name::new("Queue test"),
                RunState::Streaming,
                PromptQueue::default(),
                ImportedConversation {
                    source_agent: "Codex".into(),
                    source_sid: "retained".into(),
                    truncated: false,
                    first_prompt: None,
                },
            ))
            .id();
        for event in ImportedConversation::events(
            session,
            vec![vmux_api::session::Message::user("retained history")],
        ) {
            app.world_mut().spawn(event);
        }
        app.world_mut()
            .run_system_once(move |mut queues: vmux_session::QueuedPrompts| {
                queues.enqueue(session, "seed".into(), Vec::new(), false);
            })
            .unwrap();
        app.world_mut()
            .run_system_once(move |mut queues: vmux_session::QueuedPrompts| queues.clear(session))
            .unwrap();
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
            .id();
        let view = app
            .world_mut()
            .spawn((SessionView, PageReady, ChildOf(stack)))
            .id();
        app.update();
        app.update();
        app.update();
        assert!(app.world().get::<ChatSynced>(view).is_some());
        let revision = app
            .world()
            .entity(view)
            .get_ref::<ChatTranscriptProjection>()
            .unwrap()
            .last_changed();
        let previous = app.world().resource::<Published>().0.transcript.clone();
        app.world_mut()
            .run_system_once(move |mut queues: vmux_session::QueuedPrompts| {
                queues.enqueue(session, "queued next".into(), Vec::new(), false);
            })
            .unwrap();
        app.update();
        let state = &app.world().resource::<Published>().0;
        assert!(
            matches!(state.snapshot.queued.as_slice(), [prompt] if prompt.text == "queued next")
        );
        assert_eq!(state.transcript, previous);
        assert_eq!(
            app.world()
                .entity(view)
                .get_ref::<ChatTranscriptProjection>()
                .unwrap()
                .last_changed(),
            revision
        );
        app.world_mut().entity_mut(view).insert(PendingAgentChoice {
            session_entity: session,
            question: "Choose an agent".into(),
            options: vec!["one".into()],
        });
        app.world_mut().entity_mut(stack).insert(PageMetadata {
            icon: vmux_api::PageIcon::favicon("https://example.invalid/agent.svg"),
            ..Default::default()
        });
        app.world_mut()
            .run_system_once(move |mut queues: vmux_session::QueuedPrompts| {
                queues.enqueue(session, "with context".into(), Vec::new(), false);
            })
            .unwrap();
        app.update();
        let state = &app.world().resource::<Published>().0.snapshot;
        assert!(!state.choice_question.is_empty());
        assert!(!state.agent_icon.is_empty());
        app.world_mut()
            .entity_mut(view)
            .remove::<PendingAgentChoice>();
        app.world_mut().entity_mut(stack).remove::<PageMetadata>();
        app.world_mut()
            .run_system_once(move |mut queues: vmux_session::QueuedPrompts| {
                queues.enqueue(session, "without context".into(), Vec::new(), false);
            })
            .unwrap();
        app.update();
        let state = &app.world().resource::<Published>().0.snapshot;
        assert!(state.choice_question.is_empty());
        assert!(state.choice_options.is_empty());
        assert!(state.agent_icon.is_empty());
        app.world_mut().entity_mut(session).insert((
            Profile::registry("Canonical agent", "codex"),
            AgentConversationTitle("Stored title".into()),
        ));
        app.world_mut()
            .run_system_once(move |mut queues: vmux_session::QueuedPrompts| {
                queues.enqueue(session, "with profile".into(), Vec::new(), false);
            })
            .unwrap();
        app.update();
        let state = &app.world().resource::<Published>().0.snapshot;
        assert_eq!(state.agent_name, "Canonical agent");
        assert_eq!(state.conversation_title, "Stored title");
        app.world_mut()
            .entity_mut(session)
            .remove::<(Profile, AgentConversationTitle)>();
        app.world_mut()
            .run_system_once(move |mut queues: vmux_session::QueuedPrompts| {
                queues.enqueue(session, "without profile".into(), Vec::new(), false);
            })
            .unwrap();
        app.update();
        let state = &app.world().resource::<Published>().0.snapshot;
        assert!(state.agent_name.is_empty());
        assert_eq!(state.conversation_title, "Queue test");
        app.world_mut().get_mut::<AgentId>(session).unwrap().0 = "other".into();
        app.world_mut()
            .run_system_once(move |mut queues: vmux_session::QueuedPrompts| {
                queues.enqueue(session, "with another agent".into(), Vec::new(), false);
            })
            .unwrap();
        app.update();
        assert_eq!(
            app.world().resource::<Published>().0.snapshot.agent_id,
            "other"
        );
    }

    #[derive(Resource, Default)]
    struct Opened(Vec<vmux_api::command_bar::OpenRequest>);

    impl Opened {
        fn record(
            event: On<UiInput<vmux_api::command_bar::OpenRequest>>,
            mut opened: ResMut<Self>,
        ) {
            opened.0.push(event.payload.clone());
        }
    }

    #[test]
    fn tool_file_placement_is_selected_by_the_host_and_keeps_explicit_destinations() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TranscriptPlugin))
            .init_resource::<Opened>()
            .add_observer(Opened::record);
        let view = app
            .world_mut()
            .spawn((
                SessionView,
                PageMetadata {
                    url: Route::Session(SessionId("files".into())).url(),
                    ..Default::default()
                },
            ))
            .id();
        for (width, requested, expected) in [
            (800.0, None, OpenTarget::InNewStack),
            (
                1600.0,
                None,
                OpenTarget::InPane {
                    direction: PaneDirection::Right,
                    target: PaneTarget::Existing,
                    mode: PaneOpenMode::NewStack,
                },
            ),
            (f64::NAN, None, OpenTarget::InNewStack),
            (1600.0, Some(OpenTarget::InNewTab), OpenTarget::InNewTab),
        ] {
            app.world_mut().trigger(UiInput {
                webview: view,
                payload: crate::event::ChatToolFileOpen {
                    url: "file:///work/main.rs".into(),
                    width,
                    open: requested,
                },
            });
            app.world_mut().flush();
            assert_eq!(
                app.world().resource::<Opened>().0.last().unwrap().open,
                Some(expected)
            );
        }
        let count = app.world().resource::<Opened>().0.len();
        app.world_mut().get_mut::<PageMetadata>(view).unwrap().url = "vmux://start/".into();
        app.world_mut().trigger(UiInput {
            webview: view,
            payload: crate::event::ChatToolFileOpen {
                url: "file:///work/main.rs".into(),
                width: 1600.0,
                open: None,
            },
        });
        app.world_mut().flush();
        assert_eq!(app.world().resource::<Opened>().0.len(), count);
    }

    #[test]
    fn first_snapshot_binds_the_session_before_page_ready() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TranscriptPlugin))
            .init_resource::<Published>()
            .add_observer(Published::record);
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId("starting".into()),
                AgentId("codex".into()),
                Name::new("New conversation"),
                RunState::Idle,
                PromptQueue::default(),
            ))
            .id();
        app.world_mut().spawn(ChildOf(session));
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
            .id();
        let view = app.world_mut().spawn((SessionView, ChildOf(stack))).id();

        app.update();

        let state = &app.world().resource::<Published>().0;
        assert_eq!(state.viewport.session, "starting");
        assert!(state.transcript_revision > 0);
        assert!(!state.snapshot.transcript_loading);
        assert!(app.world().get::<PageReady>(view).is_none());
    }

    #[test]
    fn first_snapshot_restores_the_history_window_before_page_ready() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TranscriptPlugin))
            .init_resource::<Published>()
            .add_observer(Published::record);
        let viewport = ChatViewport {
            top: 240,
            loaded_start: 12,
            following: false,
        };
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId("saved".into()),
                AgentId("codex".into()),
                Name::new("Saved conversation"),
                RunState::Idle,
                PromptQueue::default(),
                viewport,
                ImportedConversation {
                    source_agent: "Codex".into(),
                    source_sid: "original".into(),
                    truncated: false,
                    first_prompt: None,
                },
            ))
            .id();
        for event in ImportedConversation::events(
            session,
            (0..40)
                .map(|index| vmux_api::session::Message::user(format!("message {index}")))
                .collect(),
        ) {
            app.world_mut().spawn(event);
        }
        app.world_mut().spawn(ChildOf(session));
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
            .id();
        let view = app.world_mut().spawn((SessionView, ChildOf(stack))).id();

        app.update();

        let state = &app.world().resource::<Published>().0;
        assert_eq!(state.viewport.session, "saved");
        assert_eq!(state.viewport.position, Some(viewport));
        assert_eq!(state.transcript.loaded_start, viewport.loaded_start);
        assert!(app.world().get::<PageReady>(view).is_none());
    }

    #[test]
    fn retained_history_pages_load_without_a_runtime_stack_binding() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TranscriptPlugin))
            .add_message::<vmux_session::ConversationHistoryMoreRequest>()
            .init_resource::<Published>()
            .add_observer(Published::record);
        let mut messages = Vec::new();
        for index in 0..80 {
            messages.push(vmux_api::session::Message::user(format!("message {index}")));
            messages.push(vmux_api::session::Message::Assistant {
                blocks: vec![vmux_api::session::AssistantBlock::Text(format!(
                    "response {index}"
                ))],
            });
        }
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId("saved".into()),
                AgentId("codex".into()),
                Name::new("Saved conversation"),
                RunState::Idle,
                PromptQueue::default(),
                ImportedConversation {
                    source_agent: "Codex".into(),
                    source_sid: "original".into(),
                    truncated: false,
                    first_prompt: None,
                },
            ))
            .id();
        for event in ImportedConversation::events(session, messages) {
            app.world_mut().spawn(event);
        }
        let stack = app.world_mut().spawn_empty().id();
        let view = app
            .world_mut()
            .spawn((
                SessionView,
                PageReady,
                ChildOf(stack),
                PageMetadata {
                    url: Route::Session(SessionId("saved".into())).url(),
                    ..Default::default()
                },
            ))
            .id();
        app.update();
        app.update();
        assert!(
            app.world()
                .resource::<Published>()
                .0
                .transcript
                .loaded_start
                > 0
        );
        while app
            .world()
            .resource::<Published>()
            .0
            .transcript
            .loaded_start
            > 0
        {
            let before = app
                .world()
                .resource::<Published>()
                .0
                .transcript
                .loaded_start;
            app.world_mut().trigger(UiInput {
                webview: view,
                payload: ChatHistoryMoreRequest,
            });
            app.update();
            app.update();
            let transcript = &app.world().resource::<Published>().0.transcript;
            assert!(
                transcript.loaded_start < before,
                "history must advance toward the first prompt"
            );
            assert!(!transcript.loading);
        }
        let transcript = &app.world().resource::<Published>().0.transcript;
        assert_eq!(transcript.loaded_start, 0);
        assert_eq!(transcript.total, 160);
        assert!(matches!(&transcript.items[0], ChatItem::User { text, .. } if text == "message 0"));
        assert!(!transcript.loading);
    }

    #[test]
    fn setup_lifecycle_reaches_a_synced_page_without_messages_or_status_changes() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TranscriptPlugin))
            .init_resource::<Published>()
            .add_observer(Published::record);
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId("setup-lifecycle".into()),
                AgentId("codex".into()),
                Name::new("Setup"),
                RunState::Errored("connection failed".into()),
                PromptQueue::default(),
                vmux_session::SetupPending,
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
            .id();
        let view = app
            .world_mut()
            .spawn((SessionView, PageReady {}, ChildOf(stack)))
            .id();
        app.update();
        app.update();
        assert!(app.world().get::<ChatSynced>(view).is_some());
        assert!(app.world().resource::<Published>().0.snapshot.setup_pending);
        app.world_mut()
            .entity_mut(session)
            .remove::<vmux_session::SetupPending>();
        app.update();
        let state = &app.world().resource::<Published>().0;
        assert!(!state.snapshot.setup_pending);
        assert_eq!(state.snapshot.status, "errored");
        assert!(state.transcript.items.is_empty());
    }

    #[test]
    fn history_loading_transitions_reach_a_synced_page_without_new_messages() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TranscriptPlugin))
            .init_resource::<Published>()
            .add_observer(Published::record);
        let session = app
            .world_mut()
            .spawn((
                Session,
                AgentId("codex".into()),
                Name::new("Resume conversation"),
                RunState::Idle,
                PromptQueue::default(),
                vmux_session::TranscriptLoading,
            ))
            .id();
        app.world_mut().spawn(ChildOf(session));
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
            .id();
        let view = app
            .world_mut()
            .spawn((SessionView, PageReady {}, ChildOf(stack)))
            .id();
        app.update();
        app.update();
        assert!(app.world().get::<ChatSynced>(view).is_some());
        for loading in [false, true, false] {
            if loading {
                app.world_mut()
                    .entity_mut(session)
                    .insert(vmux_session::TranscriptLoading);
            } else {
                app.world_mut()
                    .entity_mut(session)
                    .remove::<vmux_session::TranscriptLoading>();
            }
            app.update();
            let state = &app.world().resource::<Published>().0;
            assert_eq!(state.snapshot.transcript_loading, loading);
            assert!(state.transcript_revision > 0);
            assert!(state.transcript.items.is_empty());
        }
    }

    #[test]
    fn viewport_is_session_scoped_and_restores_the_loaded_history_window_on_revisit() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TranscriptPlugin))
            .init_resource::<Published>()
            .add_observer(Published::record);
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId("saved".into()),
                AgentId("codex".into()),
                Name::new("Saved conversation"),
                RunState::Idle,
                PromptQueue::default(),
                ImportedConversation {
                    source_agent: "Codex".into(),
                    source_sid: "original".into(),
                    truncated: false,
                    first_prompt: None,
                },
            ))
            .id();
        for event in ImportedConversation::events(
            session,
            (0..40)
                .map(|index| vmux_api::session::Message::user(format!("message {index}")))
                .collect(),
        ) {
            app.world_mut().spawn(event);
        }
        app.world_mut().spawn(ChildOf(session));
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
            .id();
        let metadata = PageMetadata {
            url: Route::Session(SessionId("saved".into())).url(),
            ..Default::default()
        };
        let view = app
            .world_mut()
            .spawn((SessionView, PageReady, metadata.clone(), ChildOf(stack)))
            .id();
        app.update();
        app.update();
        let total = app.world().resource::<Published>().0.transcript.total;
        assert!(total > CHAT_INITIAL_ITEM_LIMIT);
        let viewport = ChatViewport {
            top: 240,
            loaded_start: 12,
            following: false,
        };
        app.world_mut().trigger(UiInput {
            webview: view,
            payload: ChatViewportChanged {
                session: "unrelated".into(),
                viewport,
            },
        });
        app.world_mut().flush();
        assert!(app.world().get::<ChatViewport>(session).is_none());
        app.world_mut().trigger(UiInput {
            webview: view,
            payload: ChatViewportChanged {
                session: "saved".into(),
                viewport,
            },
        });
        app.world_mut().flush();
        app.world_mut().entity_mut(view).despawn();
        app.world_mut()
            .spawn((SessionView, PageReady, metadata, ChildOf(stack)));
        app.update();
        app.update();
        let restored = &app.world().resource::<Published>().0;
        assert_eq!(restored.viewport.session, "saved");
        assert_eq!(restored.viewport.position, Some(viewport));
        assert_eq!(restored.transcript.loaded_start, 12);
        assert_eq!(restored.transcript.total, total);
    }

    #[test]
    fn streaming_snapshots_wait_for_frame_interval() {
        assert!(!PushState::due(
            true,
            false,
            Some(PushState::STREAM_INTERVAL - std::time::Duration::from_millis(1)),
        ));
        assert!(PushState::due(
            true,
            false,
            Some(PushState::STREAM_INTERVAL),
        ));
    }

    #[test]
    fn state_changes_and_completed_turns_push_immediately() {
        assert!(PushState::due(true, true, Some(std::time::Duration::ZERO)));
        assert!(PushState::due(
            false,
            false,
            Some(std::time::Duration::ZERO),
        ));
    }

    #[test]
    fn snapshot_reports_grouped_imported_item_boundary() {
        let agent = AgentId("codex".into());
        let imported = ImportedConversation {
            source_agent: "Codex".into(),
            source_sid: "codex-1".into(),
            truncated: false,
            first_prompt: None,
        };
        let imported_messages = vec![
            vmux_api::session::Message::user("one"),
            vmux_api::session::Message::Assistant {
                blocks: vec![vmux_api::session::AssistantBlock::ToolUse {
                    call_id: "call-1".into(),
                    name: "run".into(),
                    args: "{}".into(),
                    parent_call_id: None,

                    kind: None,
                }],
            },
            vmux_api::session::Message::ToolResult {
                call_id: "call-1".into(),
                content: "two".into(),
                is_error: false,
            },
        ];
        let snapshot = ChatProjection::new(
            &agent,
            &[],
            &[],
            &RunState::Idle,
            None,
            None,
            None,
            None,
            &PromptQueue::default(),
            Vec::new(),
            Some(&imported),
            &imported_messages,
            None,
            "",
            false,
            None,
            None,
            &[],
        )
        .snapshot;

        assert_eq!(snapshot.handoff_message_count, 2);
    }

    #[test]
    fn snapshot_includes_approval_tool_and_input() {
        let agent = AgentId("codex".into());
        let snapshot = ChatProjection::new(
            &agent,
            &[],
            &[],
            &RunState::AwaitingApproval {
                call_id: "call-1".into(),
                name: "vmux.run".into(),
                args: serde_json::json!({"command": "echo hi", "focus": true}),
            },
            None,
            None,
            None,
            None,
            &PromptQueue::default(),
            Vec::new(),
            None,
            &[],
            None,
            "",
            false,
            None,
            None,
            &[],
        )
        .snapshot;

        let approval = snapshot.approval.expect("pending approval");
        assert_eq!(approval.name, "vmux.run");
        assert_eq!(approval.details[0].label, "Command");
        assert_eq!(approval.details[0].value, "echo hi");
        assert_eq!(approval.details[1].label, "Focus");
        assert_eq!(approval.details[1].value, "true");
    }

    #[test]
    fn snapshot_includes_model_written_conversation_title() {
        let agent = AgentId("codex".into());
        let title = AgentConversationTitle("Refine generated chat summaries".into());
        let snapshot = ChatProjection::new(
            &agent,
            &[],
            &[],
            &RunState::Idle,
            None,
            None,
            None,
            None,
            &PromptQueue::default(),
            Vec::new(),
            None,
            &[],
            Some(&title),
            "First prompt",
            false,
            None,
            None,
            &[],
        )
        .snapshot;

        assert_eq!(
            snapshot.conversation_title,
            "Refine generated chat summaries"
        );
    }

    #[test]
    fn snapshot_uses_session_name_until_model_writes_title() {
        let agent = AgentId("codex".into());
        let snapshot = ChatProjection::new(
            &agent,
            &[],
            &[],
            &RunState::Idle,
            None,
            None,
            None,
            None,
            &PromptQueue::default(),
            Vec::new(),
            None,
            &[],
            None,
            "First prompt",
            false,
            None,
            None,
            &[],
        )
        .snapshot;

        assert_eq!(snapshot.conversation_title, "First prompt");
    }

    #[test]
    fn snapshot_uses_the_active_user_profile_avatar() {
        let agent = AgentId("codex".into());
        let profile = Profile::user_named("Personal".into());
        let snapshot = ChatProjection::new(
            &agent,
            &[],
            &[],
            &RunState::Idle,
            None,
            None,
            Some(&profile),
            None,
            &PromptQueue::default(),
            Vec::new(),
            None,
            &[],
            None,
            "",
            false,
            None,
            None,
            &[],
        )
        .snapshot;

        assert_eq!(snapshot.user_name, "Personal");
        assert_eq!(snapshot.user_initials, "P");
        assert_eq!(snapshot.user_color, "#3b82f6");
    }

    #[test]
    fn transcript_history_is_validated_and_merged_by_host_state() {
        let mut world = World::new();
        let webview = world.spawn_empty().id();
        let session = world.spawn_empty().id();
        let mut transcript = ChatTranscriptProjection::default();
        assert!(transcript.merge_tail(TranscriptTail {
            items: vec![ChatItem::user("two"), ChatItem::user("three")],
            start: 2,
            total: 4,
        }));
        let query = transcript
            .start_history_query(webview, session)
            .expect("valid history request");
        let generation = query.generation;
        assert!(transcript.state.loading);
        assert!(transcript.finish_history_query(&ChatHistoryResult {
            webview,
            generation,
            request_id: query.request_id,
            page: Some(TranscriptPage {
                items: vec![ChatItem::user("zero"), ChatItem::user("one")],
                start: 0,
                end: query.before,
                total: 4,
            }),
        }));
        assert_eq!(transcript.state.loaded_start, 0);
        assert_eq!(transcript.state.items.len(), 4);
        assert_eq!(transcript.state.prepend_revision, 1);
        assert!(!transcript.state.loading);
    }

    #[test]
    fn transcript_rejects_stale_generation_and_cursor_results() {
        let mut world = World::new();
        let webview = world.spawn_empty().id();
        let session = world.spawn_empty().id();
        let mut transcript = ChatTranscriptProjection::default();
        transcript.merge_tail(TranscriptTail {
            items: vec![ChatItem::user("two"), ChatItem::user("three")],
            start: 2,
            total: 4,
        });
        let stale = transcript
            .start_history_query(webview, session)
            .expect("initial generation");
        transcript.merge_tail(TranscriptTail {
            items: vec![ChatItem::user("replacement")],
            start: 1,
            total: 2,
        });
        assert!(!transcript.finish_history_query(&ChatHistoryResult {
            webview,
            generation: stale.generation,
            request_id: stale.request_id,
            page: None,
        }));
        let query = transcript
            .start_history_query(webview, session)
            .expect("current generation");
        let generation = query.generation;
        assert!(transcript.finish_history_query(&ChatHistoryResult {
            webview,
            generation,
            request_id: query.request_id,
            page: Some(TranscriptPage {
                items: vec![ChatItem::user("wrong")],
                start: 0,
                end: query.before.saturating_sub(1),
                total: 4,
            }),
        }));
        assert_eq!(transcript.state.loaded_start, 1);
        assert_eq!(transcript.state.items.len(), 1);
        assert!(!transcript.state.loading);
    }

    #[test]
    fn page_ready_clears_chat_synced_only_for_chat_views() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TranscriptPlugin));

        let chat = app.world_mut().spawn((SessionView, ChatSynced)).id();
        let other = app.world_mut().spawn(ChatSynced).id();

        app.world_mut().trigger(UiInput::<PageReady> {
            webview: chat,
            payload: PageReady {},
        });
        app.world_mut().trigger(UiInput::<PageReady> {
            webview: other,
            payload: PageReady {},
        });
        app.world_mut().flush();

        assert!(
            app.world().get::<ChatSynced>(chat).is_none(),
            "a chat view must re-sync (ChatSynced cleared) when the page reloads"
        );
        assert!(
            app.world().get::<ChatSynced>(other).is_some(),
            "a non-chat view must be left untouched"
        );
    }

    #[test]
    fn route_changes_clear_chat_synced_before_reusing_a_view() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TranscriptPlugin));
        let view = app
            .world_mut()
            .spawn((
                SessionView,
                PageMetadata {
                    title: "First".into(),
                    url: "vmux://sessions/codex/first".into(),
                    ..Default::default()
                },
            ))
            .id();
        app.update();
        app.world_mut().entity_mut(view).insert((
            ChatSynced,
            PageMetadata {
                title: "Second".into(),
                url: "vmux://sessions/codex/second".into(),
                ..Default::default()
            },
        ));

        app.update();

        assert!(app.world().get::<ChatSynced>(view).is_none());
    }

    #[test]
    fn streaming_then_idle_records_one_duration() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TranscriptPlugin));
        let e = app.world_mut().spawn(RunState::Streaming).id();
        app.update();
        assert!(
            app.world()
                .get::<AgentTurnMeta>(e)
                .unwrap()
                .turn_start
                .is_some()
        );
        *app.world_mut().get_mut::<RunState>(e).unwrap() = RunState::Idle;
        app.update();
        let meta = app.world().get::<AgentTurnMeta>(e).unwrap();
        assert_eq!(meta.durations.len(), 1);
        assert!(meta.turn_start.is_none());
    }

    #[test]
    fn awaiting_approval_does_not_finalize() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TranscriptPlugin));
        let e = app.world_mut().spawn(RunState::Streaming).id();
        app.update();
        *app.world_mut().get_mut::<RunState>(e).unwrap() = RunState::AwaitingApproval {
            call_id: "c".into(),
            name: "n".into(),
            args: serde_json::Value::Null,
        };
        app.update();
        let meta = app.world().get::<AgentTurnMeta>(e).unwrap();
        assert!(meta.durations.is_empty());
        assert!(meta.turn_start.is_some());
    }
}
