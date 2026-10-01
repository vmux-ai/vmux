use bevy_app::{App, Plugin, Update};
#[cfg(host)]
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use std::collections::HashMap;
use vmux_api::prompt_media::{ChatAttachment, ChatAttachments, ChatMediaEntry};
#[cfg(host)]
use vmux_api::protocol::{AgentAttachment, ClientMessage, SharedMessage};
use vmux_api::room::RemoteMediaEntry;
#[cfg(host)]
use vmux_ecs::service::ServiceRequest;
#[cfg(host)]
use vmux_session::{AcpSession, AgentConversationTitle, AgentRunState, PromptQueue};

#[cfg(host)]
use super::composer::{ComposerChanged, ComposerState};
use super::room::Submitted;
#[cfg(host)]
use super::session::{ChatAttachmentProjection, ChatView};
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
impl Plugin for ChatPromptInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceRequest>()
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
            .add_observer(cancel_queued);
    }
}

#[cfg(host)]
fn submit(
    trigger: On<UiInput<ChatSubmit>>,
    mut views: Query<(&ChildOf, &mut ChatAttachmentProjection, &mut ComposerState), With<ChatView>>,
    mut sessions: Query<(
        &mut PromptQueue,
        &mut AgentRunState,
        Option<&AgentConversationTitle>,
    )>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let text = trigger.event().payload.text.clone();
    let Ok((parent, mut selected, mut composer)) = views.get_mut(webview) else {
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
    let session = parent.parent();
    let Ok((mut queue, mut state, title)) = sessions.get_mut(session) else {
        return;
    };
    if title.is_none()
        && let Some(title) = AgentConversationTitle::from_prompt(&text)
    {
        commands.entity(session).insert(title);
    }
    enqueue_prompt(&mut queue, &mut state, text, attachments);
    let effect = composer.effect(String::new(), true);
    commands.trigger(
        vmux_ecs::host::UiStateWrite::<crate::state::ChatUiState>::from_event(webview, &effect),
    );
    commands.trigger(ComposerChanged::new(webview));
    if selected.clear_selected() {
        commands.trigger(
            vmux_ecs::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
                webview,
                &selected.state(),
            ),
        );
    }
}

#[cfg(host)]
fn stop(
    trigger: On<UiInput<ChatStop>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(&mut PromptQueue, &mut AgentRunState, &AcpSession)>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok((mut queue, mut state, session)) = sessions.get_mut(parent.parent()) else {
        return;
    };
    if queue.items.is_empty() {
        if queue.flush_pending() {
            queue.cancel_flush();
        }
        cancel_session(session, &mut service_requests);
        return;
    }
    if queue.request_flush() && matches!(*state, AgentRunState::Errored(_)) {
        *state = AgentRunState::Idle;
    }
    if matches!(
        *state,
        AgentRunState::Streaming | AgentRunState::AwaitingApproval { .. }
    ) {
        cancel_session(session, &mut service_requests);
    }
}

#[cfg(host)]
fn enqueue_prompt(
    queue: &mut PromptQueue,
    state: &mut AgentRunState,
    text: String,
    attachments: Vec<AgentAttachment>,
) {
    queue.enqueue_with_attachments(text, attachments);
    if matches!(state, AgentRunState::Errored(_)) {
        *state = AgentRunState::Idle;
    }
}

#[cfg(host)]
fn cancel(
    trigger: On<UiInput<ChatCancel>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(&mut PromptQueue, &AcpSession)>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok((mut queue, session)) = sessions.get_mut(parent.parent()) else {
        return;
    };
    if queue.flush_pending() {
        queue.cancel_flush();
    }
    cancel_session(session, &mut service_requests);
}

#[cfg(host)]
fn cancel_session(session: &AcpSession, service_requests: &mut MessageWriter<ServiceRequest>) {
    service_requests.write(ServiceRequest(ClientMessage::Shared(
        SharedMessage::AgentCancel {
            sid: session.sid.clone(),
        },
    )));
}

#[cfg(host)]
fn escape(
    trigger: On<UiInput<ChatEscape>>,
    child_of: Query<&ChildOf>,
    mut composers: Query<&mut ComposerState, With<ChatView>>,
    mut sessions: Query<(&mut PromptQueue, &mut AgentRunState, &AcpSession)>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let webview = trigger.event().webview;
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    let Ok((mut queue, mut state, session)) = sessions.get_mut(parent.parent()) else {
        return;
    };
    let running = matches!(
        *state,
        AgentRunState::Streaming | AgentRunState::AwaitingApproval { .. }
    );
    let flush = if queue.items.is_empty() {
        if queue.flush_pending() {
            queue.cancel_flush();
        }
        false
    } else {
        queue.request_flush()
    };
    if flush && matches!(*state, AgentRunState::Errored(_)) {
        *state = AgentRunState::Idle;
    }
    if running {
        cancel_session(session, &mut service_requests);
    }
    let Ok(mut composer) = composers.get_mut(webview) else {
        return;
    };
    if !running && queue.items.is_empty() && !composer.draft().is_empty() {
        let effect = composer.effect(String::new(), true);
        commands.trigger(
            vmux_ecs::host::UiStateWrite::<crate::state::ChatUiState>::from_event(webview, &effect),
        );
        commands.trigger(ComposerChanged::new(webview));
    }
}

#[cfg(host)]
fn resume(
    trigger: On<UiInput<ChatResume>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.resume();
    }
}

#[cfg(host)]
fn clear_queue(
    trigger: On<UiInput<ChatClearQueue>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.clear();
    }
}

#[cfg(host)]
fn cancel_queued(
    trigger: On<UiInput<ChatCancelQueuedPrompt>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.remove(trigger.event().payload.id);
    }
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
    let refresh = refreshes.read().next().is_some();
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
    let refresh = refreshes.read().next().is_some();
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

    struct TestSession;

    impl TestSession {
        fn acp() -> AcpSession {
            AcpSession {
                agent_id: "mock".into(),
                sid: "session".into(),
                cwd: std::path::PathBuf::from("/tmp"),
                anchor: vmux_ecs::ProcessId::new(),
                resume: None,
            }
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
    fn input_app() -> App {
        let mut app = App::new();
        app.add_message::<ServiceRequest>();
        app
    }

    #[cfg(host)]
    #[test]
    fn first_prompt_updates_conversation_title_immediately() {
        let mut app = input_app();
        app.add_observer(submit);
        let session = app
            .world_mut()
            .spawn((PromptQueue::default(), AgentRunState::Idle))
            .id();
        let webview = app.world_mut().spawn((ChildOf(session), ChatView)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ChatSubmit {
                text: "  make me a new\nJapanese restaurant website  ".into(),
            },
        });
        app.world_mut().flush();

        assert_eq!(
            app.world().get::<AgentConversationTitle>(session),
            Some(&AgentConversationTitle(
                "make me a new Japanese restaurant website".into()
            ))
        );
        assert_eq!(
            app.world()
                .get::<PromptQueue>(session)
                .and_then(|queue| queue.items.front())
                .map(|prompt| prompt.text.as_str()),
            Some("  make me a new\nJapanese restaurant website  ")
        );
    }

    #[cfg(host)]
    #[test]
    fn submitting_after_error_rearms_prompt_dispatch() {
        let mut queue = PromptQueue::default();
        let mut state = AgentRunState::Errored("failed".into());

        enqueue_prompt(&mut queue, &mut state, "retry".into(), Vec::new());

        assert!(matches!(state, AgentRunState::Idle));
        assert_eq!(
            queue.items.front().map(|item| item.text.as_str()),
            Some("retry")
        );
        assert!(!queue.paused);
    }

    #[cfg(host)]
    #[test]
    fn normal_cancel_overrides_pending_flush() {
        let mut app = input_app();
        app.add_observer(cancel);
        let mut queue = PromptQueue::default();
        queue.enqueue("queued".into());
        assert!(queue.request_flush());
        let stack = app.world_mut().spawn((TestSession::acp(), queue)).id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(UiInput::<ChatCancel> {
            webview,
            payload: ChatCancel,
        });
        app.world_mut().flush();

        assert!(
            !app.world()
                .get::<PromptQueue>(stack)
                .unwrap()
                .flush_pending()
        );
    }

    #[cfg(host)]
    #[test]
    fn stop_with_queued_work_flushes_and_rearms_the_session() {
        let mut app = input_app();
        app.add_observer(stop);
        let mut queue = PromptQueue::default();
        queue.enqueue("retry".into());
        queue.paused = true;
        let stack = app
            .world_mut()
            .spawn((
                TestSession::acp(),
                queue,
                AgentRunState::Errored("failed".into()),
            ))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(UiInput::<ChatStop> {
            webview,
            payload: ChatStop,
        });
        app.world_mut().flush();

        assert!(matches!(
            app.world().get::<AgentRunState>(stack),
            Some(AgentRunState::Idle)
        ));
        let queue = app.world().get::<PromptQueue>(stack).unwrap();
        assert!(queue.flush_pending());
        assert!(!queue.paused);
    }

    #[cfg(host)]
    #[test]
    fn idle_escape_clears_the_host_owned_composer_draft() {
        let mut app = input_app();
        app.add_observer(escape);
        let stack = app
            .world_mut()
            .spawn((
                TestSession::acp(),
                PromptQueue::default(),
                AgentRunState::Idle,
            ))
            .id();
        let webview = app.world_mut().spawn((ChildOf(stack), ChatView)).id();
        app.world_mut()
            .get_mut::<ComposerState>(webview)
            .unwrap()
            .effect("draft", false);

        app.world_mut().trigger(UiInput::<ChatEscape> {
            webview,
            payload: ChatEscape,
        });
        app.world_mut().flush();

        assert_eq!(
            app.world().get::<ComposerState>(webview).unwrap().draft(),
            ""
        );
    }

    #[cfg(host)]
    #[test]
    fn cancel_queued_prompt_removes_only_target() {
        let mut app = input_app();
        app.add_observer(cancel_queued);
        let mut queue = PromptQueue::default();
        queue.enqueue("first".into());
        queue.enqueue("second".into());
        let second_id = queue.items[1].id;
        let stack = app.world_mut().spawn(queue).id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(UiInput::<ChatCancelQueuedPrompt> {
            webview,
            payload: ChatCancelQueuedPrompt { id: second_id },
        });
        app.world_mut().flush();

        let queue = app.world().get::<PromptQueue>(stack).unwrap();
        assert_eq!(queue.items.len(), 1);
        assert_eq!(queue.items[0].text, "first");
    }
}
