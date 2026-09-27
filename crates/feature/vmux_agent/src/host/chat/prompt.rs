use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use super::composer::{ChatComposerProjection, ChatComposerQueriesChanged};
use super::{AgentChatView, ChatAttachmentProjection};
use crate::events::{AgentApprovalReply, AgentChoiceSelected};
use crate::run_state::AgentRunState;
use vmux_api::protocol::{AgentAttachment, ClientMessage, SharedMessage};
use vmux_chat::event::{
    ChatApproval, ChatCancel, ChatCancelQueuedPrompt, ChatChoiceSelected, ChatClearQueue,
    ChatEscape, ChatResume, ChatStop, ChatSubmit,
};
use vmux_service::client::ServiceRequest;
use vmux_session::AcpSession;
use vmux_session::{
    AgentConversationTitle, AgentSession, PromptQueue, provisional_conversation_title,
};

pub(super) struct ChatPromptPlugin;

impl Plugin for ChatPromptPlugin {
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
            .add_observer(on_chat_submit)
            .add_observer(on_chat_cancel)
            .add_observer(on_chat_stop)
            .add_observer(on_chat_escape)
            .add_observer(on_chat_resume)
            .add_observer(on_chat_clear_queue)
            .add_observer(on_chat_cancel_queued_prompt)
            .add_observer(on_chat_approval)
            .add_observer(on_chat_choice_selected);
    }
}

fn on_chat_submit(
    trigger: On<UiInput<ChatSubmit>>,
    mut views: Query<
        (
            &ChildOf,
            &mut ChatAttachmentProjection,
            &mut ChatComposerProjection,
        ),
        With<AgentChatView>,
    >,
    mut sessions: Query<(
        &mut PromptQueue,
        &mut AgentRunState,
        Option<&AgentConversationTitle>,
    )>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let payload = &trigger.event().payload;
    let text = payload.text.clone();
    let Ok((parent, mut selected, mut composer)) = views.get_mut(webview) else {
        return;
    };
    let attachments = selected
        .selected
        .iter()
        .filter(|attachment| !attachment.path.is_empty())
        .map(|attachment| AgentAttachment {
            path: attachment.path.clone(),
            name: attachment.name.clone(),
            mime_type: attachment.mime_type.clone(),
            size: attachment.size,
        })
        .collect::<Vec<_>>();
    if text.trim().is_empty() && attachments.is_empty() {
        return;
    }
    let session = parent.parent();
    if let Ok((mut queue, mut state, title)) = sessions.get_mut(session) {
        if title.is_none()
            && let Some(title) = provisional_conversation_title(&text)
        {
            commands
                .entity(session)
                .insert(AgentConversationTitle(title));
        }
        enqueue_prompt(&mut queue, &mut state, text, attachments);
        let (effect, queries) = composer.effect(String::new(), true);
        commands.trigger(
            vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                webview, &effect,
            ),
        );
        if let Some(changed) = ChatComposerQueriesChanged::new(webview, queries) {
            commands.trigger(changed);
        }
        if selected.clear_selected() {
            commands.trigger(
                vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                    webview,
                    &selected.state(),
                ),
            );
        }
    }
}

fn on_chat_stop(
    trigger: On<UiInput<ChatStop>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(
        &mut PromptQueue,
        &mut AgentRunState,
        Option<&AcpSession>,
        Option<&AgentSession>,
    )>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok((mut queue, mut state, acp, page)) = sessions.get_mut(parent.parent()) else {
        return;
    };
    if queue.items.is_empty() {
        if queue.flush_pending() {
            queue.cancel_flush();
        }
        cancel_session(acp, page, &mut service_requests);
        return;
    }
    if queue.request_flush() && matches!(*state, AgentRunState::Errored(_)) {
        *state = AgentRunState::Idle;
    }
    if matches!(
        *state,
        AgentRunState::Streaming | AgentRunState::AwaitingApproval { .. }
    ) {
        cancel_session(acp, page, &mut service_requests);
    }
}

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

fn on_chat_cancel(
    trigger: On<UiInput<ChatCancel>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(&mut PromptQueue, Option<&AcpSession>, Option<&AgentSession>)>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok((mut queue, acp, page)) = sessions.get_mut(parent.parent()) else {
        return;
    };
    if queue.flush_pending() {
        queue.cancel_flush();
    }
    cancel_session(acp, page, &mut service_requests);
}

fn cancel_session(
    acp: Option<&AcpSession>,
    page: Option<&AgentSession>,
    service_requests: &mut MessageWriter<ServiceRequest>,
) {
    let Some(sid) = acp
        .map(|session| session.sid.clone())
        .or_else(|| page.map(|session| session.sid.clone()))
    else {
        return;
    };
    service_requests.write(ServiceRequest(ClientMessage::Shared(
        SharedMessage::AgentCancel { sid },
    )));
}

fn on_chat_escape(
    trigger: On<UiInput<ChatEscape>>,
    child_of: Query<&ChildOf>,
    mut composers: Query<&mut ChatComposerProjection, With<AgentChatView>>,
    mut sessions: Query<(
        &mut PromptQueue,
        &mut AgentRunState,
        Option<&AcpSession>,
        Option<&AgentSession>,
    )>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let webview = trigger.event().webview;
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    let Ok((mut queue, mut state, acp, page)) = sessions.get_mut(parent.parent()) else {
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
        cancel_session(acp, page, &mut service_requests);
    }
    let Ok(mut composer) = composers.get_mut(webview) else {
        return;
    };
    if !running && queue.items.is_empty() && !composer.draft().is_empty() {
        let (effect, queries) = composer.effect(String::new(), true);
        commands.trigger(
            vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                webview, &effect,
            ),
        );
        if let Some(changed) = ChatComposerQueriesChanged::new(webview, queries) {
            commands.trigger(changed);
        }
    }
}

fn on_chat_resume(
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

fn on_chat_clear_queue(
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

fn on_chat_cancel_queued_prompt(
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

fn on_chat_approval(
    trigger: On<UiInput<ChatApproval>>,
    child_of: Query<&ChildOf>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let payload = &trigger.event().payload;
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    commands.trigger(AgentApprovalReply {
        session: parent.parent(),
        call_id: payload.call_id.clone(),
        decision: payload.decision,
    });
}

fn on_chat_choice_selected(trigger: On<UiInput<ChatChoiceSelected>>, mut commands: Commands) {
    commands.trigger(AgentChoiceSelected {
        webview: trigger.event().webview,
        index: trigger.event().payload.index as usize,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_message::<ServiceRequest>();
        app
    }

    #[test]
    fn first_prompt_updates_conversation_title_immediately() {
        let mut app = test_app();
        app.add_observer(on_chat_submit);
        let session = app
            .world_mut()
            .spawn((PromptQueue::default(), AgentRunState::Idle))
            .id();
        let webview = app
            .world_mut()
            .spawn((ChildOf(session), AgentChatView))
            .id();

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

        app.world_mut().trigger(UiInput {
            webview,
            payload: ChatSubmit {
                text: "make it darker".into(),
            },
        });
        app.world_mut().flush();

        assert_eq!(
            app.world().get::<AgentConversationTitle>(session),
            Some(&AgentConversationTitle(
                "make me a new Japanese restaurant website".into()
            ))
        );
    }

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

    #[test]
    fn normal_cancel_overrides_pending_flush() {
        let mut app = test_app();
        app.add_observer(on_chat_cancel);
        let mut queue = PromptQueue::default();
        queue.enqueue("queued".into());
        assert!(queue.request_flush());
        let stack = app.world_mut().spawn(queue).id();
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

    #[test]
    fn stop_with_queued_work_flushes_and_rearms_the_session() {
        let mut app = test_app();
        app.add_observer(on_chat_stop);
        let mut queue = PromptQueue::default();
        queue.enqueue("retry".into());
        queue.paused = true;
        let stack = app
            .world_mut()
            .spawn((queue, AgentRunState::Errored("failed".into())))
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

    #[test]
    fn escape_flush_rearms_errored_queue() {
        let mut app = test_app();
        app.add_observer(on_chat_escape);
        let mut queue = PromptQueue::default();
        queue.enqueue("retry".into());
        queue.paused = true;
        let stack = app
            .world_mut()
            .spawn((queue, AgentRunState::Errored("failed".into())))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(UiInput::<ChatEscape> {
            webview,
            payload: ChatEscape,
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

    #[test]
    fn escape_without_queue_clears_stale_flush() {
        let mut app = test_app();
        app.add_observer(on_chat_escape);
        let mut queue = PromptQueue::default();
        queue.enqueue("queued".into());
        assert!(queue.request_flush());
        queue.items.clear();
        let stack = app
            .world_mut()
            .spawn((queue, AgentRunState::Streaming))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(UiInput::<ChatEscape> {
            webview,
            payload: ChatEscape,
        });
        app.world_mut().flush();

        assert!(
            !app.world()
                .get::<PromptQueue>(stack)
                .unwrap()
                .flush_pending()
        );
    }

    #[test]
    fn idle_escape_clears_the_host_owned_composer_draft() {
        let mut app = test_app();
        app.add_observer(on_chat_escape);
        let stack = app
            .world_mut()
            .spawn((PromptQueue::default(), AgentRunState::Idle))
            .id();
        let webview = app.world_mut().spawn((ChildOf(stack), AgentChatView)).id();
        app.world_mut()
            .get_mut::<ChatComposerProjection>(webview)
            .unwrap()
            .effect("draft", false);

        app.world_mut().trigger(UiInput::<ChatEscape> {
            webview,
            payload: ChatEscape,
        });
        app.world_mut().flush();

        assert_eq!(
            app.world()
                .get::<ChatComposerProjection>(webview)
                .unwrap()
                .draft(),
            ""
        );
    }

    #[test]
    fn cancel_queued_prompt_removes_only_target() {
        let mut app = test_app();
        app.add_observer(on_chat_cancel_queued_prompt);
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
