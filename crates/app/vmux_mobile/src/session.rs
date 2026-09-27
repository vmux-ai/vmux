use std::collections::HashSet;
use std::time::Duration;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::message::{Message, MessageReader, MessageWriter};
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{Commands, Query};
use bevy_tasks::{IoTaskPool, Task, futures_lite::future};
use dioxus::prelude::*;
use vmux_api::room::{NewChatRequest, RemoteEvent, RemoteSession};
use vmux_chat::room::{Conversation, LiveTurn, Log, Reported};

use crate::pairing::ConnectionState;
use crate::remote::{Api, ApiError, next_client_op_id, remote_event_from_shared};
use crate::runtime::RuntimeHandle;
use crate::transition;

pub(crate) struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(SessionState::default());
        app.add_message::<OpenSession>()
            .add_message::<LeaveSession>()
            .add_message::<RestartSession>()
            .add_message::<StartChatRequest>()
            .add_systems(
                Update,
                (
                    start_chats,
                    poll_started_chats,
                    open_sessions,
                    leave_sessions,
                    restart_session_streams,
                    poll_session_streams,
                    synchronize_remote_sessions,
                )
                    .chain(),
            );
    }
}

#[derive(Message)]
pub(crate) struct OpenSession(pub(crate) RemoteSession);

#[derive(Message)]
pub(crate) struct LeaveSession;

#[derive(Message)]
pub(crate) struct RestartSession;

#[derive(Message)]
pub(crate) struct StartChatRequest {
    pub(crate) text: String,
    pub(crate) agent_url: Option<String>,
}

#[derive(Clone, Default, PartialEq)]
struct SessionView {
    current: Option<RemoteSession>,
    connected: bool,
    generation: u64,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Session {
    view: Signal<SessionView>,
}

pub(crate) fn use_session(runtime: RuntimeHandle) -> Session {
    let mut view = use_signal(SessionView::default);
    use_future(move || {
        let runtime = runtime.clone();
        async move {
            loop {
                let projected = {
                    if let Ok(mut runtime) = runtime.try_borrow_mut() {
                        let world = runtime.app.world_mut();
                        let mut query = world.query::<&SessionState>();
                        query.single(world).ok().map(|state| state.view.clone())
                    } else {
                        None
                    }
                };
                if let Some(next) = projected
                    && *view.peek() != next
                {
                    view.set(next);
                }
                vmux_ui::platform::sleep_ms(50).await;
            }
        }
    });
    Session { view }
}

impl Session {
    pub(crate) fn is_open(&self) -> bool {
        self.view.read().current.is_some()
    }

    pub(crate) fn sid(&self) -> String {
        self.view
            .read()
            .current
            .as_ref()
            .map(|session| session.sid.clone())
            .unwrap_or_default()
    }
}

#[derive(Component, Default)]
struct SessionState {
    view: SessionView,
}

#[derive(Component)]
struct SessionStream {
    generation: u64,
    receiver: crossbeam_channel::Receiver<SessionStreamOutput>,
    task: Task<()>,
}

enum SessionStreamOutput {
    Connected(bool),
    Event(RemoteEvent),
}

impl SessionStream {
    fn spawn(api: Api, sid: String, generation: u64, reset: bool) -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let task = IoTaskPool::get().spawn(async move {
            if reset {
                api.reset_transport().await;
            }
            loop {
                tracing::info!(%sid, "room stream dialling");
                match api.subscribe(&sid).await {
                    Ok(mut subscription) => {
                        tracing::info!(%sid, "room stream open");
                        if sender.send(SessionStreamOutput::Connected(true)).is_err() {
                            return;
                        }
                        while let Some(event) = subscription.next().await {
                            let Some(event) = remote_event_from_shared(event) else {
                                tracing::warn!("room event not understood");
                                continue;
                            };
                            tracing::info!(kind = event.kind(), "room event");
                            if sender.send(SessionStreamOutput::Event(event)).is_err() {
                                return;
                            }
                        }
                        tracing::warn!(%sid, "room stream ended");
                    }
                    Err(ApiError::Unauthorized | ApiError::NotFound) => {
                        tracing::warn!(%sid, "room stream refused for good");
                        let _ = sender.send(SessionStreamOutput::Connected(false));
                        return;
                    }
                    Err(ApiError::Message(error)) => {
                        tracing::warn!(%sid, %error, "room stream failed; retrying");
                    }
                }
                if sender.send(SessionStreamOutput::Connected(false)).is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
        Self {
            generation,
            receiver,
            task,
        }
    }
}

#[derive(Component)]
struct StartChatOperation {
    task: Task<Option<(Vec<RemoteSession>, RemoteSession)>>,
}

fn start_chats(
    mut requests: MessageReader<StartChatRequest>,
    connections: Query<&ConnectionState>,
    mut commands: Commands,
) {
    let Ok(connection) = connections.single() else {
        return;
    };
    let Some(api) = connection.api() else {
        return;
    };
    for request in requests.read() {
        let text = request.text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        let known = connection
            .sessions()
            .iter()
            .map(|session| session.sid.clone())
            .collect::<HashSet<_>>();
        let agent_url = request.agent_url.clone();
        let api = api.clone();
        let task = IoTaskPool::get().spawn(async move {
            let request = NewChatRequest {
                client_op_id: next_client_op_id(),
                text,
                agent_url,
            };
            if api.create_chat(&request).await.is_err() {
                return None;
            }
            for _ in 0..40 {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let Ok(sessions) = api.sessions().await else {
                    continue;
                };
                let created = sessions
                    .iter()
                    .find(|candidate| !known.contains(&candidate.sid))
                    .cloned();
                if let Some(created) = created {
                    return Some((sessions, created));
                }
            }
            None
        });
        commands.spawn(StartChatOperation { task });
    }
}

fn poll_started_chats(
    mut operations: Query<(Entity, &mut StartChatOperation)>,
    mut connections: Query<&mut ConnectionState>,
    mut openings: MessageWriter<OpenSession>,
    mut commands: Commands,
) {
    for (entity, mut operation) in &mut operations {
        let Some(result) = future::block_on(future::poll_once(&mut operation.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        let Some((sessions, created)) = result else {
            continue;
        };
        if let Ok(mut connection) = connections.single_mut() {
            connection.set_sessions(sessions);
        }
        openings.write(OpenSession(created));
    }
}

fn open_sessions(
    mut requests: MessageReader<OpenSession>,
    mut sessions: Query<(Entity, &mut SessionState)>,
    connections: Query<&ConnectionState>,
    mut commands: Commands,
) {
    let Ok((entity, mut state)) = sessions.single_mut() else {
        return;
    };
    let Ok(connection) = connections.single() else {
        return;
    };
    let Some(api) = connection.api() else {
        return;
    };
    for request in requests.read() {
        transition::NativeSheet::open();
        state.view.current = Some(request.0.clone());
        state.view.connected = false;
        state.view.generation = state.view.generation.wrapping_add(1);
        commands.entity(entity).insert(SessionStream::spawn(
            api.clone(),
            request.0.sid.clone(),
            state.view.generation,
            false,
        ));
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
    mut sessions: Query<(Entity, &mut SessionState)>,
    mut commands: Commands,
) {
    let Ok((entity, mut state)) = sessions.single_mut() else {
        return;
    };
    for _ in requests.read() {
        let dismissing = transition::NativeSheet::close();
        state.view.generation = state.view.generation.wrapping_add(1);
        state.view.current = None;
        state.view.connected = false;
        commands.entity(entity).remove::<SessionStream>();
        commands.insert_resource(Conversation::default());
        commands.insert_resource(Log::default());
        commands.insert_resource(LiveTurn::default());
        dismissing.finish();
    }
}

fn restart_session_streams(
    mut requests: MessageReader<RestartSession>,
    mut sessions: Query<(Entity, &mut SessionState)>,
    connections: Query<&ConnectionState>,
    mut commands: Commands,
) {
    if requests.read().next().is_none() {
        return;
    }
    let Ok((entity, mut state)) = sessions.single_mut() else {
        return;
    };
    let Some(current) = state.view.current.as_ref() else {
        return;
    };
    let Ok(connection) = connections.single() else {
        return;
    };
    let Some(api) = connection.api() else {
        return;
    };
    let sid = current.sid.clone();
    state.view.connected = false;
    state.view.generation = state.view.generation.wrapping_add(1);
    commands.entity(entity).insert(SessionStream::spawn(
        api,
        sid,
        state.view.generation,
        true,
    ));
}

fn poll_session_streams(
    mut sessions: Query<(Entity, &mut SessionState, &mut SessionStream)>,
    mut reported: MessageWriter<Reported>,
    mut commands: Commands,
) {
    for (entity, mut state, mut stream) in &mut sessions {
        if stream.generation != state.view.generation {
            commands.entity(entity).remove::<SessionStream>();
            continue;
        }
        while let Ok(output) = stream.receiver.try_recv() {
            match output {
                SessionStreamOutput::Connected(connected) => state.view.connected = connected,
                SessionStreamOutput::Event(event) => reported.write(Reported(event)),
            }
        }
        if future::block_on(future::poll_once(&mut stream.task)).is_some() {
            state.view.connected = false;
            commands.entity(entity).remove::<SessionStream>();
        }
    }
}

fn synchronize_remote_sessions(
    mut events: MessageReader<Reported>,
    mut sessions: Query<&mut SessionState>,
) {
    let Ok(mut state) = sessions.single_mut() else {
        return;
    };
    for Reported(event) in events.read() {
        if let RemoteEvent::Session { session } = event {
            state.view.current = Some(session.clone());
        }
    }
}
