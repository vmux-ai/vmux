//! What the phone is attached to, and the stream that keeps it current.
//!
//! What the conversation *is* lives in the world — [`vmux_chat::room`] folds the link's events into
//! the snapshot the page draws. This is the shell's side: opening and leaving a conversation, and
//! the subscription that survives a suspend.

use crate::api::{Api, ApiError, next_client_op_id, remote_event_from_shared};
use crate::native_transition;
use crate::take_resumed;
use crate::world::World;
use dioxus::prelude::*;
use std::time::Duration;
use vmux_chat::room::{Conversation, LiveTurn, Log, Reported};
use vmux_wire::room::{NewChatRequest, RemoteEvent, RemoteSession};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum AuthState {
    Loading,
    Paired,
    Unpaired,
}

/// The one conversation the phone has open.
///
/// What the conversation *contains* is not here — the transcript, the run's state and the pending
/// approval are folded in the world by [`vmux_chat::room`], which is also what draws them. This is
/// the half the shell itself branches on: whether anything is open, and which stream is allowed to
/// write to it.
///
/// `Copy` and compared by signal identity, mirroring the `Chat` handle the shared chat page keeps
/// its own state in: passing it to the host costs nothing and never defeats memoization.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Session {
    pub(crate) current: Signal<Option<RemoteSession>>,
    /// Whether the event stream is up, as opposed to whether the phone is paired.
    pub(crate) connected: Signal<bool>,
    /// Bumped on every open and leave, so a stream outlived by its session stops writing.
    pub(crate) generation: Signal<u64>,
}

pub(crate) fn use_session() -> Session {
    Session {
        current: use_signal(|| None),
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

    /// Attach to `session` and start replaying it, replacing whatever was open.
    pub(crate) fn open(&self, api: Api, session: RemoteSession) {
        native_transition::NativeSheet::open();
        let mut handle = *self;
        let sid = session.sid.clone();
        handle.current.set(Some(session.clone()));
        World::with(|world| {
            world.insert(Log {
                room_id: Some(session.room_id.clone()),
                ..Log::default()
            });
            world.insert(LiveTurn::default());
            world.insert(Conversation {
                status: session.status.clone(),
                approval: session.approval.clone(),
                session: Some(session),
            });
        });
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
        World::with(|world| {
            world.insert(Conversation::default());
            world.insert(Log::default());
            world.insert(LiveTurn::default());
        });
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

    /// Hand what the link said to the world, and keep the shell's own copy of the session row.
    pub(crate) fn apply(&self, event: RemoteEvent) {
        if let RemoteEvent::Session { session } = &event {
            let mut current = self.current;
            current.set(Some(session.clone()));
        }
        World::with(|world| world.send(Reported(event)));
    }
}
