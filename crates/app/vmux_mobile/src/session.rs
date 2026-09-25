use std::time::Duration;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::message::{Message, MessageReader};
use bevy_ecs::system::{Commands, NonSendMut};
use dioxus::prelude::*;
use vmux_api::room::{NewChatRequest, RemoteEvent, RemoteSession};
use vmux_chat::room::{Conversation, LiveTurn, Log, Reported};

use crate::remote::{Api, ApiError, next_client_op_id, remote_event_from_shared};
use crate::runtime::RuntimeHandle;
use crate::take_resumed;
use crate::transition;

pub(crate) struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OpenSession>()
            .add_message::<LeaveSession>()
            .add_systems(
                Update,
                (open_sessions, leave_sessions, synchronize_remote_sessions),
            );
    }
}

#[derive(Message)]
pub(crate) struct OpenSession(pub(crate) RemoteSession);

#[derive(Message)]
pub(crate) struct LeaveSession;

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
    pub(crate) fn is_open(&self) -> bool {
        self.current.read().is_some()
    }

    pub(crate) fn sid(&self) -> String {
        match self.current.read().as_ref() {
            Some(session) => session.sid.clone(),
            None => String::new(),
        }
    }
}

pub(crate) async fn stream(
    session: Session,
    runtime: RuntimeHandle,
    api: Api,
    sid: String,
    generation: u64,
) {
    let mut session = session;
    loop {
        if (session.generation)() != generation {
            return;
        }
        if take_resumed() {
            api.reset_transport().await;
        }
        tracing::info!(%sid, "room stream dialling");
        match api.subscribe(&sid).await {
            Ok(mut subscription) => {
                tracing::info!(%sid, "room stream open");
                session.connected.set(true);
                while let Some(event) = subscription.next().await {
                    if (session.generation)() != generation {
                        return;
                    }
                    let Some(event) = remote_event_from_shared(event) else {
                        tracing::warn!("room event not understood");
                        continue;
                    };
                    tracing::info!(kind = event.kind(), "room event");
                    let refresh_now = matches!(&event, RemoteEvent::Approval { .. });
                    runtime
                        .borrow_mut()
                        .app
                        .world_mut()
                        .write_message(Reported(event));
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
        session.connected.set(false);
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub(crate) fn start_chat(
    runtime: RuntimeHandle,
    api: Api,
    mut sessions: Signal<Vec<RemoteSession>>,
    text: String,
    agent_url: Option<String>,
) {
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
            for candidate in &next {
                if !known.contains(&candidate.sid) {
                    created = Some(candidate.clone());
                    break;
                }
            }
            sessions.set(next);
            if let Some(created) = created {
                runtime
                    .borrow_mut()
                    .app
                    .world_mut()
                    .write_message(OpenSession(created));
                return;
            }
        }
    });
}

fn open_sessions(
    mut requests: MessageReader<OpenSession>,
    session: Option<NonSendMut<Session>>,
    mut commands: Commands,
) {
    let Some(mut session) = session else {
        return;
    };
    for request in requests.read() {
        transition::NativeSheet::open();
        session.current.set(Some(request.0.clone()));
        session.connected.set(false);
        let generation = (session.generation)().wrapping_add(1);
        session.generation.set(generation);
        commands.insert_resource(Log {
            room_id: Some(request.0.room_id.clone()),
            ..Log::default()
        });
        commands.insert_resource(LiveTurn::default());
        commands.insert_resource(Conversation {
            status: request.0.status.clone(),
            approval: request.0.approval.clone(),
            session: Some(request.0.clone()),
        });
    }
}

fn leave_sessions(
    mut requests: MessageReader<LeaveSession>,
    session: Option<NonSendMut<Session>>,
    mut commands: Commands,
) {
    let Some(mut session) = session else {
        return;
    };
    for _ in requests.read() {
        let dismissing = transition::NativeSheet::close();
        let generation = (session.generation)().wrapping_add(1);
        session.generation.set(generation);
        session.current.set(None);
        session.connected.set(false);
        commands.insert_resource(Conversation::default());
        commands.insert_resource(Log::default());
        commands.insert_resource(LiveTurn::default());
        dismissing.finish();
    }
}

fn synchronize_remote_sessions(
    mut events: MessageReader<Reported>,
    session: Option<NonSendMut<Session>>,
) {
    let Some(mut session) = session else {
        return;
    };
    for Reported(event) in events.read() {
        if let RemoteEvent::Session { session: updated } = event {
            session.current.set(Some(updated.clone()));
        }
    }
}
