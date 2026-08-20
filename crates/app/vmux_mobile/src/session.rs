//! What the phone is attached to, and how a room's events fold into what it draws.

use crate::api::{Api, ApiError, next_client_op_id, remote_event_from_shared};
use crate::native_transition;
use crate::take_resumed;
use dioxus::prelude::*;
use std::time::Duration;
use vmux_chat::event::ChatSnapshot;
use vmux_service::chat::group_turns_tail;
use vmux_wire::chat::ChatItem;
use vmux_wire::room::{
    AssistantBlock, Message, NewChatRequest, RemoteAgent, RemoteApproval, RemoteEvent,
    RemoteSession, RemoteStatus, RoomEvent, RoomId,
};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum AuthState {
    Loading,
    Paired,
    Unpaired,
}

/// The one conversation the phone has open, and everything that describes it.
///
/// `Copy` and compared by signal identity, mirroring the `Chat` handle the shared chat page keeps
/// its own state in: passing it to the host costs nothing and never defeats memoization.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Session {
    pub(crate) current: Signal<Option<RemoteSession>>,
    pub(crate) room: Signal<MobileRoomProjection>,
    pub(crate) live_delta: Signal<String>,
    pub(crate) status: Signal<RemoteStatus>,
    pub(crate) approval: Signal<Option<RemoteApproval>>,
    /// Whether the event stream is up, as opposed to whether the phone is paired.
    pub(crate) connected: Signal<bool>,
    /// Bumped on every open and leave, so a stream outlived by its session stops writing.
    pub(crate) generation: Signal<u64>,
}

pub(crate) fn use_session() -> Session {
    Session {
        current: use_signal(|| None),
        room: use_signal(MobileRoomProjection::default),
        live_delta: use_signal(String::new),
        status: use_signal(|| RemoteStatus::Idle),
        approval: use_signal(|| None),
        connected: use_signal(|| false),
        generation: use_signal(|| 0),
    }
}

impl Session {
    pub(crate) fn is_open(&self) -> bool {
        self.current.read().is_some()
    }

    pub(crate) fn sid(&self) -> String {
        match self.current.read().as_ref() {
            Some(session) => session.sid.clone(),
            None => String::new(),
        }
    }

    /// Fold the room's event log into the shared transcript model. The desktop gets this from
    /// `group_turns` on the daemon side; the relay does not pre-group yet, so mobile folds locally.
    pub(crate) fn open(&self, api: Api, session: RemoteSession) {
        native_transition::NativeSheet::open();
        let mut handle = *self;
        let sid = session.sid.clone();
        handle.current.set(Some(session.clone()));
        handle.room.set(MobileRoomProjection {
            room_id: Some(session.room_id.clone()),
            ..MobileRoomProjection::default()
        });
        handle.live_delta.set(String::new());
        handle.status.set(session.status.clone());
        handle.approval.set(session.approval.clone());
        handle.connected.set(false);
        let next_generation = (handle.generation)().wrapping_add(1);
        handle.generation.set(next_generation);
        spawn(async move {
            loop {
                if (handle.generation)() != next_generation {
                    return;
                }
                // A connection that survived a suspend is a connection whose socket the OS already
                // closed. Drop it before dialling rather than waiting for a request to stall.
                if take_resumed() {
                    api.reset_transport().await;
                }
                match api.subscribe(&sid).await {
                    Ok(mut subscription) => {
                        handle.connected.set(true);
                        while let Some(event) = subscription.next().await {
                            if (handle.generation)() != next_generation {
                                return;
                            }
                            let Some(event) = remote_event_from_shared(event) else {
                                continue;
                            };
                            let refresh_now = matches!(&event, RemoteEvent::Approval { .. });
                            handle.apply(event);
                            if refresh_now {
                                tokio::task::yield_now().await;
                            }
                        }
                    }
                    // Pairing is gone, or the stream route is — reconnecting brings back neither.
                    Err(ApiError::Unauthorized | ApiError::NotFound) => return,
                    Err(ApiError::Message(_)) => {}
                }
                handle.connected.set(false);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    pub(crate) fn leave(&self) {
        let mut handle = *self;
        let dismissing = native_transition::NativeSheet::close();
        handle.generation.set((handle.generation)().wrapping_add(1));
        handle.current.set(None);
        handle.room.set(MobileRoomProjection::default());
        handle.live_delta.set(String::new());
        handle.status.set(RemoteStatus::Idle);
        handle.approval.set(None);
        handle.connected.set(false);
        dismissing.finish();
    }

    /// Start a chat on the desktop, then open whichever session appears that was not there before.
    pub(crate) fn start_chat(
        &self,
        api: Api,
        mut sessions: Signal<Vec<RemoteSession>>,
        text: String,
        agent_url: Option<String>,
    ) {
        let handle = *self;
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let mut known = std::collections::HashSet::new();
        for session in sessions.read().iter() {
            known.insert(session.sid.clone());
        }
        spawn(async move {
            let request = NewChatRequest {
                client_op_id: next_client_op_id(),
                text,
                agent_url,
            };
            if api.create_chat(&request).await.is_err() {
                return;
            }
            for _ in 0..40 {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let Ok(next) = api.sessions().await else {
                    continue;
                };
                let mut created = None;
                for session in &next {
                    if !known.contains(&session.sid) {
                        created = Some(session.clone());
                        break;
                    }
                }
                sessions.set(next);
                if let Some(created) = created {
                    handle.open(api, created);
                    return;
                }
            }
        });
    }

    pub(crate) fn apply(&self, event: RemoteEvent) {
        let mut current = self.current;
        let mut room = self.room;
        let mut live_delta = self.live_delta;
        let mut status = self.status;
        let mut approval = self.approval;
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

    /// Describe the open conversation the way the shared chat page expects to be told about it.
    ///
    /// The desktop builds this from the daemon's own session state. The phone has the same facts
    /// spread across a session row and a folded room log, so this is where the two meet — every
    /// field the relay cannot answer is left at its default, which each of the page's features
    /// reads as "absent" and declines to render.
    pub(crate) fn snapshot(&self, agents: &[RemoteAgent]) -> ChatSnapshot {
        let current = self.current.read();
        let Some(session) = current.as_ref() else {
            return ChatSnapshot::default();
        };
        let streaming = matches!(session.status, RemoteStatus::Streaming);
        let items = self
            .room
            .read()
            .chat_items(&self.live_delta.read(), streaming);
        let total = items.len() as u32;
        let messages_json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string());
        // The session says which agent it is by name; the icon lives on the agent list the phone
        // already fetches, so no extra round trip and nothing new on the wire.
        let mut agent_icon = String::new();
        let mut agent_segment = "";
        for agent in agents {
            if agent.name == session.name {
                agent_icon = agent.icon.clone();
                agent_segment = agent.id.as_str();
                break;
            }
        }
        let approval = self.approval.read();
        let (approval_call_id, approval_name, approval_args_json) = match approval.as_ref() {
            Some(pending) => (
                pending.call_id.clone(),
                pending.name.clone(),
                pending.args_json.clone(),
            ),
            None => (String::new(), String::new(), String::new()),
        };
        let error = match &*self.status.read() {
            RemoteStatus::Errored(message) => message.clone(),
            _ => String::new(),
        };
        ChatSnapshot {
            messages_json,
            messages_start: 0,
            messages_total: total,
            status: self.status.read().page_status().to_string(),
            error,
            approval_call_id,
            approval_name,
            approval_args_json,
            agent_name: session.name.clone(),
            conversation_title: session.name.clone(),
            agent_icon,
            // Derived rather than sent: the accent is a pure function of the agent's url segment
            // and already lives in the shared crate, so the phone reaches the same colour the
            // desktop paints without the wire carrying a theme.
            accent_color: vmux_wire::avatar::agent_color(agent_segment),
            ..ChatSnapshot::default()
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MobileRoomProjection {
    room_id: Option<RoomId>,
    through_seq: u64,
    events: Vec<RoomEvent>,
}

impl MobileRoomProjection {
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

/// The words the shared chat page matches a run's state on.
///
/// `RemoteStatus` names the same four states differently, and the page's status is a bare string,
/// so the translation has to happen somewhere. Here, next to the snapshot that carries it.
trait PageStatus {
    fn page_status(&self) -> &'static str;
}

impl PageStatus for RemoteStatus {
    fn page_status(&self) -> &'static str {
        match self {
            RemoteStatus::Streaming => "streaming",
            RemoteStatus::Errored(_) => "errored",
            RemoteStatus::Interrupted => "interrupted",
            RemoteStatus::Idle => "idle",
        }
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

    /// The page reads its run state off a bare string, so a name that drifts from what it matches
    /// on is a silent failure: the composer would show idle mid-turn and never offer Stop.
    #[test]
    fn every_remote_status_names_a_state_the_shared_page_knows() {
        assert_eq!(RemoteStatus::Idle.page_status(), "idle");
        assert_eq!(RemoteStatus::Streaming.page_status(), "streaming");
        assert_eq!(RemoteStatus::Interrupted.page_status(), "interrupted");
        assert_eq!(
            RemoteStatus::Errored("boom".to_string()).page_status(),
            "errored"
        );
    }
}
