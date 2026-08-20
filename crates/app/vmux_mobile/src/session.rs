//! What the phone is attached to, and how a room's events fold into what it draws.

use crate::api::{Api, ApiError, next_client_op_id, remote_event_from_shared};
use crate::native_transition;
use crate::take_resumed;
use dioxus::prelude::*;
use std::time::Duration;
use vmux_service::chat::group_turns_tail;
use vmux_ui::i18n::translate;
use vmux_wire::chat::ChatItem;
use vmux_wire::room::{
    AssistantBlock, Message, NewChatRequest, RemoteApproval, RemoteEvent, RemoteSession,
    RemoteStatus, RoomEvent, RoomId,
};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum AuthState {
    Loading,
    Paired,
    Unpaired,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MobileRoomProjection {
    room_id: Option<RoomId>,
    through_seq: u64,
    events: Vec<RoomEvent>,
}

impl MobileRoomProjection {
    /// How many events have been folded in so far.
    ///
    /// Read by the transcript's scroll effect purely to depend on the log growing.
    pub(crate) fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Fold the replayed log into rendered chat items, with any streaming delta as the tail of
    /// the live turn.
    ///
    /// The phone has no imported history and no per-turn durations, so it always asks for the
    /// whole transcript and lets every turn resolve its duration to `None`.
    pub(crate) fn chat_items(&self, live_delta: &str, running: bool) -> Vec<ChatItem> {
        let mut messages = Vec::with_capacity(self.events.len() + 1);
        for event in &self.events {
            messages.push(event.message.clone());
        }
        if !live_delta.is_empty() {
            messages.push(Message::Assistant {
                blocks: vec![AssistantBlock::Text(live_delta.to_string())],
            });
        }
        group_turns_tail(&[], &messages, &[], running, usize::MAX).items
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start_new_chat(
    api: Signal<Option<Api>>,
    mut sessions: Signal<Vec<RemoteSession>>,
    current: Signal<Option<RemoteSession>>,
    room: Signal<MobileRoomProjection>,
    live_delta: Signal<String>,
    status: Signal<RemoteStatus>,
    approval: Signal<Option<RemoteApproval>>,
    connected: Signal<bool>,
    stream_generation: Signal<u64>,
    mut draft: Signal<String>,
    mut error: Signal<String>,
    mut creating: Signal<bool>,
    agent_url: Option<String>,
) {
    let text = draft.peek().trim().to_string();
    let Some(client) = api() else { return };
    if text.is_empty() || creating() {
        return;
    }
    let known = sessions
        .read()
        .iter()
        .map(|session| session.sid.clone())
        .collect::<std::collections::HashSet<_>>();
    creating.set(true);
    error.set(String::new());
    spawn(async move {
        match client
            .create_chat(&NewChatRequest {
                client_op_id: next_client_op_id(),
                text,
                agent_url,
            })
            .await
        {
            Ok(()) => {
                for _ in 0..40 {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    if let Ok(next) = client.sessions().await {
                        let created = next
                            .iter()
                            .find(|session| !known.contains(&session.sid))
                            .cloned();
                        sessions.set(next);
                        if let Some(created) = created {
                            draft.set(String::new());
                            creating.set(false);
                            open_session(
                                client,
                                created,
                                current,
                                room,
                                live_delta,
                                status,
                                approval,
                                connected,
                                stream_generation,
                            );
                            return;
                        }
                    }
                }
                error.set(translate("mobile-error-stack-missing"));
            }
            Err(ApiError::Unauthorized) => {
                error.set(translate("mobile-error-pairing-lost"));
            }
            Err(other) => error.set(other.to_string()),
        }
        creating.set(false);
    });
}

pub(crate) fn leave_session(
    mut current: Signal<Option<RemoteSession>>,
    mut room: Signal<MobileRoomProjection>,
    mut live_delta: Signal<String>,
    mut status: Signal<RemoteStatus>,
    mut approval: Signal<Option<RemoteApproval>>,
    mut connected: Signal<bool>,
    mut generation: Signal<u64>,
) {
    let dismissing = native_transition::NativeSheet::close();
    generation.set(generation().wrapping_add(1));
    current.set(None);
    room.set(MobileRoomProjection::default());
    live_delta.set(String::new());
    status.set(RemoteStatus::Idle);
    approval.set(None);
    connected.set(false);
    dismissing.finish();
}

/// Fold the room's event log into the shared transcript model. The desktop gets this from
/// `group_turns` on the daemon side; the relay does not pre-group yet, so mobile folds locally.
#[allow(clippy::too_many_arguments)]
pub(crate) fn open_session(
    api: Api,
    session: RemoteSession,
    mut current: Signal<Option<RemoteSession>>,
    mut room: Signal<MobileRoomProjection>,
    mut live_delta: Signal<String>,
    mut status: Signal<RemoteStatus>,
    mut approval: Signal<Option<RemoteApproval>>,
    mut connected: Signal<bool>,
    mut generation: Signal<u64>,
) {
    native_transition::NativeSheet::open();
    let sid = session.sid.clone();
    current.set(Some(session.clone()));
    room.set(MobileRoomProjection {
        room_id: Some(session.room_id.clone()),
        ..MobileRoomProjection::default()
    });
    live_delta.set(String::new());
    status.set(session.status.clone());
    approval.set(session.approval.clone());
    connected.set(false);
    let next_generation = generation().wrapping_add(1);
    generation.set(next_generation);
    spawn(async move {
        loop {
            if generation() != next_generation {
                return;
            }
            // A connection that survived a suspend is a connection whose socket the OS already
            // closed. Drop it before dialling rather than waiting for a request to stall.
            if take_resumed() {
                api.reset_transport().await;
            }
            match api.subscribe(&sid).await {
                Ok(mut subscription) => {
                    connected.set(true);
                    while let Some(event) = subscription.next().await {
                        if generation() != next_generation {
                            return;
                        }
                        let Some(event) = remote_event_from_shared(event) else {
                            continue;
                        };
                        let refresh_now = matches!(&event, RemoteEvent::Approval { .. });
                        apply_remote_event(event, current, room, live_delta, status, approval);
                        if refresh_now {
                            tokio::task::yield_now().await;
                        }
                    }
                }
                // Pairing is gone, or the stream route is — reconnecting brings back neither.
                Err(ApiError::Unauthorized | ApiError::NotFound) => return,
                Err(ApiError::Message(_)) => {}
            }
            connected.set(false);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

pub(crate) fn apply_remote_event(
    event: RemoteEvent,
    mut current: Signal<Option<RemoteSession>>,
    mut room: Signal<MobileRoomProjection>,
    mut live_delta: Signal<String>,
    mut status: Signal<RemoteStatus>,
    mut approval: Signal<Option<RemoteApproval>>,
) {
    match event {
        RemoteEvent::Session { session } => {
            if room
                .peek()
                .room_id
                .as_ref()
                .is_some_and(|room_id| room_id != &session.room_id)
            {
                room.set(MobileRoomProjection::default());
            }
            status.set(session.status.clone());
            approval.set(session.approval.clone());
            current.set(Some(session));
        }
        RemoteEvent::Snapshot {
            room_id,
            through_seq,
            events,
        } => {
            let matches_session = current
                .peek()
                .as_ref()
                .is_none_or(|session| session.room_id == room_id);
            let has_newer_projection = {
                let projection = room.peek();
                projection.room_id.as_ref() == Some(&room_id)
                    && projection.through_seq > through_seq
            };
            if matches_session && !has_newer_projection {
                room.set(MobileRoomProjection {
                    room_id: Some(room_id),
                    through_seq,
                    events,
                });
                live_delta.set(String::new());
            }
        }
        RemoteEvent::Delta { room_id, text } => {
            let accepts_delta = room
                .peek()
                .room_id
                .as_ref()
                .is_none_or(|current| current == &room_id);
            if accepts_delta {
                if room.peek().room_id.is_none() {
                    room.write().room_id = Some(room_id);
                }
                live_delta.write().push_str(&text);
            }
        }
        RemoteEvent::Status { status: next } => {
            if !matches!(next, RemoteStatus::Streaming) {
                approval.set(None);
            }
            status.set(next);
        }
        RemoteEvent::Approval { approval: next } => approval.set(next),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_wire::chat::ChatBlock;

    impl MobileRoomProjection {
        fn sample() -> Self {
            Self {
                room_id: None,
                through_seq: 0,
                events: RoomEvent::from_messages(
                    "s",
                    0,
                    &[
                        Message::user("hello"),
                        Message::Assistant {
                            blocks: vec![AssistantBlock::Thinking("working".to_string())],
                        },
                        Message::ToolResult {
                            call_id: "tool-1".to_string(),
                            content: "done".to_string(),
                            is_error: false,
                        },
                        Message::Assistant {
                            blocks: vec![AssistantBlock::Text("answer".to_string())],
                        },
                    ],
                ),
            }
        }
    }

    #[test]
    fn groups_agent_activity_into_one_turn() {
        let items = MobileRoomProjection::sample().chat_items("", false);

        assert_eq!(items.len(), 2);
        assert!(matches!(items[0], ChatItem::User { .. }));
        assert!(matches!(
            &items[1],
            ChatItem::Turn(turn) if turn.blocks.len() == 3 && !turn.running
        ));
    }

    #[test]
    fn streaming_delta_extends_the_live_turn() {
        let items = MobileRoomProjection::sample().chat_items("partial", true);

        let ChatItem::Turn(turn) = &items[1] else {
            panic!("expected a turn");
        };
        assert!(turn.running);
        assert_eq!(
            turn.blocks.last(),
            Some(&ChatBlock::Text("partial".to_string()))
        );
    }
}
