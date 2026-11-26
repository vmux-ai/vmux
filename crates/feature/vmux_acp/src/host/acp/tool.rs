use std::marker::PhantomData;
use std::path::PathBuf;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_ecs::profile::CurrentProfile;
use vmux_ecs::{Operation, OperationRetained, OperationState, OperationTarget};
use vmux_editor::lsp::package_path::PackageName;
use vmux_editor::lsp::store::PackageStore;
use vmux_tool::state::{
    ToolAdoptRequest, ToolForgetRequest, ToolInstallRequest, ToolOperationKey, ToolOperationKind,
    ToolUninstallRequest, ToolUpdateRequest,
};
use vmux_tool::{
    ToolOperationFailed, ToolOperationFinished, ToolOperationRequest, ToolOperationRouteSet,
    ToolOperationSucceeded, ToolProviderId, ToolProviderTarget, ToolStore, ToolStoreOperation,
    ToolStoreTarget,
};

use super::package;
use super::registry::{Catalog, CatalogSource, RegistryAgent};

pub(super) struct AcpToolPlugin;

impl Plugin for AcpToolPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<package::PackagePreparationPlugin>() {
            app.add_plugins(package::PackagePreparationPlugin);
        }
        app.add_systems(
            Update,
            (
                install::<ToolInstallRequest>,
                install::<ToolUpdateRequest>,
                uninstall,
            )
                .in_set(ToolOperationRouteSet),
        )
        .add_systems(
            Update,
            (
                (
                    cancel::<ToolInstallRequest>,
                    cancel::<ToolUpdateRequest>,
                    cancel::<ToolUninstallRequest>,
                )
                    .in_set(vmux_tool::ToolOperationCancelSet),
                cancel_configuration.in_set(vmux_tool::ToolOperationCancelSet),
                prepared,
                removed,
                configured,
            )
                .chain()
                .before(vmux_editor::lsp::activation::ActivationSet)
                .before(vmux_job::JobSet::Submit),
        )
        .add_systems(
            PostUpdate,
            (
                release::<ToolInstallRequest>,
                release::<ToolUpdateRequest>,
                release::<ToolUninstallRequest>,
                orphaned,
            )
                .after(vmux_job::JobSet::Cleanup),
        );
    }
}

#[derive(Component)]
struct Accepted<R: Send + Sync + 'static>(PhantomData<R>);

#[derive(Component)]
struct Selection {
    store_entity: Entity,
    store: ToolStore,
    provider: Entity,
    profile: Entity,
    agents: PathBuf,
    catalog: Option<(Entity, u64)>,
    id: String,
    message: String,
    release: bool,
}

#[derive(Component)]
struct Preparation(Entity);

#[derive(Component)]
struct Removal(Entity);

#[derive(Component)]
struct Configuration(Entity);

#[derive(Component)]
struct ConfigurationFor(Entity);

#[derive(Component)]
struct Done;

#[derive(SystemParam)]
struct Context<'w, 's> {
    profile: CurrentProfile<'w, 's>,
    stores: Query<'w, 's, &'static ToolStore>,
    providers: Query<'w, 's, &'static ToolProviderId>,
    catalogs: Query<'w, 's, (Entity, &'static Catalog, &'static CatalogSource)>,
    policy: Query<'w, 's, &'static crate::policy::AcpRegistryPolicy>,
    agents: Query<'w, 's, (&'static RegistryAgent, &'static ChildOf)>,
}

impl Context<'_, '_> {
    fn catalog(&self) -> Option<(Entity, PathBuf, Entity, &Catalog)> {
        let profile = self.profile.entity()?;
        let agents = self.profile.paths()?.agents();
        let (entity, catalog, source) = self.catalogs.single().ok()?;
        let policy = self.policy.single().ok()?;
        if !catalog.ready
            || catalog.loading
            || source.profile != Some(profile)
            || source.cache != agents.join("registry.json")
            || source.url != policy.url
        {
            return None;
        }
        Some((profile, agents, entity, catalog))
    }

    fn accepts(
        &self,
        selection: &Selection,
        store: Option<&ToolStoreTarget>,
        provider: Option<&ToolProviderTarget>,
    ) -> bool {
        if store.is_none_or(|store| store.entity() != selection.store_entity)
            || provider.is_none_or(|provider| provider.entity() != selection.provider)
            || self.stores.get(selection.store_entity).ok() != Some(&selection.store)
            || !self
                .providers
                .get(selection.provider)
                .is_ok_and(|provider| provider.0.id() == "acp")
            || self.profile.entity() != Some(selection.profile)
            || self
                .profile
                .paths()
                .is_none_or(|paths| paths.agents() != selection.agents)
        {
            return false;
        }
        if let Some((expected, revision)) = selection.catalog {
            let Ok((entity, catalog, source)) = self.catalogs.single() else {
                return false;
            };
            let Ok(policy) = self.policy.single() else {
                return false;
            };
            if entity != expected
                || catalog.revision != revision
                || !catalog.ready
                || source.profile != Some(selection.profile)
                || source.cache != selection.agents.join("registry.json")
                || source.url != policy.url
            {
                return false;
            }
        }
        true
    }
}

fn install<R: Clone + Send + Sync + 'static>(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<R>,
            &ToolStoreTarget,
            &ToolProviderTarget,
            &OperationState,
            Has<OperationRetained>,
        ),
        (Without<Selection>, Without<ToolOperationFinished>),
    >,
    context: Context,
    mut commands: Commands,
) where
    ToolOperationKey: From<R>,
{
    for (entity, request, store, provider, state, retained) in &requests {
        if !matches!(state, OperationState::Pending | OperationState::Running)
            || !context
                .providers
                .get(provider.entity())
                .is_ok_and(|provider| provider.0.id() == "acp")
        {
            continue;
        }
        commands
            .entity(entity)
            .remove::<vmux_tool::ExternalToolOperation>()
            .insert(ToolStoreOperation);
        let key = ToolOperationKey::from(request.0.clone());
        if key.provider.id() != "acp" {
            commands.entity(entity).insert((
                ToolOperationFailed("tool provider does not match request".into()),
                ToolOperationFinished,
            ));
            continue;
        }
        let id = key.item_id.trim();
        let Some((profile, agents, catalog_entity, catalog)) = context.catalog() else {
            continue;
        };
        let agent = context
            .agents
            .iter()
            .find(|(agent, parent)| parent.parent() == catalog_entity && agent.id == id)
            .map(|(agent, _)| agent.clone());
        let Some(agent) = agent else {
            commands.entity(entity).insert((
                ToolStoreOperation,
                ToolOperationFinished,
                ToolOperationFailed(format!("agent not in ACP registry: {id}")),
            ));
            continue;
        };
        let Ok(current) = context.stores.get(store.entity()) else {
            continue;
        };
        let native = commands
            .spawn((
                Name::new(format!("Prepare ACP tool {id}")),
                Operation::bundle("acp.tool.prepare").unwrap(),
                OperationRetained,
                OperationTarget::new(entity),
                package::ConfigurationPending,
                package::Request {
                    agent,
                    version: None,
                    store: PackageStore::at(agents.clone()),
                },
            ))
            .id();
        let message = if key.kind == ToolOperationKind::Update {
            format!("{id} updated")
        } else {
            format!("{id} installed")
        };
        commands
            .entity(entity)
            .remove::<vmux_tool::ExternalToolOperation>()
            .insert((
                ToolStoreOperation,
                OperationRetained,
                OperationState::Running,
                Accepted::<R>(PhantomData),
                Preparation(native),
                Selection {
                    store_entity: store.entity(),
                    store: current.clone(),
                    provider: provider.entity(),
                    profile,
                    agents,
                    catalog: Some((catalog_entity, catalog.revision)),
                    id: id.into(),
                    message,
                    release: !retained,
                },
            ));
    }
}

fn uninstall(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<ToolUninstallRequest>,
            &ToolStoreTarget,
            &ToolProviderTarget,
            &OperationState,
            Has<OperationRetained>,
        ),
        (Without<Selection>, Without<ToolOperationFinished>),
    >,
    context: Context,
    mut commands: Commands,
) {
    for (entity, request, store, provider, state, retained) in &requests {
        if !matches!(state, OperationState::Pending | OperationState::Running)
            || !context
                .providers
                .get(provider.entity())
                .is_ok_and(|provider| provider.0.id() == "acp")
        {
            continue;
        }
        commands
            .entity(entity)
            .remove::<vmux_tool::ExternalToolOperation>()
            .insert(ToolStoreOperation);
        if request.0.provider.id() != "acp" {
            commands.entity(entity).insert((
                ToolOperationFailed("tool provider does not match request".into()),
                ToolOperationFinished,
            ));
            continue;
        }
        let name = match PackageName::parse(request.0.id.trim()) {
            Ok(name) => name,
            Err(error) => {
                commands.entity(entity).insert((
                    ToolStoreOperation,
                    ToolOperationFinished,
                    ToolOperationFailed(error),
                ));
                continue;
            }
        };
        let Some(profile) = context.profile.entity() else {
            continue;
        };
        let Some(paths) = context.profile.paths() else {
            continue;
        };
        let agents = paths.agents();
        let Ok(current) = context.stores.get(store.entity()) else {
            continue;
        };
        let native = commands
            .spawn((
                Name::new(format!("Remove ACP tool {}", name.as_str())),
                Operation::bundle("acp.tool.remove").unwrap(),
                OperationRetained,
                OperationTarget::new(entity),
                package::ConfigurationPending,
                package::RemoveRequest {
                    store: PackageStore::at(agents.clone()),
                    name: name.clone(),
                },
            ))
            .id();
        commands
            .entity(entity)
            .remove::<vmux_tool::ExternalToolOperation>()
            .insert((
                ToolStoreOperation,
                OperationRetained,
                OperationState::Running,
                Accepted::<ToolUninstallRequest>(PhantomData),
                Removal(native),
                Selection {
                    store_entity: store.entity(),
                    store: current.clone(),
                    provider: provider.entity(),
                    profile,
                    agents,
                    catalog: None,
                    id: name.as_str().into(),
                    message: format!("{} removed", name.as_str()),
                    release: !retained,
                },
            ));
    }
}

fn cancel<R: Clone + Send + Sync + 'static>(
    mut operations: Query<
        (
            &Selection,
            Option<&ToolOperationRequest<R>>,
            Option<&ToolStoreTarget>,
            Option<&ToolProviderTarget>,
            &mut OperationState,
            Option<&Preparation>,
            Option<&Removal>,
        ),
        With<Accepted<R>>,
    >,
    context: Context,
    mut commands: Commands,
) where
    ToolOperationKey: From<R>,
{
    for (selection, request, store, provider, mut state, preparation, removal) in &mut operations {
        let valid_request = request.is_some_and(|request| {
            let key = ToolOperationKey::from(request.0.clone());
            key.item_id.trim() == selection.id && key.provider.id() == "acp"
        });
        if !context.accepts(selection, store, provider) || !valid_request {
            *state = OperationState::Canceled;
        }
        if *state == OperationState::Canceled
            && let Some(native) = preparation
                .map(|preparation| preparation.0)
                .or_else(|| removal.map(|removal| removal.0))
            && let Ok(mut native) = commands.get_entity(native)
        {
            native.try_insert(OperationState::Canceled);
        }
    }
}

fn cancel_configuration(
    configurations: Query<(Entity, &ConfigurationFor, &OperationState)>,
    owners: Query<(&Configuration, &OperationState, Has<ToolOperationFinished>), With<Selection>>,
    mut commands: Commands,
) {
    for (entity, owner, state) in &configurations {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let current = owners
            .get(owner.0)
            .is_ok_and(|(configuration, state, finished)| {
                configuration.0 == entity
                    && !finished
                    && matches!(state, OperationState::Pending | OperationState::Running)
            });
        if !current {
            commands.entity(entity).insert(OperationState::Canceled);
        }
    }
}

fn prepared(
    operations: Query<(
        Entity,
        &Selection,
        &Preparation,
        &OperationState,
        Option<&ToolStoreTarget>,
        Option<&ToolProviderTarget>,
    )>,
    native: Query<(&package::Prepared, Option<&package::Lease>), With<package::Released>>,
    context: Context,
    mut commands: Commands,
) {
    for (entity, selection, preparation, state, store, provider) in &operations {
        let Ok((result, lease)) = native.get(preparation.0) else {
            continue;
        };
        commands.entity(preparation.0).despawn();
        commands.entity(entity).remove::<Preparation>();
        if *state != OperationState::Running || !context.accepts(selection, store, provider) {
            commands
                .entity(entity)
                .insert((OperationState::Canceled, Done));
            continue;
        }
        if let Err(error) = &result.0 {
            commands.entity(entity).insert((
                Done,
                OperationState::Failed,
                ToolOperationFailed(error.clone()),
                ToolOperationFinished,
            ));
            continue;
        }
        let provider = context.providers.get(selection.provider).unwrap().0.clone();
        let child = commands
            .spawn((
                Name::new("Commit managed ACP package"),
                OperationRetained,
                ConfigurationFor(entity),
                ToolStoreTarget::new(selection.store_entity),
                ToolProviderTarget::new(selection.provider),
                ToolOperationRequest(ToolAdoptRequest {
                    provider,
                    id: selection.id.clone(),
                    value: String::new(),
                }),
            ))
            .id();
        if let Some(lease) = lease {
            commands.entity(child).insert(lease.clone());
        }
        commands.entity(entity).insert(Configuration(child));
    }
}

fn removed(
    operations: Query<(
        Entity,
        &Selection,
        &Removal,
        &OperationState,
        Option<&ToolStoreTarget>,
        Option<&ToolProviderTarget>,
    )>,
    native: Query<(&package::Removed, Option<&package::Lease>), With<package::Released>>,
    context: Context,
    mut commands: Commands,
) {
    for (entity, selection, removal, state, store, provider) in &operations {
        let Ok((result, lease)) = native.get(removal.0) else {
            continue;
        };
        commands.entity(removal.0).despawn();
        commands.entity(entity).remove::<Removal>();
        if *state != OperationState::Running || !context.accepts(selection, store, provider) {
            commands
                .entity(entity)
                .insert((OperationState::Canceled, Done));
            continue;
        }
        if let Err(error) = &result.0 {
            commands.entity(entity).insert((
                Done,
                OperationState::Failed,
                ToolOperationFailed(error.clone()),
                ToolOperationFinished,
            ));
            continue;
        }
        let provider = context.providers.get(selection.provider).unwrap().0.clone();
        let child = commands
            .spawn((
                Name::new("Commit ACP package removal"),
                OperationRetained,
                ConfigurationFor(entity),
                ToolStoreTarget::new(selection.store_entity),
                ToolProviderTarget::new(selection.provider),
                ToolOperationRequest(ToolForgetRequest {
                    provider,
                    id: selection.id.clone(),
                }),
            ))
            .id();
        if let Some(lease) = lease {
            commands.entity(child).insert(lease.clone());
        }
        commands.entity(entity).insert(Configuration(child));
    }
}

fn configured(
    operations: Query<(
        Entity,
        &Selection,
        &Configuration,
        &OperationState,
        Option<&ToolStoreTarget>,
        Option<&ToolProviderTarget>,
    )>,
    configured: Query<
        (
            Option<&ToolOperationSucceeded>,
            Option<&ToolOperationFailed>,
        ),
        With<ToolOperationFinished>,
    >,
    context: Context,
    drain: vmux_tool::ToolPackageDrain,
    mut commands: Commands,
) {
    for (entity, selection, configuration, state, store, provider) in &operations {
        if !drain.is_drained(configuration.0) {
            continue;
        }
        let Ok((success, failure)) = configured.get(configuration.0) else {
            continue;
        };
        commands.entity(configuration.0).despawn();
        commands
            .entity(entity)
            .remove::<Configuration>()
            .insert(Done);
        if *state != OperationState::Running || !context.accepts(selection, store, provider) {
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        }
        let mut operation = commands.entity(entity);
        if success.is_some() {
            operation.insert((
                OperationState::Succeeded,
                ToolOperationSucceeded(selection.message.clone()),
            ));
        } else {
            operation.insert((
                OperationState::Failed,
                ToolOperationFailed(
                    failure
                        .map(|failure| failure.0.clone())
                        .unwrap_or_else(|| "ACP package configuration canceled".into()),
                ),
            ));
        }
        operation.insert(ToolOperationFinished);
    }
}

fn release<R: Send + Sync + 'static>(
    operations: Query<
        (Entity, &Selection, &OperationState),
        (
            With<Done>,
            With<Accepted<R>>,
            Without<Preparation>,
            Without<Removal>,
            Without<Configuration>,
        ),
    >,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, selection, state) in &operations {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let mut operation = commands.entity(entity);
        operation.remove::<(Selection, Done, Accepted<R>)>();
        if selection.release {
            operation.remove::<OperationRetained>();
        }
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn orphaned(
    configurations: Query<(Entity, &ConfigurationFor), With<ToolOperationFinished>>,
    owners: Query<(), With<Selection>>,
    drain: vmux_tool::ToolPackageDrain,
    mut commands: Commands,
) {
    for (entity, owner) in &configurations {
        if !owners.contains(owner.0) && drain.is_drained(entity) {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::registry::{Distribution, PackageDist};
    use super::*;
    use bevy::prelude::{Add, App, Entity, Name, On, Plugin, ResMut, Resource};
    use vmux_ecs::profile::{ActiveProfile, Profile, ProfileDirectories, ProfilePaths};

    #[derive(Resource, Default)]
    struct Effects {
        native: Vec<Entity>,
        configured: Vec<Entity>,
    }

    struct FixtureIoPlugin;

    impl Plugin for FixtureIoPlugin {
        fn build(&self, app: &mut App) {
            app.add_observer(native).add_observer(configuration);
        }
    }

    fn native(trigger: On<Add, package::Request>, mut effects: ResMut<Effects>) {
        let entity = trigger.event_target();
        effects.native.push(entity);
    }

    fn configuration(
        trigger: On<Add, ToolOperationRequest<ToolAdoptRequest>>,
        mut effects: ResMut<Effects>,
    ) {
        effects.configured.push(trigger.event_target());
    }

    struct IoGate {
        release: Vec<std::sync::mpsc::Sender<()>>,
        tasks: Vec<bevy::tasks::Task<()>>,
        _owner: std::sync::MutexGuard<'static, ()>,
    }

    static IO_GATES: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct WakeChannel(std::sync::mpsc::SyncSender<()>);

    impl std::task::Wake for WakeChannel {
        fn wake(self: std::sync::Arc<Self>) {
            let _ = self.0.try_send(());
        }
        fn wake_by_ref(self: &std::sync::Arc<Self>) {
            let _ = self.0.try_send(());
        }
    }

    impl IoGate {
        fn new() -> Self {
            let owner = IO_GATES.lock().unwrap_or_else(|poison| poison.into_inner());
            let pool = bevy::tasks::IoTaskPool::get();
            let (started, entered) = std::sync::mpsc::channel();
            let mut release = Vec::new();
            let mut tasks = Vec::new();
            for _ in 0..pool.thread_num() {
                let (sender, wait) = std::sync::mpsc::channel();
                let started = started.clone();
                tasks.push(pool.spawn(async move {
                    started.send(()).unwrap();
                    wait.recv_timeout(std::time::Duration::from_secs(10))
                        .unwrap();
                }));
                release.push(sender);
            }
            for _ in 0..pool.thread_num() {
                entered
                    .recv_timeout(std::time::Duration::from_secs(3))
                    .unwrap();
            }
            Self {
                release,
                tasks,
                _owner: owner,
            }
        }
    }

    impl Drop for IoGate {
        fn drop(&mut self) {
            for sender in &self.release {
                let _ = sender.send(());
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while self.tasks.iter().any(|task| !task.is_finished()) {
                assert!(
                    std::time::Instant::now() < deadline,
                    "fixture IO did not drain"
                );
                std::thread::yield_now();
            }
        }
    }

    struct Fixture {
        app: App,
        profile: Entity,
        catalog: Entity,
        operation: Entity,
        provider: Entity,
        store: ToolStore,
        gate: Option<IoGate>,
        _directory: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            bevy::tasks::IoTaskPool::get_or_init(|| {
                bevy::tasks::TaskPoolBuilder::new().num_threads(2).build()
            });
            let mut app = App::new();
            app.init_resource::<Effects>().add_plugins((
                bevy::app::TaskPoolPlugin::default(),
                vmux_tool::ToolRuntimePlugin,
                AcpToolPlugin,
                FixtureIoPlugin,
            ));
            let gate = IoGate::new();
            let paths = ProfilePaths::at(Profile::named("test"), directory.path().join("data"));
            let agents = paths.agents();
            let target = match (std::env::consts::OS, std::env::consts::ARCH) {
                ("macos", "aarch64") => "darwin-arm64",
                ("macos", "x86_64") => "darwin-x64",
                ("linux", "aarch64") => "linux-arm64",
                ("linux", "x86_64") => "linux-x64",
                _ => panic!("native ACP fixture requires supported platform"),
            };
            let node = agents
                .join("packages/node")
                .join(format!("node-v22.11.0-{target}"));
            std::fs::create_dir_all(node.join("bin")).unwrap();
            std::fs::create_dir_all(node.join("lib/node_modules/npm/bin")).unwrap();
            std::fs::write(node.join("bin/node"), b"fixture node").unwrap();
            std::fs::write(
                node.join("lib/node_modules/npm/bin/npx-cli.js"),
                b"fixture npx",
            )
            .unwrap();
            let profile = app
                .world_mut()
                .spawn((
                    Name::new("Test"),
                    ActiveProfile(Profile::named("test")),
                    ProfileDirectories(paths),
                ))
                .id();
            let captured = ToolStore::new(
                directory.path().join("config"),
                directory.path().join("home"),
            );
            let mut manifest = vmux_tool::ToolsManifest::default();
            manifest.set_package("other", "preserved", true);
            manifest.write_to(&captured.manifest_path()).unwrap();
            let store = app.world_mut().spawn(captured.clone()).id();
            let provider = app
                .world_mut()
                .spawn((
                    ToolProviderId(vmux_tool::state::ToolProvider::new("acp")),
                    vmux_tool::observation::PackageProvider,
                ))
                .id();
            let mut catalog = Catalog::new();
            catalog.revision = 1;
            catalog.ready = true;
            let catalog = app
                .world_mut()
                .spawn((
                    catalog,
                    CatalogSource {
                        url: "https://example.invalid/registry".into(),
                        cache: agents.join("registry.json"),
                        profile: Some(profile),
                    },
                ))
                .id();
            app.world_mut().spawn(crate::policy::AcpRegistryPolicy {
                url: "https://example.invalid/registry".into(),
            });
            app.world_mut().spawn((
                RegistryAgent {
                    id: "agent".into(),
                    name: "Agent".into(),
                    version: Some("1".into()),
                    description: None,
                    icon: None,
                    distribution: Distribution {
                        binary: None,
                        npx: Some(PackageDist {
                            package: "agent".into(),
                            args: Vec::new(),
                            env: Default::default(),
                        }),
                        uvx: None,
                    },
                },
                ChildOf(catalog),
            ));
            let operation = app
                .world_mut()
                .spawn((
                    Operation::bundle("test.acp.tool").unwrap(),
                    ToolOperationRequest(ToolInstallRequest {
                        provider: vmux_tool::state::ToolProvider::new("acp"),
                        id: "agent".into(),
                    }),
                    ToolStoreTarget::new(store),
                    ToolProviderTarget::new(provider),
                ))
                .id();
            Self {
                app,
                profile,
                catalog,
                operation,
                provider,
                store: captured,
                gate: Some(gate),
                _directory: directory,
            }
        }

        fn notifications(&mut self) -> std::sync::mpsc::Receiver<()> {
            let (wake, events) = std::sync::mpsc::sync_channel(64);
            self.app
                .insert_resource(vmux_ecs::JobWake(std::task::Waker::from(
                    std::sync::Arc::new(WakeChannel(wake)),
                )));
            events
        }

        fn configured(&mut self) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while self.app.world().resource::<Effects>().configured.is_empty() {
                self.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "configuration was not requested"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            for _ in 0..3 {
                self.app.update();
            }
        }

        fn finish(&mut self) {
            self.gate.take();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while self.app.world().get::<Selection>(self.operation).is_some() {
                self.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "ACP operation did not retire"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn acp_tool_success_waits_for_typed_configuration_commit() {
        let mut fixture = Fixture::new();
        fixture.configured();
        assert_eq!(fixture.app.world().resource::<Effects>().native.len(), 1);
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationSucceeded>(fixture.operation)
                .is_none()
        );
        assert!(
            !vmux_tool::ToolsManifest::read(&fixture.store.manifest_path())
                .unwrap()
                .contains("acp", "agent")
        );
        fixture.finish();
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationSucceeded>(fixture.operation)
                .is_some()
        );
        let committed = vmux_tool::ToolsManifest::read(&fixture.store.manifest_path()).unwrap();
        assert!(committed.contains("acp", "agent"));
        assert!(committed.contains("other", "preserved"));
    }

    #[test]
    fn queued_tool_changes_refuse_replaced_profiles_or_catalogs() {
        for change_profile in [true, false] {
            let mut fixture = Fixture::new();
            fixture.configured();
            if change_profile {
                let profile = fixture
                    .app
                    .world()
                    .get::<ActiveProfile>(fixture.profile)
                    .unwrap()
                    .clone();
                let paths = fixture
                    .app
                    .world()
                    .get::<ProfileDirectories>(fixture.profile)
                    .unwrap()
                    .clone();
                fixture.app.world_mut().despawn(fixture.profile);
                fixture
                    .app
                    .world_mut()
                    .spawn((Name::new("Replacement"), profile, paths));
            } else {
                fixture
                    .app
                    .world_mut()
                    .get_mut::<Catalog>(fixture.catalog)
                    .unwrap()
                    .revision += 1;
            }
            fixture.finish();
            assert!(
                !vmux_tool::ToolsManifest::read(&fixture.store.manifest_path())
                    .unwrap()
                    .contains("acp", "agent")
            );
            assert_eq!(
                fixture.app.world().get::<OperationState>(fixture.operation),
                Some(&OperationState::Canceled)
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ToolOperationSucceeded>(fixture.operation)
                    .is_none()
            );
        }
    }

    #[test]
    fn configuration_failure_preserves_manifest_and_refuses_outer_success() {
        let mut fixture = Fixture::new();
        fixture.configured();
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        std::fs::create_dir(fixture.store.brewfile_path()).unwrap();
        fixture.finish();
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationFailed>(fixture.operation)
                .is_some()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationSucceeded>(fixture.operation)
                .is_none()
        );
        for child in &fixture.app.world().resource::<Effects>().configured {
            assert!(fixture.app.world().get_entity(*child).is_err());
        }
    }

    #[test]
    fn configuration_commit_and_retirement_complete_from_native_wakes() {
        let mut fixture = Fixture::new();
        let events = fixture.notifications();
        fixture.configured();
        while events.try_recv().is_ok() {}
        fixture.gate.take();
        while fixture
            .app
            .world()
            .get::<Selection>(fixture.operation)
            .is_some()
        {
            events.recv_timeout(std::time::Duration::from_secs(3)).expect("configuration commit/drain must wake its next frame without another input or timer");
            while events.try_recv().is_ok() {}
            fixture.app.update();
        }
        assert!(
            vmux_tool::ToolsManifest::read(&fixture.store.manifest_path())
                .unwrap()
                .contains("acp", "agent")
        );
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationSucceeded>(fixture.operation)
                .is_some()
        );
    }

    #[test]
    fn revoked_acp_configuration_preserves_manifest_and_native_lease_until_drain() {
        for interruption in [
            "cancel", "delete", "profile", "catalog", "provider", "input",
        ] {
            let mut fixture = Fixture::new();
            fixture.configured();
            let before = std::fs::read(fixture.store.manifest_path()).unwrap();
            let configuration = fixture.app.world().resource::<Effects>().configured[0];
            assert!(
                fixture
                    .app
                    .world()
                    .get::<package::Lease>(configuration)
                    .is_some()
            );
            let agents = fixture
                .app
                .world()
                .get::<ProfileDirectories>(fixture.profile)
                .unwrap()
                .0
                .agents();
            let removal = fixture
                .app
                .world_mut()
                .spawn((
                    Operation::bundle("test.acp.configuration.wait").unwrap(),
                    OperationRetained,
                    package::RemoveRequest {
                        store: PackageStore::at(agents),
                        name: PackageName::parse("agent").unwrap(),
                    },
                ))
                .id();
            match interruption {
                "cancel" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.operation)
                        .insert(OperationState::Canceled);
                }
                "delete" => {
                    fixture.app.world_mut().despawn(fixture.operation);
                }
                "profile" => {
                    fixture.app.world_mut().despawn(fixture.profile);
                }
                "catalog" => {
                    fixture
                        .app
                        .world_mut()
                        .get_mut::<Catalog>(fixture.catalog)
                        .unwrap()
                        .revision += 1;
                }
                "provider" => {
                    fixture.app.world_mut().despawn(fixture.provider);
                }
                "input" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.operation)
                        .insert(ToolOperationRequest(ToolInstallRequest {
                            provider: vmux_tool::state::ToolProvider::new("acp"),
                            id: "replacement".into(),
                        }));
                }
                _ => unreachable!(),
            }
            for _ in 0..3 {
                fixture.app.update();
            }
            assert!(
                fixture.app.world().get_entity(configuration).is_ok(),
                "{interruption}: configuration owner dropped before native drain"
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<package::Lease>(configuration)
                    .is_some(),
                "{interruption}: physical package lease released before manifest IO drain"
            );
            assert!(
                fixture.app.world().get::<package::Lease>(removal).is_none(),
                "{interruption}: overlapping native mutation overtook manifest drain"
            );
            fixture.gate.take();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while fixture
                .app
                .world()
                .get::<package::Released>(removal)
                .is_none()
            {
                fixture.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "revoked configuration did not drain"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert_eq!(
                std::fs::read(fixture.store.manifest_path()).unwrap(),
                before,
                "{interruption}: revoked configuration mutated the manifest"
            );
            assert!(fixture.app.world().get_entity(configuration).is_err());
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ToolOperationSucceeded>(fixture.operation)
                    .is_none()
            );
        }
    }
}
