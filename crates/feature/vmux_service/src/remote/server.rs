use std::sync::Arc;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use tokio::sync::{mpsc, watch};
use vmux_api::remote::{Live, Observation as RemoteObservation, RemotePhase};
use vmux_ecs::service::{Executor, Register, Remote};
use vmux_ecs::{Revision, TokioTask};
use vmux_feature::flag::Decision;
use vmux_transport::service::RemoteDriver;

use crate::AccountRelayCredential;
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
                read,
                refresh,
                connect,
                disconnect,
                retire,
                dial,
                dispatch,
                observe,
            )
                .chain()
                .after(vmux_api::remote::LiveDecisionSet),
        );
    }
}

#[derive(Component)]
struct RemoteLiveness(watch::Sender<bool>);

#[derive(Component, Clone)]
#[allow(unknown_lints, vmux_ecs_ownership)]
struct RemoteHandler(Arc<dyn RemoteDriver>);

#[derive(Component, Clone)]
struct RemoteSessionPublisher(SessionPublisher);

#[derive(Component)]
struct SessionStartedInbox(mpsc::UnboundedReceiver<SessionStarted>);

#[derive(Component)]
struct SessionEndedInbox(mpsc::UnboundedReceiver<SessionEnded>);

#[derive(Component)]
struct Dialer;

#[derive(Component, Clone)]
#[component(immutable)]
struct DialLiveness(watch::Receiver<bool>);

#[derive(Component)]
struct Stopping;

#[derive(Component)]
struct Requested(bool);

#[derive(Component)]
#[require(RemotePhase)]
struct Observation;

#[derive(Component)]
struct Epoch(uuid::Uuid);

#[derive(Component)]
pub(crate) struct ObservationSource(watch::Sender<RemoteObservation>);

impl ObservationSource {
    pub(crate) fn subscribe(&self) -> watch::Receiver<RemoteObservation> {
        self.0.subscribe()
    }
}

#[derive(Component, Clone, Copy)]
struct RemoteSession;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct SessionId(usize);

type StartingRemote<'a> = (Entity, &'a mut Remote);
type StartingRemoteFilter = (With<RemoteAuthorizations>, Without<RemoteLiveness>);

#[derive(SystemParam)]
struct StartingRemotes<'w, 's> {
    values: Query<'w, 's, StartingRemote<'static>, StartingRemoteFilter>,
}

type RemoteRuntimes<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static AccountRelayCredential,
        &'static RemoteAuthorizations,
        &'static RemoteHandler,
        &'static RemoteSessionPublisher,
        &'static RemoteExposure,
    ),
>;

type OwnedDialers<'w, 's> =
    Query<'w, 's, (Entity, &'static TokioTask, &'static ChildOf, Has<Stopping>), With<Dialer>>;

fn retire(
    tasks: OwnedDialers,
    runtimes: Query<&RemoteExposure>,
    sessions: Query<&ChildOf, With<RemoteSession>>,
    mut commands: Commands,
) {
    for (entity, task, owner, stopping) in &tasks {
        let Ok(exposure) = runtimes.get(owner.parent()) else {
            continue;
        };
        if !exposure.0 && !stopping {
            commands.entity(entity).insert(Stopping);
        }
        if task.is_finished()
            && !sessions
                .iter()
                .any(|session| session.parent() == owner.parent())
        {
            commands.entity(entity).despawn();
        }
    }
}

fn start(mut servers: StartingRemotes, mut commands: Commands) {
    for (entity, mut remote) in &mut servers.values {
        let Some(remote) = remote.0.take() else {
            continue;
        };
        let legacy = crate::RemotePaths::current();
        let _ = std::fs::remove_file(legacy.relay_token());
        let requested = RemoteExposure::current();
        let exposure = RemoteExposure(false);
        let (liveness, _) = watch::channel(false);
        let (sessions, started, ended) = SessionPublisher::channel();
        let epoch = uuid::Uuid::new_v4();
        let (observation, _) = watch::channel(RemoteObservation {
            epoch,
            revision: 0,
            phase: RemotePhase::Disabled,
        });
        commands.spawn((
            Name::new("Live runtime observation"),
            Observation,
            Epoch(epoch),
            Revision(0),
            RemotePhase::Disabled,
            ChildOf(entity),
        ));
        commands.entity(entity).remove::<Remote>().insert((
            RemoteHandler(remote),
            RemoteLiveness(liveness),
            RemoteSessionPublisher(sessions),
            SessionStartedInbox(started),
            SessionEndedInbox(ended),
            exposure,
            Requested(requested.0),
            ObservationSource(observation),
        ));
        if let Some(credential) = AccountRelayCredential::read_current() {
            commands.entity(entity).insert(credential);
        }
    }
}

fn read(
    mut requested: Query<(Entity, &mut Requested, Option<&AccountRelayCredential>)>,
    mut commands: Commands,
) {
    let exposed = RemoteExposure::current().0;
    let current = AccountRelayCredential::read_current();
    for (entity, mut requested, previous) in &mut requested {
        requested.0 = exposed;
        match (&current, previous) {
            (Some(next), Some(existing)) if existing == next => {}
            (Some(next), _) => {
                commands.entity(entity).insert(next.clone());
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<AccountRelayCredential>();
            }
            (None, None) => {}
        }
    }
}

fn refresh(
    mut runtimes: Query<(
        &RemoteLiveness,
        &Requested,
        Has<AccountRelayCredential>,
        &mut RemoteExposure,
    )>,
    decision: Query<&Decision<bool>, With<Live>>,
) {
    let available = decision.single().is_ok_and(|decision| decision.value);
    for (liveness, requested, has_credential, mut exposure) in &mut runtimes {
        let current = RemoteExposure(requested.0 && available && has_credential);
        if *exposure == current {
            continue;
        }
        *exposure = current;
        if !current.0 {
            liveness.0.send_replace(false);
        }
        tracing::info!(enabled = current.0, "remote quic: exposure changed");
    }
}

fn dial(
    runtimes: RemoteRuntimes,
    active: Query<&ChildOf, With<Dialer>>,
    sessions: Query<&ChildOf, With<RemoteSession>>,
    decision: Query<&Decision<bool>, With<Live>>,
    mut commands: Commands,
) {
    for (entity, credential, authorizations, remote, publisher, exposure) in &runtimes {
        if !exposure.0
            || active.iter().any(|owner| owner.parent() == entity)
            || sessions.iter().any(|owner| owner.parent() == entity)
        {
            continue;
        }
        let Ok(decision) = decision.single() else {
            continue;
        };
        if !decision.value {
            continue;
        }
        let state = RemoteState {
            device_id: credential.device_id().clone(),
            relay_token: Arc::from(credential.assertion()),
            authorizations: authorizations.clone(),
            remote: remote.0.clone(),
            sessions: publisher.0.clone(),
        };
        let (liveness, receiver) = watch::channel(true);
        commands.entity(entity).insert(RemoteLiveness(liveness));
        commands.spawn((
            Name::new("remote dialer"),
            Dialer,
            ChildOf(entity),
            state,
            DialLiveness(receiver),
            decision.clone(),
        ));
    }
}

fn dispatch(
    requests: Query<(Entity, &ChildOf, &RemoteState, &DialLiveness), Without<TokioTask>>,
    runtimes: Query<(&Executor, &RemoteExposure)>,
    decision: Query<&Decision<bool>, With<Live>>,
    mut commands: Commands,
) {
    let available = decision.single().is_ok_and(|decision| decision.value);
    for (entity, owner, state, liveness) in &requests {
        let Ok((executor, exposure)) = runtimes.get(owner.parent()) else {
            continue;
        };
        if !available || !exposure.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let task = executor.0.spawn(state.clone().dial(liveness.0.clone()));
        commands.entity(entity).insert(TokioTask::new(task));
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
                Name::new(started.device.as_str().to_string()),
                RemoteSession,
                ChildOf(server),
                SessionId(started.id),
            ));
        }
    }
}

fn disconnect(
    mut servers: Query<(Entity, &mut SessionEndedInbox)>,
    sessions: Query<(Entity, &SessionId, &ChildOf, &Name), With<RemoteSession>>,
    mut commands: Commands,
) {
    for (server, mut inbox) in &mut servers {
        while let Ok(ended) = inbox.0.try_recv() {
            for (entity, id, owner, device) in &sessions {
                if id.0 == ended.id && owner.parent() == server {
                    tracing::info!(device = %device.as_str(), "remote quic: session ended");
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

fn observe(
    mut observations: Query<(&ChildOf, &Epoch, &mut Revision, &mut RemotePhase), With<Observation>>,
    runtimes: Query<(&RemoteExposure, &RemoteSessionPublisher, &ObservationSource)>,
    dialers: Query<&ChildOf, With<Dialer>>,
    sessions: Query<&ChildOf, With<RemoteSession>>,
) {
    for (parent, epoch, mut revision, mut current) in &mut observations {
        let runtime = parent.parent();
        let Ok((exposure, publisher, source)) = runtimes.get(runtime) else {
            continue;
        };
        let phase = if exposure.0 {
            if publisher.0.registered() {
                RemotePhase::Enabled
            } else {
                RemotePhase::Starting
            }
        } else if dialers.iter().any(|parent| parent.parent() == runtime)
            || sessions.iter().any(|parent| parent.parent() == runtime)
        {
            RemotePhase::Stopping
        } else {
            RemotePhase::Disabled
        };
        if *current == phase {
            continue;
        }
        *current = phase;
        revision.0 = revision.0.saturating_add(1);
        source.0.send_replace(RemoteObservation {
            epoch: epoch.0,
            revision: revision.0,
            phase,
        });
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::host::server::{
        Daemon, ExitSignal, ServiceDaemonPlugin, SocketListener, TrustedDesktop,
    };
    use std::time::{Duration, Instant};
    use vmux_api::protocol::{ClientMessage, ServiceMessage};
    use vmux_transport::service::ServiceConnection;
    use vmux_transport::service_peer::CodeRequirement;

    struct Fixture {
        app: App,
        root: Entity,
        connection: Arc<ServiceConnection>,
        exited: mpsc::Receiver<()>,
        _directory: tempfile::TempDir,
    }

    impl Fixture {
        async fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let socket = directory.path().join("service.sock");
            let listener = tokio::net::UnixListener::bind(&socket).unwrap();
            let binary = std::env::current_exe().unwrap();
            let (wake, _wakes) = mpsc::unbounded_channel();
            let (exit, exited) = mpsc::channel(1);
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ServiceDaemonPlugin));
            let root = app
                .world_mut()
                .spawn((
                    Daemon,
                    SocketListener(Some(listener)),
                    ExitSignal(exit),
                    Executor(tokio::runtime::Handle::current()),
                    vmux_ecs::service::Wake(wake),
                    TrustedDesktop::for_binary(&binary).unwrap(),
                ))
                .id();
            app.update();
            app.world_mut()
                .entity_mut(root)
                .insert(Remote(Some(Arc::new(
                    crate::remote::quic::live::ClosedRemote,
                ))));
            app.update();
            assert!(
                app.world().get::<ObservationSource>(root).is_some(),
                "fixture must register a real remote runtime before connecting"
            );
            let requirement = CodeRequirement::for_binary(&binary).unwrap();
            let connection = ServiceConnection::connect_checked(&socket, &requirement, unsafe {
                libc::geteuid()
            })
            .await
            .unwrap();
            Self {
                app,
                root,
                connection: Arc::new(connection),
                exited,
                _directory: directory,
            }
        }

        async fn receive(&mut self) -> ServiceMessage {
            let connection = self.connection.clone();
            let received = tokio::spawn(async move { connection.recv().await.unwrap().unwrap() });
            let until = Instant::now() + Duration::from_secs(5);
            while !received.is_finished() {
                self.app.update();
                assert!(
                    Instant::now() < until,
                    "native observation was not delivered"
                );
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            received.await.unwrap()
        }

        async fn stop(mut self) {
            self.connection
                .send(&ClientMessage::Shutdown)
                .await
                .unwrap();
            loop {
                if matches!(self.receive().await, ServiceMessage::ProcessList { .. }) {
                    break;
                }
            }
            self.exited.recv().await.unwrap();
            self.app.update();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_observed_stop_waits_for_owned_io_and_preserves_unrelated_local_work() {
        let mut fixture = Fixture::new().await;
        let local = fixture
            .app
            .world_mut()
            .spawn((Name::new("Unrelated local Session"), ChildOf(fixture.root)))
            .id();
        let (finish, waiting) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = waiting.await;
        });
        let dialer = fixture
            .app
            .world_mut()
            .spawn((
                Name::new("Accepted remote IO"),
                Dialer,
                ChildOf(fixture.root),
                TokioTask::new(task),
            ))
            .id();
        fixture.app.update();
        fixture
            .connection
            .send(&ClientMessage::SubscribeRemoteObservations)
            .await
            .unwrap();
        let ServiceMessage::RemoteObserved(stopping) = fixture.receive().await else {
            panic!("native stopping observation required")
        };
        assert_eq!(stopping.phase, RemotePhase::Stopping);
        assert!(
            !fixture
                .app
                .world()
                .get::<TokioTask>(dialer)
                .unwrap()
                .is_finished()
        );
        finish.send(()).unwrap();
        let ServiceMessage::RemoteObserved(stopped) = fixture.receive().await else {
            panic!("native stopped observation required")
        };
        assert_eq!(stopped.epoch, stopping.epoch);
        assert!(stopped.revision > stopping.revision);
        assert_eq!(stopped.phase, RemotePhase::Disabled);
        assert!(fixture.app.world().get_entity(dialer).is_err());
        assert!(fixture.app.world().get_entity(local).is_ok());
        fixture.stop().await;
    }
}
