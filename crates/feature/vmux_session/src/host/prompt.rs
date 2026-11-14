use bevy_app::{App, Plugin, Update};
#[cfg(host)]
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use std::collections::HashMap;
use vmux_api::prompt_media::{ChatAttachment, ChatAttachments, ChatMediaEntry};
#[cfg(host)]
use vmux_api::protocol::{AgentAttachment, ClientMessage, SharedMessage};
use vmux_api::session::RemoteMediaEntry;
#[cfg(host)]
use vmux_ecs::service::ServiceRequest;
#[cfg(host)]
use vmux_session::{AgentConversationTitle, QueuedPrompts, RunState, SessionId};

#[cfg(host)]
use super::composer::{ComposerChanged, ComposerState};
use super::conversation::Submitted;
#[cfg(host)]
use super::session::{ChatAttachmentProjection, SessionView};
use super::state::{ChatRuntime, ChatUiStateProjection, RepublishChatUiState};
#[cfg(host)]
use crate::event::{
    ChatApproval, ChatCancel, ChatCancelQueuedPrompt, ChatChoiceSelected, ChatClearQueue,
    ChatEscape, ChatResume, ChatStop, ChatSubmit,
};
use crate::event::{ChatMediaState, ChatPromptFocusEffect};

pub struct ChatPromptPlugin;

impl Plugin for ChatPromptPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Attach>()
            .add_message::<RemoveAttachment>()
            .add_message::<Submitted>()
            .add_message::<Browsed>()
            .add_systems(
                Update,
                (
                    (fold_attachments, remove_attachments, spend_attachments)
                        .chain()
                        .in_set(PromptProjection),
                    receive_browsed.before(project_media),
                    project_media.in_set(PromptProjection),
                    emit_attachments.after(PromptProjection),
                    emit_media.after(PromptProjection),
                ),
            );
    }
}

#[cfg(host)]
pub(crate) struct ChatPromptInputPlugin;

#[cfg(host)]
#[derive(bevy_ecs::system::SystemParam)]
struct SessionControl<'w> {
    requests: MessageWriter<'w, ServiceRequest>,
}

#[cfg(host)]
impl SessionControl<'_> {
    fn cancel(&mut self, session: &SessionId) {
        self.requests.write(ServiceRequest(ClientMessage::Shared(
            SharedMessage::AgentCancel {
                sid: session.0.clone(),
            },
        )));
    }
}

#[cfg(host)]
impl Plugin for ChatPromptInputPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::QueuePlugin>() {
            app.add_plugins(crate::QueuePlugin);
        }
        app.add_message::<ServiceRequest>()
            .add_message::<RecoverPrompt>()
            .add_plugins(UiEventPlugin::<(
                ChatSubmit,
                ChatCancel,
                ChatStop,
                ChatEscape,
                ChatResume,
                ChatClearQueue,
                ChatCancelQueuedPrompt,
            )>::default())
            .add_plugins(UiEventPlugin::<(ChatApproval, ChatChoiceSelected)>::default())
            .add_observer(submit)
            .add_observer(cancel)
            .add_observer(stop)
            .add_observer(escape)
            .add_observer(resume)
            .add_observer(clear_queue)
            .add_observer(cancel_queued)
            .add_systems(
                Update,
                (recover, restore)
                    .chain()
                    .after(vmux_ecs::PageOpenSet::HandleKnownPages),
            );
    }
}

#[cfg(host)]
#[derive(Message)]
pub struct RecoverPrompt {
    pub session: SessionId,
    pub text: String,
}

#[cfg(host)]
#[derive(Component)]
pub(super) struct RecoveredDraft(String);

#[cfg(host)]
fn recover(
    mut requests: MessageReader<RecoverPrompt>,
    sessions: Query<(Entity, &SessionId), With<vmux_session::Session>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        for (entity, session) in &sessions {
            if session == &request.session {
                commands
                    .entity(entity)
                    .insert(RecoveredDraft(request.text.clone()));
            }
        }
    }
}

#[cfg(host)]
fn restore(
    targets: super::session::SessionViews,
    drafts: Query<&RecoveredDraft>,
    mut views: Query<(Entity, &mut ComposerState), With<SessionView>>,
    mut commands: Commands,
) {
    for (view, mut composer) in &mut views {
        let Some(session) = targets.session(view) else {
            continue;
        };
        let Ok(draft) = drafts.get(session) else {
            continue;
        };
        if !composer.draft().is_empty() {
            continue;
        }
        let effect = composer.effect(draft.0.clone(), true);
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(view, &effect),
        );
        commands.trigger(ComposerChanged::new(view));
    }
}

#[cfg(host)]
fn submit(
    trigger: On<UiInput<ChatSubmit>>,
    targets: super::session::SessionViews,
    mut views: Query<
        (&ChildOf, &mut ChatAttachmentProjection, &mut ComposerState),
        With<SessionView>,
    >,
    mut sessions: Query<(&mut RunState, Option<&AgentConversationTitle>)>,
    mut queues: QueuedPrompts,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let text = trigger.event().payload.text.clone();
    let Ok((_, mut selected, mut composer)) = views.get_mut(webview) else {
        return;
    };
    let mut attachments = Vec::new();
    for attachment in &selected.selected {
        if attachment.path.is_empty() {
            continue;
        }
        attachments.push(AgentAttachment {
            path: attachment.path.clone(),
            name: attachment.name.clone(),
            mime_type: attachment.mime_type.clone(),
            size: attachment.size,
        });
    }
    if text.trim().is_empty() && attachments.is_empty() {
        return;
    }
    let Some(session) = targets.session(webview) else {
        return;
    };
    let Ok((mut state, title)) = sessions.get_mut(session) else {
        return;
    };
    if let RunState::Errored(error) = &*state
        && vmux_session::Failure(error).in_use()
    {
        return;
    }
    if title.is_none()
        && let Some(title) = AgentConversationTitle::from_prompt(&text)
    {
        commands
            .entity(session)
            .insert((title, vmux_session::TitlePending));
    }
    queues.enqueue(session, text, attachments, false);
    commands.entity(session).remove::<RecoveredDraft>();
    if matches!(*state, RunState::Errored(_)) {
        *state = RunState::Idle;
    }
    let effect = composer.effect(String::new(), true);
    commands
        .trigger(vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(webview, &effect));
    commands.trigger(ComposerChanged::new(webview));
    if selected.clear_selected() {
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
                webview,
                &selected.state(),
            ),
        );
    }
}

#[cfg(host)]
fn stop(
    trigger: On<UiInput<ChatStop>>,
    targets: super::session::SessionViews,
    mut sessions: Query<(&mut RunState, &SessionId)>,
    mut queues: QueuedPrompts,
    mut control: SessionControl,
    mut commands: Commands,
) {
    let Some(session_entity) = targets.session(trigger.event().webview) else {
        return;
    };
    let Ok((mut state, session)) = sessions.get_mut(session_entity) else {
        return;
    };
    if state.setup_in_progress() {
        commands.trigger(UiInput {
            webview: trigger.event().webview,
            payload: crate::onboarding::ChatCancelSetup,
        });
        return;
    }
    if queues.empty(session_entity) {
        if queues.flush_pending(session_entity) {
            queues.cancel_flush(session_entity);
        }
        control.cancel(session);
        return;
    }
    if queues.request_flush(session_entity) && matches!(*state, RunState::Errored(_)) {
        *state = RunState::Idle;
    }
    if matches!(
        *state,
        RunState::Streaming | RunState::AwaitingApproval { .. }
    ) {
        control.cancel(session);
    }
}

#[cfg(host)]
fn cancel(
    trigger: On<UiInput<ChatCancel>>,
    targets: super::session::SessionViews,
    sessions: Query<(&SessionId, Option<&RunState>)>,
    mut queues: QueuedPrompts,
    mut control: SessionControl,
    mut commands: Commands,
) {
    let Some(session_entity) = targets.session(trigger.event().webview) else {
        return;
    };
    let Ok((session, state)) = sessions.get(session_entity) else {
        return;
    };
    if state.is_some_and(RunState::setup_in_progress) {
        commands.trigger(UiInput {
            webview: trigger.event().webview,
            payload: crate::onboarding::ChatCancelSetup,
        });
        return;
    }
    if queues.flush_pending(session_entity) {
        queues.cancel_flush(session_entity);
    }
    control.cancel(session);
}

#[cfg(host)]
fn escape(
    trigger: On<UiInput<ChatEscape>>,
    targets: super::session::SessionViews,
    mut composers: Query<&mut ComposerState, With<SessionView>>,
    mut sessions: Query<(&mut RunState, &SessionId)>,
    mut queues: QueuedPrompts,
    mut commands: Commands,
    mut control: SessionControl,
) {
    let webview = trigger.event().webview;
    let Some(session_entity) = targets.session(webview) else {
        return;
    };
    let Ok((mut state, session)) = sessions.get_mut(session_entity) else {
        return;
    };
    if state.setup_in_progress() {
        commands.trigger(UiInput {
            webview,
            payload: crate::onboarding::ChatCancelSetup,
        });
        return;
    }
    let running = matches!(
        *state,
        RunState::Streaming | RunState::AwaitingApproval { .. }
    );
    let flush = if queues.empty(session_entity) {
        if queues.flush_pending(session_entity) {
            queues.cancel_flush(session_entity);
        }
        false
    } else {
        queues.request_flush(session_entity)
    };
    if flush && matches!(*state, RunState::Errored(_)) {
        *state = RunState::Idle;
    }
    if running {
        control.cancel(session);
    }
    let Ok(mut composer) = composers.get_mut(webview) else {
        return;
    };
    if !running && queues.empty(session_entity) && !composer.draft().is_empty() {
        commands.entity(session_entity).remove::<RecoveredDraft>();
        let effect = composer.effect(String::new(), true);
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(webview, &effect),
        );
        commands.trigger(ComposerChanged::new(webview));
    }
}

#[cfg(host)]
fn resume(
    trigger: On<UiInput<ChatResume>>,
    targets: super::session::SessionViews,
    mut queues: QueuedPrompts,
) {
    let Some(session) = targets.session(trigger.event().webview) else {
        return;
    };
    queues.resume(session);
}

#[cfg(host)]
fn clear_queue(
    trigger: On<UiInput<ChatClearQueue>>,
    targets: super::session::SessionViews,
    mut queues: QueuedPrompts,
) {
    let Some(session) = targets.session(trigger.event().webview) else {
        return;
    };
    queues.clear(session);
}

#[cfg(host)]
fn cancel_queued(
    trigger: On<UiInput<ChatCancelQueuedPrompt>>,
    targets: super::session::SessionViews,
    mut queues: QueuedPrompts,
) {
    let Some(session) = targets.session(trigger.event().webview) else {
        return;
    };
    queues.remove(session, trigger.event().payload.id);
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PromptProjection;

#[derive(Message)]
pub struct Attach(pub Vec<ChatAttachment>);

#[derive(Message)]
pub struct RemoveAttachment(pub String);

#[derive(Component, Default)]
pub(crate) struct ChatPromptFocusRevision(pub(crate) u64);

impl ChatPromptFocusRevision {
    pub(crate) fn next(&mut self) -> ChatPromptFocusEffect {
        self.0 = self.0.wrapping_add(1).max(1);
        ChatPromptFocusEffect { revision: self.0 }
    }
}

#[derive(Component, Default, PartialEq)]
pub struct Attachments(pub Vec<ChatAttachment>);

#[derive(Component, Default, PartialEq)]
pub(crate) struct AttachmentPreviews(HashMap<String, ChatAttachment>);

impl AttachmentPreviews {
    pub(crate) fn hydrate(&self, attachments: &mut [ChatAttachment]) -> bool {
        let mut changed = false;
        for attachment in attachments {
            if !attachment.preview_data_url.is_empty() {
                continue;
            }
            let Some(preview) = self.0.get(&attachment.path) else {
                continue;
            };
            if preview.preview_data_url.is_empty() {
                continue;
            }
            attachment
                .preview_data_url
                .clone_from(&preview.preview_data_url);
            changed = true;
        }
        changed
    }
}

#[derive(Component, Message, Clone, Default, PartialEq)]
pub struct Browsed {
    pub request_id: u64,
    pub query: String,
    pub entries: Vec<RemoteMediaEntry>,
}

#[derive(Component, Default)]
pub struct Media(pub ChatMediaState);

type ChangedMediaPicker<'w, 's> =
    Query<'w, 's, (&'static Browsed, &'static mut Media), (With<ChatRuntime>, Changed<Browsed>)>;

fn receive_browsed(
    mut messages: MessageReader<Browsed>,
    mut runtimes: Query<&mut Browsed, With<ChatRuntime>>,
) {
    let Ok(mut browsed) = runtimes.single_mut() else {
        return;
    };
    for update in messages.read() {
        if *browsed != *update {
            *browsed = update.clone();
        }
    }
}

fn project_media(mut runtimes: ChangedMediaPicker) {
    let Ok((browsed, mut media)) = runtimes.single_mut() else {
        return;
    };
    let mut entries = Vec::with_capacity(browsed.entries.len());
    for entry in &browsed.entries {
        entries.push(ChatMediaEntry {
            path: entry.path.clone(),
            name: entry.name.clone(),
            parent: entry.parent.clone(),
            mime_type: entry.mime_type.clone(),
            is_dir: entry.is_dir,
            preview_data_url: entry.preview_data_url.clone(),
        });
    }
    media.0 = ChatMediaState {
        request_id: browsed.request_id,
        query: browsed.query.clone(),
        entries,
        loading: false,
    };
}

fn emit_media(
    mut refreshes: MessageReader<RepublishChatUiState>,
    mut runtimes: Query<(Ref<Media>, &mut ChatUiStateProjection), With<ChatRuntime>>,
) {
    let refresh = refreshes.read().count() > 0;
    let Ok((media, mut projection)) = runtimes.single_mut() else {
        return;
    };
    if media.0.request_id == 0 || (!refresh && !media.is_changed()) {
        return;
    }
    projection.write(&media.0);
}

fn emit_attachments(
    mut refreshes: MessageReader<RepublishChatUiState>,
    attachments: Single<Ref<Attachments>, With<ChatRuntime>>,
    focus: Single<Ref<ChatPromptFocusRevision>, With<ChatRuntime>>,
    mut projection: Single<&mut ChatUiStateProjection, With<ChatRuntime>>,
) {
    let refresh = refreshes.read().count() > 0;
    if refresh || attachments.is_changed() {
        let payload = ChatAttachments {
            attachments: attachments.0.clone(),
        };
        projection.write(&payload);
    }
    if focus.0 > 0 && focus.is_changed() {
        projection.write(&ChatPromptFocusEffect { revision: focus.0 });
    }
}

fn spend_attachments(
    mut submitted: MessageReader<Submitted>,
    mut attachments: Single<&mut Attachments, With<ChatRuntime>>,
    mut focus: Single<&mut ChatPromptFocusRevision, With<ChatRuntime>>,
) {
    if submitted.read().count() == 0 || attachments.0.is_empty() {
        return;
    }
    attachments.0.clear();
    focus.next();
}

fn fold_attachments(
    mut asked: MessageReader<Attach>,
    mut attachments: Single<&mut Attachments, With<ChatRuntime>>,
    mut previews: Single<&mut AttachmentPreviews, With<ChatRuntime>>,
    mut focus: Single<&mut ChatPromptFocusRevision, With<ChatRuntime>>,
) {
    let mut changed = false;
    for Attach(added) in asked.read() {
        for attachment in added {
            if attachment.preview_data_url.is_empty() {
                continue;
            }
            previews
                .0
                .insert(attachment.path.clone(), attachment.clone());
        }
        changed |= ChatAttachments {
            attachments: added.clone(),
        }
        .merge_into(&mut attachments.0);
    }
    if changed {
        focus.next();
    }
}

fn remove_attachments(
    mut removed: MessageReader<RemoveAttachment>,
    mut attachments: Single<&mut Attachments, With<ChatRuntime>>,
    mut focus: Single<&mut ChatPromptFocusRevision, With<ChatRuntime>>,
) {
    let mut changed = false;
    for RemoveAttachment(path) in removed.read() {
        let previous = attachments.0.len();
        attachments.0.retain(|attachment| attachment.path != *path);
        changed |= attachments.0.len() != previous;
    }
    if changed {
        focus.next();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::RunSystemOnce;

    #[cfg(host)]
    #[test]
    fn credential_recovery_survives_view_replacement_and_preserves_new_user_input() {
        let mut app = App::new();
        app.add_plugins((
            ChatPromptInputPlugin,
            super::super::composer::ChatComposerPlugin,
        ));
        let session = app
            .world_mut()
            .spawn((
                TestSession::bundle(),
                crate::PromptQueue::default(),
                RunState::Idle,
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn(vmux_ecs::EntityTarget::<vmux_session::Session>::new(
                session,
            ))
            .id();
        let old_view = app.world_mut().spawn((ChildOf(stack), SessionView)).id();
        let id = app.world().get::<SessionId>(session).unwrap().clone();
        app.world_mut().write_message(RecoverPrompt {
            session: id,
            text: "Explain the failing build".into(),
        });
        app.update();
        assert_eq!(
            app.world().get::<ComposerState>(old_view).unwrap().draft(),
            "Explain the failing build"
        );
        let replacement = app.world_mut().spawn((ChildOf(stack), SessionView)).id();
        app.update();
        assert_eq!(
            app.world()
                .get::<ComposerState>(replacement)
                .unwrap()
                .draft(),
            "Explain the failing build"
        );
        app.world_mut().trigger(UiInput {
            webview: replacement,
            payload: crate::event::ChatDraftChanged {
                text: "Keep this newer request".into(),
            },
        });
        app.world_mut().flush();
        app.update();
        assert_eq!(
            app.world()
                .get::<ComposerState>(replacement)
                .unwrap()
                .draft(),
            "Keep this newer request"
        );
        let another_view = app.world_mut().spawn((ChildOf(stack), SessionView)).id();
        app.update();
        assert!(
            app.world()
                .get::<ComposerState>(another_view)
                .unwrap()
                .draft()
                .is_empty()
        );
    }

    struct TestSession;

    impl TestSession {
        fn bundle() -> impl Bundle {
            (
                vmux_session::Session,
                SessionId("session".into()),
                vmux_session::AgentId("mock".into()),
                vmux_ecs::Cwd("/tmp".into()),
                vmux_ecs::ProcessAnchor(vmux_ecs::ProcessId::new()),
            )
        }
    }

    struct Started(App);

    impl Started {
        fn empty() -> Self {
            let mut app = App::new();
            app.add_plugins((crate::host::state::ChatUiStatePlugin, ChatPromptPlugin));
            app.update();
            Self(app)
        }

        fn attach(&mut self, paths: &[&str]) {
            let mut added = Vec::with_capacity(paths.len());
            for path in paths {
                added.push(ChatAttachment {
                    path: path.to_string(),
                    name: path.to_string(),
                    mime_type: String::new(),
                    size: 0,
                    preview_data_url: String::new(),
                    media_url: String::new(),
                });
            }
            self.0.world_mut().write_message(Attach(added));
            self.0.update();
        }

        fn submit(&mut self) {
            self.0.world_mut().write_message(Submitted);
            self.0.update();
        }

        fn remove(&mut self, path: &str) {
            self.0
                .world_mut()
                .write_message(RemoveAttachment(path.to_string()));
            self.0.update();
        }

        fn paths(&self) -> Vec<&str> {
            let mut paths = Vec::new();
            let attachments = self
                .0
                .world()
                .iter_entities()
                .find_map(|entity| entity.get::<Attachments>())
                .expect("chat attachments");
            for attachment in &attachments.0 {
                paths.push(attachment.path.as_str());
            }
            paths
        }

        fn focus_revision(&mut self) -> u64 {
            let mut revisions = self.0.world_mut().query::<&ChatPromptFocusRevision>();
            revisions.single(self.0.world()).unwrap().0
        }
    }

    #[test]
    fn attaching_accumulates_without_repeating_a_path() {
        let mut started = Started::empty();
        started.attach(&["a.png"]);
        started.attach(&["b.png", "a.png"]);

        assert_eq!(started.paths(), ["a.png", "b.png"]);
    }

    #[test]
    fn submitting_spends_the_pile() {
        let mut started = Started::empty();
        started.attach(&["a.png", "b.png"]);
        started.submit();

        assert!(started.paths().is_empty());
    }

    #[test]
    fn removing_the_last_attachment_empties_the_pile() {
        let mut started = Started::empty();
        started.attach(&["a.png"]);
        started.remove("a.png");

        assert!(started.paths().is_empty());
    }

    #[test]
    fn attachment_selection_changes_advance_focus_revision() {
        let mut started = Started::empty();
        started.attach(&["a.png"]);
        assert_eq!(started.focus_revision(), 1);

        started.attach(&["a.png"]);
        assert_eq!(started.focus_revision(), 1);

        started.remove("a.png");
        assert_eq!(started.focus_revision(), 2);

        started.remove("a.png");
        assert_eq!(started.focus_revision(), 2);
    }

    #[cfg(host)]
    struct InputFixture {
        app: App,
        session: Entity,
        webview: Entity,
    }

    #[cfg(host)]
    impl InputFixture {
        fn new(state: RunState) -> Self {
            let mut app = App::new();
            app.add_plugins(ChatPromptInputPlugin);
            let session = app
                .world_mut()
                .spawn((TestSession::bundle(), crate::PromptQueue::default(), state))
                .id();
            let stack = app
                .world_mut()
                .spawn(vmux_ecs::EntityTarget::<crate::Session>::new(session))
                .id();
            let webview = app.world_mut().spawn((ChildOf(stack), SessionView)).id();
            Self {
                app,
                session,
                webview,
            }
        }

        fn submit(&mut self, text: &str) {
            self.app.world_mut().trigger(UiInput {
                webview: self.webview,
                payload: ChatSubmit { text: text.into() },
            });
            self.app.world_mut().flush();
        }

        fn enqueue(&mut self, text: &str) {
            let session = self.session;
            let text = text.to_string();
            self.app
                .world_mut()
                .run_system_once(move |mut queues: QueuedPrompts| {
                    queues.enqueue(session, text.clone(), Vec::new(), false);
                })
                .unwrap();
        }

        fn queued(&mut self) -> Vec<(u64, String)> {
            let world = self.app.world_mut();
            let mut items = world.query_filtered::<(
                &ChildOf,
                &crate::QueuedPromptId,
                &vmux_ecs::Order,
                &vmux_ecs::PendingPrompt,
            ), With<crate::QueuedPrompt>>();
            let mut ordered = Vec::new();
            for (parent, id, order, body) in items.iter(world) {
                if world
                    .get::<ChildOf>(parent.parent())
                    .is_some_and(|root| root.parent() == self.session)
                {
                    ordered.push((order.0, id.0, body.0.clone()));
                }
            }
            ordered.sort_unstable_by_key(|(order, _, _)| *order);
            let mut rows = Vec::new();
            for (_, id, text) in ordered {
                rows.push((id, text));
            }
            rows
        }
    }

    #[cfg(host)]
    #[test]
    fn first_prompt_updates_conversation_title_immediately() {
        let mut fixture = InputFixture::new(RunState::Idle);
        let text = "  make me a new\nJapanese restaurant website  ";
        fixture.submit(text);
        assert_eq!(
            fixture
                .app
                .world()
                .get::<AgentConversationTitle>(fixture.session),
            Some(&AgentConversationTitle(
                "make me a new Japanese restaurant website".into()
            ))
        );
        assert_eq!(fixture.queued(), [(0, text.to_string())]);
    }

    #[cfg(host)]
    #[test]
    fn owned_conversation_rejects_submit_without_spending_the_draft_or_attachments() {
        let mut fixture = InputFixture::new(RunState::Errored(
            "thread already has an active writer".into(),
        ));
        fixture
            .app
            .world_mut()
            .get_mut::<ComposerState>(fixture.webview)
            .unwrap()
            .update("keep this draft");
        fixture
            .app
            .world_mut()
            .get_mut::<ChatAttachmentProjection>(fixture.webview)
            .unwrap()
            .selected
            .push(ChatAttachment {
                path: "/tmp/image.png".into(),
                ..Default::default()
            });
        fixture.submit("keep this draft");
        assert!(fixture.queued().is_empty());
        assert!(
            matches!(fixture.app.world().get::<RunState>(fixture.session), Some(RunState::Errored(error)) if crate::Failure(error).in_use())
        );
        assert!(
            fixture
                .app
                .world()
                .get::<AgentConversationTitle>(fixture.session)
                .is_none()
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ComposerState>(fixture.webview)
                .unwrap()
                .draft(),
            "keep this draft"
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ChatAttachmentProjection>(fixture.webview)
                .unwrap()
                .selected[0]
                .path,
            "/tmp/image.png"
        );
    }

    #[cfg(host)]
    #[test]
    fn submitting_after_error_rearms_prompt_dispatch() {
        let mut fixture = InputFixture::new(RunState::Errored("failed".into()));
        fixture.submit("retry");
        assert!(matches!(
            fixture.app.world().get::<RunState>(fixture.session),
            Some(RunState::Idle)
        ));
        assert_eq!(fixture.queued(), [(0, "retry".into())]);
        assert!(
            !fixture
                .app
                .world()
                .get::<crate::PromptQueue>(fixture.session)
                .unwrap()
                .paused
        );
    }

    #[cfg(host)]
    #[test]
    fn normal_cancel_overrides_pending_flush() {
        let mut fixture = InputFixture::new(RunState::Idle);
        fixture.enqueue("queued");
        let session = fixture.session;
        assert!(
            fixture
                .app
                .world_mut()
                .run_system_once(move |mut queues: QueuedPrompts| queues.request_flush(session))
                .unwrap()
        );
        fixture.app.world_mut().trigger(UiInput {
            webview: fixture.webview,
            payload: ChatCancel,
        });
        fixture.app.world_mut().flush();
        assert!(
            !fixture
                .app
                .world()
                .get::<crate::PromptQueue>(session)
                .unwrap()
                .flush_pending()
        );
        assert_eq!(fixture.queued(), [(0, "queued".into())]);
    }

    #[cfg(host)]
    #[test]
    fn stop_with_queued_work_flushes_and_rearms_the_session() {
        let mut fixture = InputFixture::new(RunState::Errored("failed".into()));
        fixture.enqueue("retry");
        fixture
            .app
            .world_mut()
            .get_mut::<crate::PromptQueue>(fixture.session)
            .unwrap()
            .paused = true;
        fixture.app.world_mut().trigger(UiInput {
            webview: fixture.webview,
            payload: ChatStop,
        });
        fixture.app.world_mut().flush();
        assert!(matches!(
            fixture.app.world().get::<RunState>(fixture.session),
            Some(RunState::Idle)
        ));
        let queue = fixture
            .app
            .world()
            .get::<crate::PromptQueue>(fixture.session)
            .unwrap();
        assert!(queue.flush_pending());
        assert!(!queue.paused);
        assert_eq!(fixture.queued(), [(0, "retry".into())]);
    }

    #[cfg(host)]
    #[test]
    fn idle_escape_clears_the_host_owned_composer_draft() {
        let mut fixture = InputFixture::new(RunState::Idle);
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.session)
            .insert(RecoveredDraft("draft".into()));
        fixture
            .app
            .world_mut()
            .get_mut::<ComposerState>(fixture.webview)
            .unwrap()
            .effect("draft", false);
        fixture.app.world_mut().trigger(UiInput {
            webview: fixture.webview,
            payload: ChatEscape,
        });
        fixture.app.world_mut().flush();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ComposerState>(fixture.webview)
                .unwrap()
                .draft(),
            ""
        );
        assert!(
            fixture
                .app
                .world()
                .get::<RecoveredDraft>(fixture.session)
                .is_none()
        );
    }

    #[cfg(host)]
    #[derive(Resource, Default)]
    struct Interrupted(Vec<Entity>);

    #[cfg(host)]
    fn record_interruption(
        input: On<UiInput<crate::onboarding::ChatCancelSetup>>,
        mut recorded: ResMut<Interrupted>,
    ) {
        recorded.0.push(input.event().webview);
    }

    #[cfg(host)]
    #[test]
    fn standard_stop_cancel_and_escape_route_to_setup_interruption_without_flushing_work() {
        for status in [0, 1, 2] {
            for request in [0, 1, 2] {
                let mut app = App::new();
                app.add_plugins(ChatPromptInputPlugin)
                    .init_resource::<Interrupted>()
                    .add_observer(record_interruption);
                let state = match status {
                    0 => RunState::Preparing,
                    1 => RunState::Installing {
                        detail: "Installing".into(),
                        percent: None,
                    },
                    _ => RunState::AuthenticationRequired {
                        methods: Vec::new(),
                        pending: true,
                    },
                };
                let session = app
                    .world_mut()
                    .spawn((TestSession::bundle(), crate::PromptQueue::default(), state))
                    .id();
                app.world_mut()
                    .run_system_once(move |mut queues: QueuedPrompts| {
                        queues.enqueue(
                            session,
                            "Preserve this real work request".into(),
                            Vec::new(),
                            false,
                        );
                    })
                    .unwrap();
                app.world_mut()
                    .get_mut::<crate::PromptQueue>(session)
                    .unwrap()
                    .paused = true;
                let stack = app
                    .world_mut()
                    .spawn(vmux_ecs::EntityTarget::<vmux_session::Session>::new(
                        session,
                    ))
                    .id();
                let view = app.world_mut().spawn((ChildOf(stack), SessionView)).id();
                app.world_mut()
                    .get_mut::<ComposerState>(view)
                    .unwrap()
                    .effect("Preserve this draft", false);
                match request {
                    0 => app.world_mut().trigger(UiInput {
                        webview: view,
                        payload: ChatStop,
                    }),
                    1 => app.world_mut().trigger(UiInput {
                        webview: view,
                        payload: ChatCancel,
                    }),
                    _ => app.world_mut().trigger(UiInput {
                        webview: view,
                        payload: ChatEscape,
                    }),
                }
                app.world_mut().flush();
                assert_eq!(app.world().resource::<Interrupted>().0, [view]);
                let queue = app.world().get::<crate::PromptQueue>(session).unwrap();
                assert!(queue.paused);
                assert!(!queue.flush_pending());
                assert_eq!(
                    app.world_mut()
                        .run_system_once(move |queues: crate::QueuedPromptReads| queues
                            .visible(session)[0]
                            .1
                            .to_string())
                        .unwrap(),
                    "Preserve this real work request"
                );
                assert_eq!(
                    app.world().get::<ComposerState>(view).unwrap().draft(),
                    "Preserve this draft"
                );
            }
        }
    }

    #[cfg(host)]
    #[test]
    fn cancel_queued_prompt_removes_only_target() {
        let mut fixture = InputFixture::new(RunState::Idle);
        fixture.enqueue("first");
        fixture.enqueue("second");
        let rows = fixture.queued();
        fixture.app.world_mut().trigger(UiInput {
            webview: fixture.webview,
            payload: ChatCancelQueuedPrompt { id: rows[1].0 },
        });
        fixture.app.world_mut().flush();
        assert_eq!(fixture.queued(), [rows[0].clone()]);
    }
}
