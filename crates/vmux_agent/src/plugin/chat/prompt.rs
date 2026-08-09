//! What the composer sends, and the queue it lands in.
//!
//! A prompt is never dispatched from here: it is enqueued, and the runtime drains the queue. That
//! is what lets a second prompt arrive mid-turn, and it is why interrupting and "send everything
//! now" are both edits to the queue rather than special cases at the send site. Approvals and
//! choices share this module because they are the other two things the page answers with.

use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};

use crate::client::acp::AcpSession;
use crate::components::{
    AgentConversationTitle, AgentSession, PromptQueue, provisional_conversation_title,
};
use crate::event::chat::{
    ChatApproval, ChatCancel, ChatCancelQueuedPrompt, ChatChoiceSelected, ChatClearQueue,
    ChatEscape, ChatResume, ChatSubmit,
};
use crate::events::{AgentApprovalReply, AgentChoiceSelected, ApprovalDecision};
use crate::run_state::AgentRunState;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{AgentAttachment, ClientMessage, SharedMessage};

/// Submitting a prompt, everything that acts on the queue behind it, and the two answers the page
/// can give a running turn.
pub(super) struct ChatPromptPlugin;

impl Plugin for ChatPromptPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BinEventEmitterPlugin::<(
            ChatSubmit,
            ChatCancel,
            ChatEscape,
            ChatResume,
            ChatClearQueue,
            ChatCancelQueuedPrompt,
        )>::for_hosts(&["agent", "start"]))
            .add_plugins(
                BinEventEmitterPlugin::<(ChatApproval, ChatChoiceSelected)>::for_hosts(&[
                    "agent", "start",
                ]),
            )
            .add_observer(on_chat_submit)
            .add_observer(on_chat_cancel)
            .add_observer(on_chat_escape)
            .add_observer(on_chat_resume)
            .add_observer(on_chat_clear_queue)
            .add_observer(on_chat_cancel_queued_prompt)
            .add_observer(on_chat_approval)
            .add_observer(on_chat_choice_selected);
    }
}

fn on_chat_submit(
    trigger: On<BinReceive<ChatSubmit>>,
    child_of: Query<&ChildOf>,
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
    let attachments = payload
        .attachments
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
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
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
    trigger: On<BinReceive<ChatCancel>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(&mut PromptQueue, Option<&AcpSession>, Option<&AgentSession>)>,
    service: Option<Res<ServiceClient>>,
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
    cancel_session(service.as_deref(), acp, page);
}

fn cancel_session(
    service: Option<&ServiceClient>,
    acp: Option<&AcpSession>,
    page: Option<&AgentSession>,
) {
    let Some(service) = service else {
        return;
    };
    let Some(sid) = acp
        .map(|session| session.sid.clone())
        .or_else(|| page.map(|session| session.sid.clone()))
    else {
        return;
    };
    service.0.send(ClientMessage::Shared(SharedMessage::agent(
        sid,
        vmux_wire::protocol::AgentAction::Cancel,
    )));
}

fn on_chat_escape(
    trigger: On<BinReceive<ChatEscape>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(
        &mut PromptQueue,
        &mut AgentRunState,
        Option<&AcpSession>,
        Option<&AgentSession>,
    )>,
    service: Option<Res<ServiceClient>>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
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
        cancel_session(service.as_deref(), acp, page);
    }
}

fn on_chat_resume(
    trigger: On<BinReceive<ChatResume>>,
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
    trigger: On<BinReceive<ChatClearQueue>>,
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
    trigger: On<BinReceive<ChatCancelQueuedPrompt>>,
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
    trigger: On<BinReceive<ChatApproval>>,
    child_of: Query<&ChildOf>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let payload = &trigger.event().payload;
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    let decision = match payload.decision {
        1 => ApprovalDecision::Allow,
        2 => ApprovalDecision::AllowAlways,
        _ => ApprovalDecision::Deny,
    };
    commands.trigger(AgentApprovalReply {
        session: parent.parent(),
        call_id: payload.call_id.clone(),
        decision,
    });
}

fn on_chat_choice_selected(trigger: On<BinReceive<ChatChoiceSelected>>, mut commands: Commands) {
    commands.trigger(AgentChoiceSelected {
        webview: trigger.event().webview,
        index: trigger.event().payload.index as usize,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_prompt_updates_conversation_title_immediately() {
        let mut app = App::new();
        app.add_observer(on_chat_submit);
        let session = app
            .world_mut()
            .spawn((PromptQueue::default(), AgentRunState::Idle))
            .id();
        let webview = app.world_mut().spawn(ChildOf(session)).id();

        app.world_mut().trigger(BinReceive {
            webview,
            payload: ChatSubmit {
                text: "  make me a new\nJapanese restaurant website  ".into(),
                attachments: Vec::new(),
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

        app.world_mut().trigger(BinReceive {
            webview,
            payload: ChatSubmit {
                text: "make it darker".into(),
                attachments: Vec::new(),
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
        let mut app = App::new();
        app.add_observer(on_chat_cancel);
        let mut queue = PromptQueue::default();
        queue.enqueue("queued".into());
        assert!(queue.request_flush());
        let stack = app.world_mut().spawn(queue).id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive::<ChatCancel> {
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
    fn escape_flush_rearms_errored_queue() {
        let mut app = App::new();
        app.add_observer(on_chat_escape);
        let mut queue = PromptQueue::default();
        queue.enqueue("retry".into());
        queue.paused = true;
        let stack = app
            .world_mut()
            .spawn((queue, AgentRunState::Errored("failed".into())))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive::<ChatEscape> {
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
        let mut app = App::new();
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

        app.world_mut().trigger(BinReceive::<ChatEscape> {
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
    fn cancel_queued_prompt_removes_only_target() {
        let mut app = App::new();
        app.add_observer(on_chat_cancel_queued_prompt);
        let mut queue = PromptQueue::default();
        queue.enqueue("first".into());
        queue.enqueue("second".into());
        let second_id = queue.items[1].id;
        let stack = app.world_mut().spawn(queue).id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut()
            .trigger(BinReceive::<ChatCancelQueuedPrompt> {
                webview,
                payload: ChatCancelQueuedPrompt { id: second_id },
            });
        app.world_mut().flush();

        let queue = app.world().get::<PromptQueue>(stack).unwrap();
        assert_eq!(queue.items.len(), 1);
        assert_eq!(queue.items[0].text, "first");
    }
}
