use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use std::sync::Arc;
use uuid::Uuid;
use vmux_api::control::{ControlCredentialSet, ControlReady, CredentialOutcome, CredentialReply};
use vmux_ecs::service::{Protocols, Register, Wake};
use vmux_ecs::{Operation, Revision, TokioTask};
use vmux_feature::http::{Credential, CredentialSource, Endpoint};
use vmux_feature::{InstallationScope, ReleaseProvider, Set};

use super::control_driver::{Driver, HelloInput, Inbox, OfferInput, WithdrawInput};
use super::peer::{ClientPeer, DesktopPeer, TrustedDesktop};
use super::server::{Client, Daemon};

pub(super) struct ControlPlugin;

impl bevy_app::Plugin for ControlPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_feature::FeaturePlugin>() {
            app.add_plugins(vmux_feature::FeaturePlugin);
        }
        if !app.is_plugin_added::<vmux_feature::clock::ClockPlugin>() {
            app.add_plugins(vmux_feature::clock::ClockPlugin);
        }
        app.init_resource::<ControlTime>()
            .add_systems(Update, register.in_set(Register))
            .add_systems(
                Update,
                (
                    sample,
                    bind,
                    receive,
                    expire,
                    hello,
                    offer,
                    withdraw,
                    reply_hello,
                    reply_offer,
                    reply_withdraw,
                )
                    .chain()
                    .in_set(ControlCredentialSet)
                    .after(Set::Clock)
                    .before(Set::Prepare),
            );
    }
}

#[derive(Resource, Default)]
struct ControlTime(u64);

#[derive(Component)]
#[require(Revision)]
struct ControlSource;

#[derive(Component, Default)]
struct Epoch(u64);

#[derive(Clone, PartialEq, Eq)]
struct Binding {
    provider: Entity,
    installation: Uuid,
    environment: String,
    endpoint: Endpoint,
}

#[derive(Component)]
#[component(immutable)]
struct Session {
    nonce: Uuid,
    epoch: u64,
    binding: Binding,
}

#[derive(Component)]
#[component(immutable)]
struct Installed {
    nonce: Uuid,
    epoch: u64,
    generation: u64,
    revision: u64,
    operation: Uuid,
    withdrawn: bool,
    binding: Binding,
}

impl Installed {
    fn permits(&self, epoch: u64, generation: u64, revision: u64) -> bool {
        if epoch > self.epoch {
            return true;
        }
        epoch == self.epoch
            && (generation > self.generation
                || generation == self.generation && revision > self.revision)
    }
}

#[derive(Component)]
struct HelloReply {
    client: Option<Entity>,
    value: ControlReady,
}

#[derive(Component)]
struct LeaseReply(CredentialReply);

#[derive(bevy_ecs::system::SystemParam)]
struct Current<'w, 's> {
    clients: Query<
        'w,
        's,
        (
            &'static ChildOf,
            &'static ClientPeer,
            &'static TokioTask,
            Has<DesktopPeer>,
        ),
        With<Client>,
    >,
    policies: Query<'w, 's, &'static TrustedDesktop, With<Daemon>>,
    sessions: Query<'w, 's, &'static Session>,
    scopes: Query<'w, 's, &'static vmux_feature::flag::Context, With<InstallationScope>>,
    providers: Query<'w, 's, (Entity, &'static Name, &'static Endpoint), With<ReleaseProvider>>,
}

impl Current<'_, '_> {
    fn hello(
        &self,
        client: Option<u64>,
        daemon: Entity,
        request: &vmux_api::control::ControlHello,
    ) -> Result<(Entity, Binding), CredentialOutcome> {
        if request.version != 1 {
            return Err(CredentialOutcome::Unsupported);
        }
        if request.operation_id.is_nil() {
            return Err(CredentialOutcome::Invalid);
        }
        Ok((
            self.client(client, daemon)?,
            self.binding(request.installation, &request.environment)?,
        ))
    }

    fn offer(
        &self,
        client: Option<u64>,
        daemon: Entity,
        request: &vmux_api::control::CredentialOffer,
        now: u64,
        epoch: u64,
    ) -> Result<&Session, CredentialOutcome> {
        if request.version != 1 {
            return Err(CredentialOutcome::Unsupported);
        }
        if request.operation_id.is_nil()
            || request.source_generation == 0
            || request.source_revision == 0
            || !request.lease.valid()
        {
            return Err(CredentialOutcome::Invalid);
        }
        if request.lease.expires_at <= now {
            return Err(CredentialOutcome::Expired);
        }
        if request.lease.expires_at > now.saturating_add(300) {
            return Err(CredentialOutcome::Invalid);
        }
        self.session(
            client,
            daemon,
            request.nonce,
            request.installation,
            &request.lease.environment,
            epoch,
        )
    }

    fn withdraw(
        &self,
        client: Option<u64>,
        daemon: Entity,
        request: &vmux_api::control::CredentialWithdraw,
        epoch: u64,
    ) -> Result<&Session, CredentialOutcome> {
        if request.version != 1 {
            return Err(CredentialOutcome::Unsupported);
        }
        if request.operation_id.is_nil()
            || request.source_generation == 0
            || request.source_revision == 0
        {
            return Err(CredentialOutcome::Invalid);
        }
        self.session(
            client,
            daemon,
            request.nonce,
            request.installation,
            &request.environment,
            epoch,
        )
    }

    fn client(&self, client: Option<u64>, daemon: Entity) -> Result<Entity, CredentialOutcome> {
        let client = Entity::from_bits(client.ok_or(CredentialOutcome::Refused)?);
        let (parent, peer, task, admitted) = self
            .clients
            .get(client)
            .map_err(|_| CredentialOutcome::Refused)?;
        if parent.parent() != daemon || !admitted || task.is_finished() {
            return Err(CredentialOutcome::Refused);
        }
        let policy = self
            .policies
            .get(daemon)
            .map_err(|_| CredentialOutcome::Refused)?;
        if !policy.admits(&peer.0) {
            return Err(CredentialOutcome::Refused);
        }
        Ok(client)
    }

    fn binding(&self, installation: Uuid, environment: &str) -> Result<Binding, CredentialOutcome> {
        let context = self
            .scopes
            .single()
            .map_err(|_| CredentialOutcome::Unavailable)?;
        if context.validate().is_err()
            || Uuid::parse_str(&context.subject.key).ok() != Some(installation)
        {
            return Err(CredentialOutcome::Invalid);
        }
        let (provider, name, endpoint) = self
            .providers
            .single()
            .map_err(|_| CredentialOutcome::Unavailable)?;
        if name.as_str() != environment {
            return Err(CredentialOutcome::Invalid);
        }
        Ok(Binding {
            provider,
            installation,
            environment: environment.into(),
            endpoint: endpoint.clone(),
        })
    }

    fn session(
        &self,
        client: Option<u64>,
        daemon: Entity,
        nonce: Uuid,
        installation: Uuid,
        environment: &str,
        epoch: u64,
    ) -> Result<&Session, CredentialOutcome> {
        let client = self.client(client, daemon)?;
        let session = self
            .sessions
            .get(client)
            .map_err(|_| CredentialOutcome::Refused)?;
        if session.nonce != nonce
            || session.epoch != epoch
            || session.binding != self.binding(installation, environment)?
        {
            return Err(CredentialOutcome::Stale);
        }
        Ok(session)
    }
}

fn sample(mut ticks: MessageReader<vmux_feature::Tick>, mut time: ResMut<ControlTime>) {
    for tick in ticks.read() {
        time.0 = tick.0;
    }
}

fn register(
    mut daemons: Query<(Entity, &mut Protocols, &Wake), With<Daemon>>,
    sources: Query<&ChildOf, With<ControlSource>>,
    mut commands: Commands,
) {
    for (daemon, mut protocols, wake) in &mut daemons {
        if sources.iter().any(|parent| parent.parent() == daemon) {
            continue;
        }
        let (driver, inbox) = Driver::new(wake.0.clone());
        protocols.0.push(Arc::new(driver));
        commands.spawn((
            Name::new("Service Control credentials"),
            ControlSource,
            Epoch::default(),
            inbox,
            ChildOf(daemon),
        ));
    }
}

fn bind(
    providers: Query<(Entity, Option<&CredentialSource>), With<ReleaseProvider>>,
    sources: Query<Entity, With<ControlSource>>,
    mut commands: Commands,
) {
    let Ok(source) = sources.single() else {
        return;
    };
    for (provider, existing) in &providers {
        if existing.is_none_or(|existing| existing.0 != source) {
            commands.entity(provider).insert(CredentialSource(source));
        }
    }
}

fn receive(mut sources: Query<(Entity, &mut Inbox), With<ControlSource>>, mut commands: Commands) {
    for (source, mut inbox) in &mut sources {
        if let Ok(input) = inbox.hello.try_recv() {
            commands.spawn((
                Operation::bundle("control.hello").expect("Control hello kind"),
                ChildOf(source),
                input,
            ));
        }
        if let Ok(input) = inbox.offer.try_recv() {
            commands.spawn((
                Operation::bundle("control.credential.offer").expect("Control offer kind"),
                ChildOf(source),
                input,
            ));
        }
        if let Ok(input) = inbox.withdraw.try_recv() {
            commands.spawn((
                Operation::bundle("control.credential.withdraw").expect("Control withdraw kind"),
                ChildOf(source),
                input,
            ));
        }
    }
}

fn expire(
    sources: Query<(Entity, &Installed, &Credential), With<ControlSource>>,
    now: Res<ControlTime>,
    current: Current,
    mut commands: Commands,
) {
    for (source, installed, credential) in &sources {
        if credential.expires_at() <= now.0
            || current
                .binding(
                    installed.binding.installation,
                    &installed.binding.environment,
                )
                .ok()
                .as_ref()
                != Some(&installed.binding)
        {
            commands.entity(source).remove::<Credential>();
        }
    }
}

fn hello(
    mut inputs: Query<(Entity, &ChildOf, &mut HelloInput), Without<HelloReply>>,
    mut sources: Query<(&ChildOf, &mut Epoch), With<ControlSource>>,
    current: Current,
    mut commands: Commands,
) {
    for (entity, parent, mut input) in &mut inputs {
        let Some(request) = input.request.take() else {
            continue;
        };
        let mut value = ControlReady {
            operation_id: request.operation_id,
            version: 1,
            nonce: None,
            refusal: None,
        };
        let result = match sources.get_mut(parent.parent()) {
            Ok((daemon, mut epoch)) => match current.hello(input.client, daemon.parent(), &request)
            {
                Ok((client, binding)) => match epoch.0.checked_add(1) {
                    Some(next) => {
                        epoch.0 = next;
                        let nonce = Uuid::new_v4();
                        commands.entity(client).insert(Session {
                            nonce,
                            epoch: next,
                            binding,
                        });
                        Ok((client, nonce))
                    }
                    None => Err(CredentialOutcome::Unavailable),
                },
                Err(error) => Err(error),
            },
            Err(_) => Err(CredentialOutcome::Unavailable),
        };
        let client = match result {
            Ok((client, nonce)) => {
                value.nonce = Some(nonce);
                Some(client)
            }
            Err(error) => {
                value.refusal = Some(error);
                None
            }
        };
        commands.entity(entity).insert(HelloReply { client, value });
    }
}

type OfferSources<'w, 's> = Query<
    'w,
    's,
    (
        &'static ChildOf,
        &'static Epoch,
        &'static mut Revision,
        Option<&'static Installed>,
        Option<&'static Credential>,
    ),
    With<ControlSource>,
>;

fn offer(
    mut inputs: Query<(Entity, &ChildOf, &mut OfferInput), Without<LeaseReply>>,
    mut sources: OfferSources,
    now: Res<ControlTime>,
    current: Current,
    mut commands: Commands,
) {
    for (entity, parent, mut input) in &mut inputs {
        let Some(request) = input.request.take() else {
            continue;
        };
        let mut value = CredentialReply {
            operation_id: request.operation_id,
            nonce: request.nonce,
            source_generation: request.source_generation,
            source_revision: request.source_revision,
            revision: 0,
            outcome: CredentialOutcome::Applied,
        };
        let (daemon, epoch, mut revision, installed, credential) =
            match sources.get_mut(parent.parent()) {
                Ok(source) => source,
                Err(_) => {
                    value.outcome = CredentialOutcome::Unavailable;
                    commands.entity(entity).insert(LeaseReply(value));
                    continue;
                }
            };
        let session = match current.offer(input.client, daemon.parent(), &request, now.0, epoch.0) {
            Ok(session) => session,
            Err(error) => {
                value.outcome = error;
                commands.entity(entity).insert(LeaseReply(value));
                continue;
            }
        };
        let duplicate = installed.is_some_and(|installed| {
            installed.operation == request.operation_id
                && !installed.withdrawn
                && installed.nonce == request.nonce
                && installed.generation == request.source_generation
                && installed.revision == request.source_revision
                && credential.is_some_and(|credential| credential.lease() == request.lease)
        });
        if duplicate {
            value.revision = revision.0;
            commands.entity(entity).insert(LeaseReply(value));
            continue;
        }
        if installed.is_some_and(|installed| {
            !installed.permits(
                session.epoch,
                request.source_generation,
                request.source_revision,
            )
        }) {
            value.outcome = CredentialOutcome::Stale;
            commands.entity(entity).insert(LeaseReply(value));
            continue;
        }
        let credential = match Credential::from_lease(request.lease.clone()) {
            Ok(credential) => credential,
            Err(_) => {
                value.outcome = CredentialOutcome::Invalid;
                commands.entity(entity).insert(LeaseReply(value));
                continue;
            }
        };
        let Some(next) = revision.0.checked_add(1) else {
            value.outcome = CredentialOutcome::Unavailable;
            commands.entity(entity).insert(LeaseReply(value));
            continue;
        };
        revision.0 = next;
        commands.entity(parent.parent()).insert((
            credential,
            Installed {
                nonce: request.nonce,
                epoch: session.epoch,
                generation: request.source_generation,
                revision: request.source_revision,
                operation: request.operation_id,
                withdrawn: false,
                binding: session.binding.clone(),
            },
        ));
        value.revision = next;
        commands.entity(entity).insert(LeaseReply(value));
    }
}

fn withdraw(
    mut inputs: Query<(Entity, &ChildOf, &mut WithdrawInput), Without<LeaseReply>>,
    mut sources: Query<(&ChildOf, &Epoch, &mut Revision, Option<&Installed>), With<ControlSource>>,
    current: Current,
    mut commands: Commands,
) {
    for (entity, parent, mut input) in &mut inputs {
        let Some(request) = input.request.take() else {
            continue;
        };
        let mut value = CredentialReply {
            operation_id: request.operation_id,
            nonce: request.nonce,
            source_generation: request.source_generation,
            source_revision: request.source_revision,
            revision: 0,
            outcome: CredentialOutcome::Applied,
        };
        let (daemon, epoch, mut revision, installed) = match sources.get_mut(parent.parent()) {
            Ok(source) => source,
            Err(_) => {
                value.outcome = CredentialOutcome::Unavailable;
                commands.entity(entity).insert(LeaseReply(value));
                continue;
            }
        };
        let session = match current.withdraw(input.client, daemon.parent(), &request, epoch.0) {
            Ok(session) => session,
            Err(error) => {
                value.outcome = error;
                commands.entity(entity).insert(LeaseReply(value));
                continue;
            }
        };
        let duplicate = installed.is_some_and(|installed| {
            installed.operation == request.operation_id
                && installed.withdrawn
                && installed.nonce == request.nonce
                && installed.generation == request.source_generation
                && installed.revision == request.source_revision
        });
        if duplicate {
            value.revision = revision.0;
            commands.entity(entity).insert(LeaseReply(value));
            continue;
        }
        if installed.is_some_and(|installed| {
            !installed.permits(
                session.epoch,
                request.source_generation,
                request.source_revision,
            )
        }) {
            value.outcome = CredentialOutcome::Stale;
            commands.entity(entity).insert(LeaseReply(value));
            continue;
        }
        let Some(next) = revision.0.checked_add(1) else {
            value.outcome = CredentialOutcome::Unavailable;
            commands.entity(entity).insert(LeaseReply(value));
            continue;
        };
        revision.0 = next;
        commands
            .entity(parent.parent())
            .remove::<Credential>()
            .insert(Installed {
                nonce: request.nonce,
                epoch: session.epoch,
                generation: request.source_generation,
                revision: request.source_revision,
                operation: request.operation_id,
                withdrawn: true,
                binding: session.binding.clone(),
            });
        value.revision = next;
        commands.entity(entity).insert(LeaseReply(value));
    }
}

fn reply_hello(
    mut inputs: Query<(Entity, &mut HelloInput, &mut HelloReply)>,
    sessions: Query<&Session>,
    mut commands: Commands,
) {
    for (entity, mut input, mut reply) in &mut inputs {
        if reply.value.refusal.is_none()
            && !reply.client.is_some_and(|client| {
                sessions
                    .get(client)
                    .is_ok_and(|session| Some(session.nonce) == reply.value.nonce)
            })
        {
            reply.value.nonce = None;
            reply.value.refusal = Some(CredentialOutcome::Stale);
        }
        if let Some(sender) = input.reply.take() {
            let _ = sender.send(reply.value.clone());
        }
        commands.entity(entity).despawn();
    }
}

fn reply_offer(
    mut inputs: Query<(Entity, &ChildOf, &mut OfferInput, &mut LeaseReply)>,
    sources: Query<(&Installed, &Revision), With<ControlSource>>,
    mut commands: Commands,
) {
    for (entity, parent, mut input, mut reply) in &mut inputs {
        if reply.0.outcome == CredentialOutcome::Applied
            && !sources
                .get(parent.parent())
                .is_ok_and(|(installed, revision)| {
                    installed.operation == reply.0.operation_id
                        && revision.0 == reply.0.revision
                        && !installed.withdrawn
                })
        {
            reply.0.outcome = CredentialOutcome::Stale;
        }
        if let Some(sender) = input.reply.take() {
            let _ = sender.send(reply.0.clone());
        }
        commands.entity(entity).despawn();
    }
}

fn reply_withdraw(
    mut inputs: Query<(Entity, &ChildOf, &mut WithdrawInput, &mut LeaseReply)>,
    sources: Query<(&Installed, &Revision), With<ControlSource>>,
    mut commands: Commands,
) {
    for (entity, parent, mut input, mut reply) in &mut inputs {
        if reply.0.outcome == CredentialOutcome::Applied
            && !sources
                .get(parent.parent())
                .is_ok_and(|(installed, revision)| {
                    installed.operation == reply.0.operation_id
                        && revision.0 == reply.0.revision
                        && installed.withdrawn
                })
        {
            reply.0.outcome = CredentialOutcome::Stale;
        }
        if let Some(sender) = input.reply.take() {
            let _ = sender.send(reply.0.clone());
        }
        commands.entity(entity).despawn();
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use tokio::net::UnixListener;
    use tokio::sync::mpsc;
    use vmux_api::control::{
        ControlHello, ControlLease, ControlToken, CredentialOffer, CredentialWithdraw,
    };
    use vmux_api::protocol::{ClientMessage, ServiceMessage};
    use vmux_ecs::service::Executor;
    use vmux_transport::service::ServiceConnection;
    use vmux_transport::service_peer::CodeRequirement;

    struct Fixture {
        app: bevy::prelude::App,
        connection: Arc<ServiceConnection>,
        installation: Uuid,
        _directory: tempfile::TempDir,
        exited: mpsc::Receiver<()>,
    }

    impl Fixture {
        async fn new(trusted: bool) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let socket = directory.path().join("service.sock");
            let listener = UnixListener::bind(&socket).unwrap();
            let binary = std::env::current_exe().unwrap();
            let policy_binary = if trusted {
                binary.as_path()
            } else {
                std::path::Path::new("/bin/ls")
            };
            let (wake, _wakes) = mpsc::unbounded_channel();
            let (exit, exited) = mpsc::channel(1);
            let installation = Uuid::new_v4();
            let mut app = bevy::prelude::App::new();
            app.add_plugins((
                bevy::prelude::MinimalPlugins,
                super::super::server::ServiceDaemonPlugin,
            ));
            app.world_mut().spawn((
                Daemon,
                super::super::server::SocketListener(Some(listener)),
                super::super::server::ExitSignal(exit),
                Executor(tokio::runtime::Handle::current()),
                Wake(wake),
                TrustedDesktop::for_binary(policy_binary).unwrap(),
            ));
            app.world_mut().spawn((
                vmux_feature::Provider::bundle("development"),
                ReleaseProvider,
                Endpoint::new("http://127.0.0.1:9/v1/flags/snapshot").unwrap(),
            ));
            app.world_mut().spawn((
                Name::new("Control installation"),
                InstallationScope,
                vmux_feature::flag::Context::installation(&installation.to_string()),
            ));
            app.update();
            let requirement = CodeRequirement::for_binary(&binary).unwrap();
            let connection = ServiceConnection::connect_checked(&socket, &requirement, unsafe {
                libc::geteuid()
            })
            .await
            .unwrap();
            Self {
                app,
                connection: Arc::new(connection),
                installation,
                _directory: directory,
                exited,
            }
        }

        async fn request(&mut self, message: ClientMessage) -> ServiceMessage {
            self.connection.send(&message).await.unwrap();
            let connection = self.connection.clone();
            let result = tokio::spawn(async move { connection.recv().await.unwrap().unwrap() });
            let until = Instant::now() + Duration::from_secs(5);
            while !result.is_finished() {
                self.app.update();
                assert!(
                    Instant::now() < until,
                    "production Control request did not complete"
                );
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            result.await.unwrap()
        }

        async fn replace_connection(&mut self) -> Arc<ServiceConnection> {
            let binary = std::env::current_exe().unwrap();
            let requirement = CodeRequirement::for_binary(&binary).unwrap();
            let connection = ServiceConnection::connect_checked(
                &self._directory.path().join("service.sock"),
                &requirement,
                unsafe { libc::geteuid() },
            )
            .await
            .unwrap();
            std::mem::replace(&mut self.connection, Arc::new(connection))
        }

        fn installed(&mut self) -> Option<ControlLease> {
            let world = self.app.world_mut();
            world
                .query_filtered::<&Credential, With<ControlSource>>()
                .single(world)
                .ok()
                .map(Credential::lease)
        }

        async fn stop(mut self) {
            let reply = self.request(ClientMessage::Shutdown).await;
            assert!(matches!(reply, ServiceMessage::ProcessList { .. }));
            self.exited.recv().await.unwrap();
            self.app.update();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn only_the_actual_signed_desktop_peer_can_install_and_withdraw_a_versioned_credential() {
        for trusted in [true, false] {
            let mut fixture = Fixture::new(trusted).await;
            let hello = ControlHello {
                operation_id: Uuid::new_v4(),
                version: 1,
                installation: fixture.installation,
                environment: "development".into(),
            };
            let ready = match fixture
                .request(ClientMessage::ControlHello(Box::new(hello)))
                .await
            {
                ServiceMessage::ControlReady(ready) => ready,
                other => panic!("unexpected Control hello reply: {other:?}"),
            };
            if !trusted {
                assert_eq!(ready.refusal, Some(CredentialOutcome::Refused));
                assert!(ready.nonce.is_none());
                assert!(fixture.installed().is_none());
                fixture.stop().await;
                continue;
            }
            assert!(ready.refusal.is_none());
            let nonce = ready.nonce.unwrap();
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let lease = ControlLease {
                token: ControlToken::new("fixture-control-credential".into()).unwrap(),
                account_id: Uuid::new_v4(),
                device_id: Uuid::new_v4(),
                workspace_id: Uuid::new_v4(),
                environment: "development".into(),
                expires_at: now + 120,
            };
            let offer = CredentialOffer {
                operation_id: Uuid::new_v4(),
                version: 1,
                nonce,
                installation: fixture.installation,
                source_generation: 1,
                source_revision: 1,
                lease: lease.clone(),
            };
            let installed = match fixture
                .request(ClientMessage::CredentialOffer(Box::new(offer.clone())))
                .await
            {
                ServiceMessage::CredentialReply(reply) => reply,
                other => panic!("unexpected credential offer reply: {other:?}"),
            };
            assert_eq!(installed.outcome, CredentialOutcome::Applied);
            assert_eq!(fixture.installed(), Some(lease.clone()));
            let repeated = match fixture
                .request(ClientMessage::CredentialOffer(Box::new(offer.clone())))
                .await
            {
                ServiceMessage::CredentialReply(reply) => reply,
                other => panic!("unexpected repeated credential reply: {other:?}"),
            };
            assert_eq!(repeated.revision, installed.revision);
            assert_eq!(repeated.outcome, CredentialOutcome::Applied);
            let mut altered = offer.clone();
            altered.lease.token = ControlToken::new("altered-fixture-credential".into()).unwrap();
            let rejected = match fixture
                .request(ClientMessage::CredentialOffer(Box::new(altered)))
                .await
            {
                ServiceMessage::CredentialReply(reply) => reply,
                other => panic!("unexpected altered credential reply: {other:?}"),
            };
            assert_eq!(rejected.outcome, CredentialOutcome::Stale);
            assert_eq!(fixture.installed(), Some(lease));
            let withdrawal = CredentialWithdraw {
                operation_id: Uuid::new_v4(),
                version: 1,
                nonce,
                installation: fixture.installation,
                environment: "development".into(),
                source_generation: 1,
                source_revision: 2,
            };
            let withdrawn = match fixture
                .request(ClientMessage::CredentialWithdraw(Box::new(withdrawal)))
                .await
            {
                ServiceMessage::CredentialReply(reply) => reply,
                other => panic!("unexpected credential withdrawal reply: {other:?}"),
            };
            assert_eq!(withdrawn.outcome, CredentialOutcome::Applied);
            assert!(withdrawn.revision > installed.revision);
            assert!(fixture.installed().is_none());
            fixture.stop().await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connection_replacement_and_invalid_contexts_cannot_reuse_an_old_credential_offer() {
        let mut fixture = Fixture::new(true).await;
        let mut hello = ControlHello {
            operation_id: Uuid::new_v4(),
            version: 0,
            installation: fixture.installation,
            environment: "development".into(),
        };
        let unsupported = fixture
            .request(ClientMessage::ControlHello(Box::new(hello.clone())))
            .await;
        assert!(matches!(
            unsupported,
            ServiceMessage::ControlReady(ControlReady {
                refusal: Some(CredentialOutcome::Unsupported),
                ..
            })
        ));
        hello.version = 1;
        hello.operation_id = Uuid::new_v4();
        let nonce = match fixture
            .request(ClientMessage::ControlHello(Box::new(hello.clone())))
            .await
        {
            ServiceMessage::ControlReady(ready) => ready.nonce.unwrap(),
            other => panic!("unexpected Control hello: {other:?}"),
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let offer = CredentialOffer {
            operation_id: Uuid::new_v4(),
            version: 1,
            nonce,
            installation: fixture.installation,
            source_generation: 1,
            source_revision: 1,
            lease: ControlLease {
                token: ControlToken::new("context-fixture-credential".into()).unwrap(),
                account_id: Uuid::new_v4(),
                device_id: Uuid::new_v4(),
                workspace_id: Uuid::new_v4(),
                environment: "development".into(),
                expires_at: now + 120,
            },
        };
        for case in 0..4 {
            let mut invalid = offer.clone();
            let expected = match case {
                0 => {
                    invalid.nonce = Uuid::new_v4();
                    CredentialOutcome::Stale
                }
                1 => {
                    invalid.installation = Uuid::new_v4();
                    CredentialOutcome::Invalid
                }
                2 => {
                    invalid.lease.environment = "production".into();
                    CredentialOutcome::Invalid
                }
                _ => {
                    invalid.lease.expires_at = now - 1;
                    CredentialOutcome::Expired
                }
            };
            let reply = fixture
                .request(ClientMessage::CredentialOffer(Box::new(invalid)))
                .await;
            assert!(
                matches!(reply, ServiceMessage::CredentialReply(reply) if reply.outcome == expected)
            );
            assert!(fixture.installed().is_none());
        }
        let accepted = fixture
            .request(ClientMessage::CredentialOffer(Box::new(offer.clone())))
            .await;
        assert!(
            matches!(accepted, ServiceMessage::CredentialReply(reply) if reply.outcome == CredentialOutcome::Applied)
        );
        let old = fixture.replace_connection().await;
        hello.operation_id = Uuid::new_v4();
        let next_nonce = match fixture
            .request(ClientMessage::ControlHello(Box::new(hello)))
            .await
        {
            ServiceMessage::ControlReady(ready) => ready.nonce.unwrap(),
            other => panic!("unexpected replacement hello: {other:?}"),
        };
        assert_ne!(nonce, next_nonce);
        let replacement = std::mem::replace(&mut fixture.connection, old);
        let mut stale = offer.clone();
        stale.operation_id = Uuid::new_v4();
        stale.source_revision = 2;
        let rejected = fixture
            .request(ClientMessage::CredentialOffer(Box::new(stale)))
            .await;
        assert!(
            matches!(rejected, ServiceMessage::CredentialReply(reply) if reply.outcome == CredentialOutcome::Stale)
        );
        assert_eq!(fixture.installed(), Some(offer.lease.clone()));
        fixture.connection = replacement;
        let mut fresh = offer;
        fresh.operation_id = Uuid::new_v4();
        fresh.nonce = next_nonce;
        fresh.lease.token = ControlToken::new("replacement-fixture-credential".into()).unwrap();
        let expected = fresh.lease.clone();
        let replaced = fixture
            .request(ClientMessage::CredentialOffer(Box::new(fresh)))
            .await;
        assert!(
            matches!(replaced, ServiceMessage::CredentialReply(reply) if reply.outcome == CredentialOutcome::Applied)
        );
        assert_eq!(fixture.installed(), Some(expected));
        fixture.stop().await;
    }
}
