use crate::nav::{Nav, Screen};
use crate::remote::{Api, ApiError, next_client_op_id, remote_event_from_shared};
use crate::runtime::World;
use crate::take_resumed;
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

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Session {
    pub(crate) current: Signal<Option<RemoteSession>>,
    pub(crate) connected: Signal<bool>,
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
    pub(crate) fn sid(&self) -> String {
        match self.current.read().as_ref() {
            Some(session) => session.sid.clone(),
            None => String::new(),
        }
    }

    pub(crate) fn open(&self, session: RemoteSession) {
        let mut handle = *self;
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
        handle.generation.set((handle.generation)().wrapping_add(1));
    }

    pub(crate) fn attach(&self, nav: Nav, session: RemoteSession) {
        let screen = Screen::Chat {
            sid: Some(session.sid.clone()),
            title: session.title.clone(),
        };
        self.open(session);
        nav.open(screen);
    }

    pub(crate) async fn stream(self, api: Api, sid: String, generation: u64) {
        let mut handle = self;
        loop {
            if (handle.generation)() != generation {
                return;
            }
            if take_resumed() {
                api.reset_transport().await;
            }
            tracing::info!(%sid, "room stream dialling");
            match api.subscribe(&sid).await {
                Ok(mut subscription) => {
                    tracing::info!(%sid, "room stream open");
                    handle.connected.set(true);
                    while let Some(event) = subscription.next().await {
                        if (handle.generation)() != generation {
                            return;
                        }
                        let Some(event) = remote_event_from_shared(event) else {
                            tracing::warn!("room event not understood");
                            continue;
                        };
                        tracing::info!(kind = event.kind(), "room event");
                        let refresh_now = matches!(&event, RemoteEvent::Approval { .. });
                        handle.apply(event);
                        if refresh_now {
                            tokio::task::yield_now().await;
                        }
                    }
                    tracing::warn!(%sid, "room stream ended");
                }
                Err(ApiError::Unauthorized | ApiError::NotFound) => {
                    tracing::warn!(%sid, "room stream refused for good");
                    return;
                }
                Err(ApiError::Message(error)) => {
                    tracing::warn!(%sid, %error, "room stream failed; retrying");
                }
            }
            handle.connected.set(false);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    pub(crate) fn leave(&self) {
        let mut handle = *self;
        handle.generation.set((handle.generation)().wrapping_add(1));
        handle.current.set(None);
        World::with(|world| {
            world.insert(Conversation::default());
            world.insert(Log::default());
            world.insert(LiveTurn::default());
        });
        handle.connected.set(false);
    }

    pub(crate) fn start_chat(
        &self,
        api: Api,
        mut sessions: Signal<Vec<RemoteSession>>,
        nav: Nav,
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
                    handle.attach(nav, created);
                    return;
                }
            }
        });
    }

    pub(crate) fn apply(&self, event: RemoteEvent) {
        if let RemoteEvent::Session { session } = &event {
            let mut current = self.current;
            current.set(Some(session.clone()));
        }
        World::with(|world| world.send(Reported(event)));
    }
}
