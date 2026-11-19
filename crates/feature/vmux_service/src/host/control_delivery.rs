use async_io::Timer;
use bevy_app::{App, Startup, Update};
use bevy_ecs::prelude::*;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use uuid::Uuid;
use vmux_api::control::{
    ControlHello, CredentialOffer, CredentialOutcome, CredentialReply, CredentialWithdraw,
};
use vmux_ecs::{JobOwner, JobPlugin, JobRequest, JobResult, JobSet, JobWake, Operation, Revision};
use vmux_feature::http::{Credential, CredentialSource, Endpoint};
use vmux_feature::{InstallationScope, ReleaseProvider, Set};
use vmux_transport::service_peer::CodeRequirement;

use super::control_delivery_io::{Connect, Connection, Hello, Offer, Withdraw};

pub struct ControlDeliveryPlugin;

impl bevy_app::Plugin for ControlDeliveryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JobPlugin::<Connect>::default(),
            JobPlugin::<Hello>::default(),
            JobPlugin::<Offer>::default(),
            JobPlugin::<Withdraw>::default(),
        ))
        .add_systems(Startup, configure)
        .add_systems(
            Update,
            (retire, prepare, connect, offer, withdraw)
                .chain()
                .after(Set::Clock)
                .before(JobSet::Submit),
        )
        .add_systems(
            Update,
            (connected, ready, offered, withdrawn)
                .chain()
                .after(JobSet::Apply),
        );
    }
}

#[derive(Resource, Clone)]
pub struct ControlDeliveryTarget {
    socket: PathBuf,
    requirement: CodeRequirement,
    uid: u32,
}

impl ControlDeliveryTarget {
    pub fn for_binary(socket: PathBuf, binary: &std::path::Path) -> Result<Self, String> {
        Ok(Self {
            socket,
            requirement: CodeRequirement::for_binary(binary).map_err(|error| error.to_string())?,
            uid: unsafe { libc::geteuid() },
        })
    }
}

#[derive(Resource)]
struct TargetUnavailable;

#[derive(Component)]
struct Channel;

#[derive(Component, Clone)]
#[component(immutable)]
struct Binding {
    provider: Entity,
    installation: Uuid,
    environment: String,
    endpoint: Endpoint,
}

#[derive(Component, Clone)]
struct Connected(Arc<Connection>);

#[derive(Component)]
struct Nonce(Uuid);

#[derive(Component)]
struct HelloPending {
    operation: Uuid,
}

#[derive(Component, Default)]
struct Delivery {
    generation: u64,
    source: Option<Entity>,
    revision: u64,
    applied: bool,
    operation: Option<Uuid>,
    operation_revision: u64,
}

#[derive(Component)]
struct Pending(Entity);

#[derive(Component)]
struct RetryAt {
    timer: Timer,
}

impl RetryAt {
    fn new(duration: Duration) -> Self {
        Self {
            timer: Timer::after(duration),
        }
    }

    fn ready(&mut self, wake: Option<&JobWake>) -> bool {
        let Some(wake) = wake else {
            return false;
        };
        let mut context = Context::from_waker(&wake.0);
        matches!(Pin::new(&mut self.timer).poll(&mut context), Poll::Ready(_))
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
struct Captured {
    operation: Uuid,
    nonce: Uuid,
    generation: u64,
    revision: u64,
}

#[derive(bevy_ecs::system::SystemParam)]
struct Current<'w, 's> {
    providers: Query<
        'w,
        's,
        (
            &'static Name,
            &'static Endpoint,
            Option<&'static CredentialSource>,
        ),
        With<ReleaseProvider>,
    >,
    scopes: Query<'w, 's, &'static vmux_feature::flag::Context, With<InstallationScope>>,
    credentials: Query<'w, 's, (&'static Credential, &'static Revision)>,
}

impl Current<'_, '_> {
    fn valid(&self, binding: &Binding) -> bool {
        let Ok((name, endpoint, _)) = self.providers.get(binding.provider) else {
            return false;
        };
        let Ok(context) = self.scopes.single() else {
            return false;
        };
        name.as_str() == binding.environment
            && *endpoint == binding.endpoint
            && context.validate().is_ok()
            && Uuid::parse_str(&context.subject.key).ok() == Some(binding.installation)
    }

    fn source(&self, binding: &Binding) -> Option<(Entity, &Credential, u64)> {
        let (_, _, source) = self.providers.get(binding.provider).ok()?;
        let source = source?;
        let (credential, revision) = self.credentials.get(source.0).ok()?;
        if credential.environment() != binding.environment {
            return None;
        }
        Some((source.0, credential, revision.0))
    }
}

fn configure(target: Option<Res<ControlDeliveryTarget>>, mut commands: Commands) {
    if target.is_some() {
        return;
    }
    let result = super::daemon::DaemonBinary::current()
        .map_err(|error| error.to_string())
        .and_then(|binary| {
            ControlDeliveryTarget::for_binary(
                crate::ServicePaths::current().socket(),
                binary.path(),
            )
        });
    match result {
        Ok(target) => {
            commands.insert_resource(target);
        }
        Err(_) => {
            commands.insert_resource(TargetUnavailable);
        }
    }
}

fn retire(
    channels: Query<(Entity, &Binding), With<Channel>>,
    current: Current,
    mut commands: Commands,
) {
    for (entity, binding) in &channels {
        if !current.valid(binding) {
            commands.entity(entity).despawn();
        }
    }
}

fn prepare(
    target: Option<Res<ControlDeliveryTarget>>,
    providers: Query<(Entity, &Name, &Endpoint), With<ReleaseProvider>>,
    scopes: Query<&vmux_feature::flag::Context, With<InstallationScope>>,
    channels: Query<&Binding, With<Channel>>,
    mut commands: Commands,
) {
    let Some(target) = target else {
        return;
    };
    let Ok(context) = scopes.single() else {
        return;
    };
    if context.validate().is_err() {
        return;
    }
    let Ok(installation) = Uuid::parse_str(&context.subject.key) else {
        return;
    };
    for (provider, name, endpoint) in &providers {
        if channels.iter().any(|binding| binding.provider == provider) {
            continue;
        }
        let binding = Binding {
            provider,
            installation,
            environment: name.as_str().into(),
            endpoint: endpoint.clone(),
        };
        let entity = commands
            .spawn((
                Name::new("Control service delivery"),
                Channel,
                binding,
                Delivery::default(),
                ChildOf(provider),
            ))
            .id();
        let input = Connect {
            socket: target.socket.clone(),
            requirement: target.requirement.clone(),
            uid: target.uid,
            timeout: Duration::from_secs(3),
        };
        let operation = commands
            .spawn((
                Operation::bundle("control.connect").expect("Control connect kind"),
                ChildOf(entity),
                input.clone(),
                JobRequest(input),
            ))
            .id();
        commands.entity(entity).insert(Pending(operation));
    }
}

fn connected(
    mut results: Query<(
        Entity,
        &ChildOf,
        &JobOwner<Connect>,
        &mut JobResult<Connect>,
    )>,
    bindings: Query<(&Binding, &Pending), With<Channel>>,
    current: Current,
    mut commands: Commands,
) {
    for (entity, parent, accepted, mut result) in &mut results {
        let value = std::mem::replace(&mut result.0, Err("Control connect result consumed".into()));
        commands.entity(entity).despawn();
        let Ok((binding, pending)) = bindings.get(parent.parent()) else {
            continue;
        };
        if pending.0 != entity || !accepted.accepts(Some(parent)) || !current.valid(binding) {
            continue;
        }
        let connection = match value {
            Ok(Ok(connection)) => connection,
            _ => {
                commands
                    .entity(parent.parent())
                    .remove::<Pending>()
                    .insert(RetryAt::new(Duration::from_secs(3)));
                continue;
            }
        };
        let operation_id = Uuid::new_v4();
        let input = Hello {
            connection: connection.clone(),
            request: ControlHello {
                operation_id,
                version: 1,
                installation: binding.installation,
                environment: binding.environment.clone(),
            },
            timeout: Duration::from_secs(3),
        };
        let operation = commands
            .spawn((
                Operation::bundle("control.hello").expect("Control hello kind"),
                ChildOf(parent.parent()),
                input.clone(),
                JobRequest(input),
            ))
            .id();
        commands.entity(parent.parent()).insert((
            Connected(connection),
            HelloPending {
                operation: operation_id,
            },
            Pending(operation),
        ));
    }
}

fn ready(
    mut results: Query<(Entity, &ChildOf, &JobOwner<Hello>, &mut JobResult<Hello>)>,
    bindings: Query<(&Binding, &HelloPending, &Pending), With<Channel>>,
    current: Current,
    mut commands: Commands,
) {
    for (entity, parent, accepted, mut result) in &mut results {
        let value = std::mem::replace(&mut result.0, Err("Control hello result consumed".into()));
        commands.entity(entity).despawn();
        let Ok((binding, pending, active)) = bindings.get(parent.parent()) else {
            continue;
        };
        if active.0 != entity || !accepted.accepts(Some(parent)) || !current.valid(binding) {
            continue;
        }
        let nonce = match value {
            Ok(Ok(reply))
                if reply.operation_id == pending.operation
                    && reply.version == 1
                    && reply.refusal.is_none() =>
            {
                reply.nonce
            }
            _ => None,
        };
        let Some(nonce) = nonce else {
            commands
                .entity(parent.parent())
                .remove::<(Pending, Connected, HelloPending)>()
                .insert(RetryAt::new(Duration::from_secs(5)));
            continue;
        };
        commands
            .entity(parent.parent())
            .remove::<(Pending, HelloPending, RetryAt)>()
            .insert(Nonce(nonce));
    }
}

impl Captured {
    fn accepts(&self, reply: &CredentialReply) -> bool {
        reply.operation_id == self.operation
            && reply.nonce == self.nonce
            && reply.source_generation == self.generation
            && reply.source_revision == self.revision
            && reply.outcome == CredentialOutcome::Applied
    }
}

type ReplyResults<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static JobOwner<J>,
        &'static Captured,
        &'static mut JobResult<J>,
    ),
>;

type ReadyChannels<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Binding,
        &'static mut Delivery,
        Option<&'static Connected>,
        Option<&'static Nonce>,
        Option<&'static mut RetryAt>,
    ),
    (With<Channel>, Without<Pending>),
>;

fn offered(
    mut results: ReplyResults<Offer>,
    mut channels: Query<(&Binding, &mut Delivery, &Pending, &Nonce), With<Channel>>,
    current: Current,
    mut commands: Commands,
) {
    for (entity, parent, accepted, captured, mut result) in &mut results {
        let value = std::mem::replace(&mut result.0, Err("Control offer result consumed".into()));
        commands.entity(entity).despawn();
        let Ok((binding, mut delivery, pending, nonce)) = channels.get_mut(parent.parent()) else {
            continue;
        };
        if pending.0 != entity
            || nonce.0 != captured.nonce
            || delivery.generation != captured.generation
            || delivery.operation != Some(captured.operation)
            || !accepted.accepts(Some(parent))
            || !current.valid(binding)
        {
            continue;
        }
        commands.entity(parent.parent()).remove::<Pending>();
        match value {
            Ok(Ok(reply)) if captured.accepts(&reply) => {
                delivery.applied = true;
                delivery.revision = captured.revision;
                delivery.operation = None;
            }
            _ => {
                commands
                    .entity(parent.parent())
                    .remove::<(Nonce, Connected)>()
                    .insert(RetryAt::new(Duration::from_secs(3)));
            }
        }
    }
}

fn withdrawn(
    mut results: ReplyResults<Withdraw>,
    mut channels: Query<(&Binding, &mut Delivery, &Pending, &Nonce), With<Channel>>,
    current: Current,
    mut commands: Commands,
) {
    for (entity, parent, accepted, captured, mut result) in &mut results {
        let value = std::mem::replace(
            &mut result.0,
            Err("Control withdraw result consumed".into()),
        );
        commands.entity(entity).despawn();
        let Ok((binding, mut delivery, pending, nonce)) = channels.get_mut(parent.parent()) else {
            continue;
        };
        if pending.0 != entity
            || nonce.0 != captured.nonce
            || delivery.generation != captured.generation
            || delivery.operation != Some(captured.operation)
            || !accepted.accepts(Some(parent))
            || !current.valid(binding)
        {
            continue;
        }
        commands.entity(parent.parent()).remove::<Pending>();
        match value {
            Ok(Ok(reply)) if captured.accepts(&reply) => {
                delivery.applied = false;
                delivery.revision = captured.revision;
                delivery.operation = None;
            }
            _ => {
                commands
                    .entity(parent.parent())
                    .remove::<(Nonce, Connected)>()
                    .insert(RetryAt::new(Duration::from_secs(3)));
            }
        }
    }
}

fn connect(
    mut channels: ReadyChannels,
    current: Current,
    target: Option<Res<ControlDeliveryTarget>>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    let Some(target) = target else { return };
    for (entity, binding, _, connected, nonce, retry) in &mut channels {
        if !current.valid(binding) || connected.is_some() && nonce.is_some() {
            continue;
        }
        if let Some(mut retry) = retry
            && !retry.ready(wake.as_deref())
        {
            continue;
        }
        let input = Connect {
            socket: target.socket.clone(),
            requirement: target.requirement.clone(),
            uid: target.uid,
            timeout: Duration::from_secs(3),
        };
        let operation = commands
            .spawn((
                Operation::bundle("control.connect").expect("Control connect kind"),
                ChildOf(entity),
                input.clone(),
                JobRequest(input),
            ))
            .id();
        commands
            .entity(entity)
            .remove::<RetryAt>()
            .insert(Pending(operation));
    }
}

fn offer(mut channels: ReadyChannels, current: Current, mut commands: Commands) {
    for (entity, binding, mut delivery, connected, nonce, _) in &mut channels {
        if !current.valid(binding) {
            continue;
        }
        let (Some(connection), Some(nonce)) = (connected, nonce) else {
            continue;
        };
        let connection = connection.0.clone();
        let nonce = nonce.0;
        let Some((source, credential, revision)) = current.source(binding) else {
            continue;
        };
        if revision == 0 {
            continue;
        }
        if delivery.source != Some(source) {
            delivery.generation = delivery.generation.saturating_add(1);
            delivery.source = Some(source);
            delivery.revision = 0;
            delivery.operation = None;
        }
        if delivery.applied && delivery.revision == revision {
            continue;
        }
        if delivery.operation_revision != revision {
            delivery.operation = None;
            delivery.operation_revision = revision;
        }
        let operation_id = delivery.operation.unwrap_or_else(Uuid::new_v4);
        delivery.operation = Some(operation_id);
        let request = CredentialOffer {
            operation_id,
            version: 1,
            nonce,
            installation: binding.installation,
            source_generation: delivery.generation,
            source_revision: revision,
            lease: credential.lease(),
        };
        let captured = Captured {
            operation: operation_id,
            nonce,
            generation: delivery.generation,
            revision,
        };
        let input = Offer {
            connection,
            request,
            timeout: Duration::from_secs(3),
        };
        let operation = commands
            .spawn((
                Operation::bundle("control.credential.offer").expect("Control offer kind"),
                ChildOf(entity),
                captured,
                input.clone(),
                JobRequest(input),
            ))
            .id();
        commands.entity(entity).insert(Pending(operation));
    }
}

fn withdraw(mut channels: ReadyChannels, current: Current, mut commands: Commands) {
    for (entity, binding, mut delivery, connected, nonce, _) in &mut channels {
        if !current.valid(binding) || current.source(binding).is_some() {
            continue;
        }
        let (Some(connection), Some(nonce)) = (connected, nonce) else {
            continue;
        };
        let connection = connection.0.clone();
        let nonce = nonce.0;
        if !delivery.applied {
            continue;
        }
        let operation_id = delivery.operation.unwrap_or_else(Uuid::new_v4);
        delivery.operation = Some(operation_id);
        let revision = delivery.revision.saturating_add(1);
        let request = CredentialWithdraw {
            operation_id,
            version: 1,
            nonce,
            installation: binding.installation,
            environment: binding.environment.clone(),
            source_generation: delivery.generation,
            source_revision: revision,
        };
        let captured = Captured {
            operation: operation_id,
            nonce,
            generation: delivery.generation,
            revision,
        };
        let input = Withdraw {
            connection,
            request,
            timeout: Duration::from_secs(3),
        };
        let operation = commands
            .spawn((
                Operation::bundle("control.credential.withdraw").expect("Control withdraw kind"),
                ChildOf(entity),
                captured,
                input.clone(),
                JobRequest(input),
            ))
            .id();
        commands.entity(entity).insert(Pending(operation));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::super::server::{
        Daemon, ExitSignal, ServiceDaemonPlugin, SocketListener, TrustedDesktop,
    };
    use super::*;
    use bevy::prelude::MinimalPlugins;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};
    use tokio::net::UnixListener;
    use tokio::sync::mpsc;
    use vmux_api::protocol::{ClientMessage, ServiceMessage};
    use vmux_ecs::service::{Executor, Wake};
    use vmux_feature::Provider;
    use vmux_transport::service::ServiceConnection;

    struct Fixture {
        sender: App,
        daemon: App,
        provider: Entity,
        source: Entity,
        socket: PathBuf,
        binary: PathBuf,
        _directory: tempfile::TempDir,
        exited: mpsc::Receiver<()>,
    }

    impl Fixture {
        async fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let socket = directory.path().join("service.sock");
            let listener = UnixListener::bind(&socket).unwrap();
            let binary = std::env::current_exe().unwrap();
            let (wake, _wakes) = mpsc::unbounded_channel();
            let (exit, exited) = mpsc::channel(1);
            let installation = Uuid::new_v4();
            let mut daemon = App::new();
            daemon.add_plugins((MinimalPlugins, ServiceDaemonPlugin));
            daemon.world_mut().spawn((
                Daemon,
                SocketListener(Some(listener)),
                ExitSignal(exit),
                Executor(tokio::runtime::Handle::current()),
                Wake(wake),
                TrustedDesktop::for_binary(&binary).unwrap(),
            ));
            daemon.world_mut().spawn((
                Provider::bundle("development"),
                ReleaseProvider,
                Endpoint::new("http://127.0.0.1:9/v1/flags/snapshot").unwrap(),
            ));
            daemon.world_mut().spawn((
                Name::new("Daemon installation"),
                InstallationScope,
                vmux_feature::flag::Context::installation(&installation.to_string()),
            ));
            daemon.update();
            let mut sender = App::new();
            sender.insert_resource(
                ControlDeliveryTarget::for_binary(socket.clone(), &binary).unwrap(),
            );
            sender.add_plugins((vmux_feature::FeaturePlugin, ControlDeliveryPlugin));
            let provider = sender
                .world_mut()
                .spawn((
                    Provider::bundle("development"),
                    ReleaseProvider,
                    Endpoint::new("http://127.0.0.1:9/v1/flags/snapshot").unwrap(),
                ))
                .id();
            sender.world_mut().spawn((
                Name::new("Sender installation"),
                InstallationScope,
                vmux_feature::flag::Context::installation(&installation.to_string()),
            ));
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let source = sender
                .world_mut()
                .spawn((
                    Name::new("Desktop Control broker credential"),
                    Credential::new(
                        "fixture-delivered-credential".into(),
                        Uuid::new_v4(),
                        Uuid::new_v4(),
                        Uuid::new_v4(),
                        "development".into(),
                        now + 120,
                    )
                    .unwrap(),
                    Revision(1),
                ))
                .id();
            sender
                .world_mut()
                .entity_mut(provider)
                .insert(CredentialSource(source));
            Self {
                sender,
                daemon,
                provider,
                source,
                socket,
                binary,
                _directory: directory,
                exited,
            }
        }

        async fn settle(&mut self, present: bool) {
            let until = Instant::now() + Duration::from_secs(8);
            loop {
                self.sender.update();
                self.daemon.update();
                let world = self.daemon.world_mut();
                let installed = world.query::<&Credential>().iter(world).next().is_some();
                let world = self.sender.world_mut();
                let settled = world
                    .query_filtered::<&Delivery, (With<Channel>, Without<Pending>)>()
                    .single(world)
                    .is_ok_and(|delivery| {
                        delivery.applied == present && (present || delivery.revision > 1)
                    });
                if installed == present && settled {
                    return;
                }
                assert!(
                    Instant::now() < until,
                    "production credential delivery did not settle"
                );
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }

        async fn stop(mut self) {
            self.sender.world_mut().entity_mut(self.provider).despawn();
            self.sender.update();
            let requirement = CodeRequirement::for_binary(&self.binary).unwrap();
            let connection =
                ServiceConnection::connect_checked(&self.socket, &requirement, unsafe {
                    libc::geteuid()
                })
                .await
                .unwrap();
            connection.send(&ClientMessage::Shutdown).await.unwrap();
            let result = tokio::spawn(async move { connection.recv().await.unwrap().unwrap() });
            while !result.is_finished() {
                self.daemon.update();
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            assert!(matches!(
                result.await.unwrap(),
                ServiceMessage::ProcessList { .. }
            ));
            self.exited.recv().await.unwrap();
            self.daemon.update();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_desktop_source_reaches_an_independent_signed_daemon_and_unlink_withdraws_it() {
        let mut fixture = Fixture::new().await;
        fixture.settle(true).await;
        let expected = fixture
            .sender
            .world()
            .get::<Credential>(fixture.source)
            .unwrap()
            .lease();
        let world = fixture.daemon.world_mut();
        assert_eq!(
            world.query::<&Credential>().single(world).unwrap().lease(),
            expected
        );
        fixture
            .sender
            .world_mut()
            .entity_mut(fixture.source)
            .remove::<Credential>();
        fixture.settle(false).await;
        fixture.stop().await;
    }
}
