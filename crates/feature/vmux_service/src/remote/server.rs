use std::sync::Arc;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use tokio::sync::{mpsc, watch};
use vmux_ecs::service::{AbortTask, Executor, Register, Remote};
use vmux_transport::DeviceId;
use vmux_transport::service::RemoteDriver;

use crate::RelayToken;
use crate::remote::authorization::RemoteAuthorizations;
use crate::remote::exposure_driver::RemoteExposure;
use crate::remote::quic::{RemoteState, SessionEnded, SessionPublisher, SessionStarted};

pub(crate) struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            super::authorization::RemoteAuthorizationPlugin,
            super::client_operation::ClientOperationPlugin,
        ))
        .add_systems(
            Update,
            (
                start.after(Register),
                ApplyDeferred,
                refresh,
                dial,
                connect,
                disconnect,
            )
                .chain(),
        );
    }
}

#[derive(Component)]
struct RemoteLiveness(watch::Sender<bool>);

#[derive(Component, Clone)]
struct RelayCredential(Arc<str>);

#[derive(Component, Clone)]
struct RemoteHandler(Arc<dyn RemoteDriver>);

#[derive(Component, Clone)]
struct RemoteSessionPublisher(SessionPublisher);

#[derive(Component)]
struct SessionStartedInbox(mpsc::UnboundedReceiver<SessionStarted>);

#[derive(Component)]
struct SessionEndedInbox(mpsc::UnboundedReceiver<SessionEnded>);

#[derive(Component)]
struct Dialer;

#[derive(Component, Clone, Copy)]
struct RemoteSession;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct SessionId(usize);

#[derive(Component, Clone)]
struct SessionDevice(DeviceId);

type StartingRemote<'a> = (Entity, &'a mut Remote);
type StartingRemoteFilter = (With<RemoteAuthorizations>, Without<RemoteLiveness>);

#[derive(SystemParam)]
struct StartingRemotes<'w, 's> {
    values: Query<'w, 's, StartingRemote<'static>, StartingRemoteFilter>,
}

type RemoteRuntime<'a> = (
    Entity,
    &'a Executor,
    &'a RelayCredential,
    &'a RemoteAuthorizations,
    &'a RemoteHandler,
    &'a RemoteLiveness,
    &'a RemoteSessionPublisher,
    &'a RemoteExposure,
    Option<&'a Children>,
);

#[derive(SystemParam)]
struct RemoteRuntimes<'w, 's> {
    values: Query<'w, 's, RemoteRuntime<'static>>,
    tasks: Query<'w, 's, &'static AbortTask, With<Dialer>>,
}

fn start(mut servers: StartingRemotes, mut commands: Commands) {
    for (entity, mut remote) in &mut servers.values {
        let Some(remote) = remote.0.take() else {
            continue;
        };
        let relay_token = match RelayToken::ensure() {
            Ok(token) => token,
            Err(error) => {
                tracing::error!(%error, "remote: token setup failed");
                commands.entity(entity).remove::<Remote>();
                continue;
            }
        };
        let exposure = RemoteExposure::current();
        let (liveness, _) = watch::channel(exposure.0);
        let (sessions, started, ended) = SessionPublisher::channel();
        commands.entity(entity).remove::<Remote>().insert((
            RelayCredential(Arc::from(relay_token.as_str())),
            RemoteHandler(remote),
            RemoteLiveness(liveness),
            RemoteSessionPublisher(sessions),
            SessionStartedInbox(started),
            SessionEndedInbox(ended),
            exposure,
        ));
    }
}

fn refresh(mut runtimes: Query<(&RemoteLiveness, &mut RemoteExposure)>) {
    for (liveness, mut exposure) in &mut runtimes {
        let current = RemoteExposure::current();
        if *exposure == current {
            continue;
        }
        *exposure = current;
        liveness.0.send_replace(current.0);
        tracing::info!(enabled = current.0, "remote quic: exposure changed");
    }
}

fn dial(runtimes: RemoteRuntimes, mut commands: Commands) {
    for (
        entity,
        executor,
        credential,
        authorizations,
        remote,
        liveness,
        sessions,
        exposure,
        children,
    ) in &runtimes.values
    {
        if !exposure.0 {
            if let Some(children) = children {
                let mut stopped = false;
                for child in children.iter() {
                    if runtimes.tasks.contains(child) {
                        commands.entity(child).despawn();
                        stopped = true;
                    }
                }
                if stopped {
                    tracing::info!("remote quic: the relay dialer stopped");
                }
            }
            continue;
        }
        if let Some(children) = children {
            let mut active = false;
            for child in children.iter() {
                let Ok(task) = runtimes.tasks.get(child) else {
                    continue;
                };
                if task.0.is_finished() {
                    commands.entity(child).despawn();
                } else {
                    active = true;
                }
            }
            if active {
                continue;
            }
        }
        tracing::info!("remote quic: dialing the relay");
        let state = RemoteState {
            relay_token: credential.0.clone(),
            authorizations: authorizations.clone(),
            remote: remote.0.clone(),
            sessions: sessions.0.clone(),
        };
        let task = executor.0.spawn(state.dial(liveness.0.subscribe()));
        commands.spawn((
            Name::new("remote dialer"),
            Dialer,
            ChildOf(entity),
            AbortTask(task),
        ));
    }
}

fn connect(
    mut servers: Query<(Entity, &mut SessionStartedInbox)>,
    sessions: Query<(&SessionId, &ChildOf), With<RemoteSession>>,
    mut commands: Commands,
) {
    for (server, mut inbox) in &mut servers {
        while let Ok(started) = inbox.0.try_recv() {
            if sessions
                .iter()
                .any(|(id, owner)| id.0 == started.id && owner.parent() == server)
            {
                continue;
            }
            commands.spawn((
                Name::new(format!("remote session: {}", started.device.as_str())),
                RemoteSession,
                ChildOf(server),
                SessionId(started.id),
                SessionDevice(started.device),
            ));
        }
    }
}

fn disconnect(
    mut servers: Query<(Entity, &mut SessionEndedInbox)>,
    sessions: Query<(Entity, &SessionId, &ChildOf, &SessionDevice), With<RemoteSession>>,
    mut commands: Commands,
) {
    for (server, mut inbox) in &mut servers {
        while let Ok(ended) = inbox.0.try_recv() {
            for (entity, id, owner, device) in &sessions {
                if id.0 == ended.id && owner.parent() == server {
                    tracing::info!(device = %device.0.as_str(), "remote quic: session ended");
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}
