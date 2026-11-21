use std::marker::PhantomData;
use std::path::PathBuf;

use super::package_path::PackageName;
use super::store::PackageStore;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_ecs::profile::CurrentProfile;
use vmux_ecs::{Operation, OperationRetained, OperationState, OperationTarget};
use vmux_tool::state::{
    ToolAdoptRequest, ToolForgetRequest, ToolInstallRequest, ToolOperationKey, ToolOperationKind,
    ToolUninstallRequest, ToolUpdateRequest,
};
use vmux_tool::{
    ToolOperationFailed, ToolOperationFinished, ToolOperationRequest, ToolOperationRouteSet,
    ToolOperationSucceeded, ToolProviderId, ToolProviderTarget, ToolStore, ToolStoreOperation,
    ToolStoreTarget,
};

use super::catalog::Package;
use super::catalog_runtime::{Catalog, Source as CatalogSource};
use super::{install_runtime as package, removal};

pub(super) struct LspToolPlugin;

impl Plugin for LspToolPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<package::InstallPlugin>() {
            app.add_plugins(package::InstallPlugin);
        }
        app.add_plugins(removal::RemovePlugin)
            .add_systems(
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
                    .before(package::InstallSet)
                    .before(vmux_ecs::JobSet::Submit),
            )
            .add_systems(
                PostUpdate,
                (
                    release::<ToolInstallRequest>,
                    release::<ToolUpdateRequest>,
                    release::<ToolUninstallRequest>,
                    orphaned,
                    wake,
                )
                    .after(vmux_ecs::JobSet::Cleanup),
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
    packages: PathBuf,
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
    packages: Query<'w, 's, (Entity, &'static Package, &'static ChildOf)>,
}

impl Context<'_, '_> {
    fn catalog(&self) -> Option<(Entity, PathBuf, Entity, &Catalog)> {
        let profile = self.profile.entity()?;
        let packages = self.profile.paths()?.lsp();
        let (entity, catalog, source) = self.catalogs.single().ok()?;
        if !catalog.ready
            || catalog.loading
            || source.profile != Some(profile)
            || source.cache != packages.join("registries/registry.json")
        {
            return None;
        }
        Some((profile, packages, entity, catalog))
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
                .is_ok_and(|provider| provider.0.id() == "lsp")
            || self.profile.entity() != Some(selection.profile)
            || self
                .profile
                .paths()
                .is_none_or(|paths| paths.lsp() != selection.packages)
        {
            return false;
        }
        if let Some((expected, revision)) = selection.catalog {
            let Ok((entity, catalog, source)) = self.catalogs.single() else {
                return false;
            };
            if entity != expected
                || catalog.revision != revision
                || !catalog.ready
                || source.profile != Some(selection.profile)
                || source.cache != selection.packages.join("registries/registry.json")
            {
                return false;
            }
        }
        true
    }
}

type InstallRequests<'w, 's, R> = Query<
    'w,
    's,
    (
        Entity,
        &'static ToolOperationRequest<R>,
        &'static ToolStoreTarget,
        &'static ToolProviderTarget,
        &'static OperationState,
        Has<OperationRetained>,
    ),
    (Without<Selection>, Without<ToolOperationFinished>),
>;

fn install<R: Clone + Send + Sync + 'static>(
    requests: InstallRequests<'_, '_, R>,
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
                .is_ok_and(|provider| provider.0.id() == "lsp")
        {
            continue;
        }
        commands
            .entity(entity)
            .remove::<vmux_tool::ExternalToolOperation>()
            .insert(ToolStoreOperation);
        let key = ToolOperationKey::from(request.0.clone());
        if key.provider.id() != "lsp" {
            commands.entity(entity).insert((
                ToolOperationFailed("tool provider does not match request".into()),
                ToolOperationFinished,
            ));
            continue;
        }
        let id = key.item_id.trim();
        let Some((profile, packages, catalog_entity, catalog)) = context.catalog() else {
            continue;
        };
        let package = context
            .packages
            .iter()
            .find(|(_, package, parent)| {
                parent.parent() == catalog_entity && package.name.as_str() == id
            })
            .map(|(entity, _, _)| entity);
        let Some(package_entity) = package else {
            commands.entity(entity).insert((
                ToolStoreOperation,
                ToolOperationFinished,
                ToolOperationFailed(format!("package not in LSP catalog: {id}")),
            ));
            continue;
        };
        let Ok(current) = context.stores.get(store.entity()) else {
            continue;
        };
        let native = commands
            .spawn((
                Name::new(format!("Prepare LSP tool {id}")),
                Operation::bundle("lsp.tool.prepare").unwrap(),
                OperationRetained,
                OperationTarget::new(entity),
                package::ConfigurationPending,
                package::Request {
                    package: package_entity,
                    store: PackageStore::at(packages.clone()),
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
                    packages,
                    catalog: Some((catalog_entity, catalog.revision)),
                    id: id.into(),
                    message,
                    release: !retained,
                },
            ));
    }
}

type UninstallRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ToolOperationRequest<ToolUninstallRequest>,
        &'static ToolStoreTarget,
        &'static ToolProviderTarget,
        &'static OperationState,
        Has<OperationRetained>,
    ),
    (Without<Selection>, Without<ToolOperationFinished>),
>;

fn uninstall(requests: UninstallRequests<'_, '_>, context: Context, mut commands: Commands) {
    for (entity, request, store, provider, state, retained) in &requests {
        if !matches!(state, OperationState::Pending | OperationState::Running)
            || !context
                .providers
                .get(provider.entity())
                .is_ok_and(|provider| provider.0.id() == "lsp")
        {
            continue;
        }
        commands
            .entity(entity)
            .remove::<vmux_tool::ExternalToolOperation>()
            .insert(ToolStoreOperation);
        if request.0.provider.id() != "lsp" {
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
        let packages = paths.lsp();
        let Ok(current) = context.stores.get(store.entity()) else {
            continue;
        };
        let native = commands
            .spawn((
                Name::new(format!("Remove LSP tool {}", name.as_str())),
                Operation::bundle("lsp.tool.remove").unwrap(),
                OperationRetained,
                OperationTarget::new(entity),
                package::ConfigurationPending,
                removal::Request {
                    store: PackageStore::at(packages.clone()),
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
                    packages,
                    catalog: None,
                    id: name.as_str().into(),
                    message: format!("{} removed", name.as_str()),
                    release: !retained,
                },
            ));
    }
}

type CancelOperations<'w, 's, R> = Query<
    'w,
    's,
    (
        &'static Selection,
        Option<&'static ToolOperationRequest<R>>,
        Option<&'static ToolStoreTarget>,
        Option<&'static ToolProviderTarget>,
        &'static mut OperationState,
        Option<&'static Preparation>,
        Option<&'static Removal>,
    ),
    With<Accepted<R>>,
>;

fn cancel<R: Clone + Send + Sync + 'static>(
    mut operations: CancelOperations<'_, '_, R>,
    context: Context,
    mut commands: Commands,
) where
    ToolOperationKey: From<R>,
{
    for (selection, request, store, provider, mut state, preparation, removal) in &mut operations {
        let valid_request = request.is_some_and(|request| {
            let key = ToolOperationKey::from(request.0.clone());
            key.item_id.trim() == selection.id && key.provider.id() == "lsp"
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

type PreparedOperations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Selection,
        &'static Preparation,
        &'static OperationState,
        Option<&'static ToolStoreTarget>,
        Option<&'static ToolProviderTarget>,
    ),
>;

fn prepared(
    operations: PreparedOperations<'_, '_>,
    native: Query<(
        &package::Completed,
        Option<&package::Lease>,
        Has<package::Released>,
    )>,
    context: Context,
    mut commands: Commands,
) {
    for (entity, selection, preparation, state, store, provider) in &operations {
        let Ok((result, lease, released)) = native.get(preparation.0) else {
            continue;
        };
        if !released {
            if let Err(error) = &result.0 {
                commands.entity(entity).insert((
                    ToolOperationFailed(error.clone()),
                    ToolOperationFinished,
                    OperationState::Failed,
                ));
            }
            continue;
        }
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
                Name::new("Commit managed LSP package"),
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

type RemovedOperations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Selection,
        &'static Removal,
        &'static OperationState,
        Option<&'static ToolStoreTarget>,
        Option<&'static ToolProviderTarget>,
    ),
>;

fn removed(
    operations: RemovedOperations<'_, '_>,
    native: Query<(&removal::Completed, Option<&package::Lease>), With<package::Released>>,
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
                Name::new("Commit LSP package removal"),
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

type ConfiguredOperations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Selection,
        &'static Configuration,
        &'static OperationState,
        Option<&'static ToolStoreTarget>,
        Option<&'static ToolProviderTarget>,
    ),
>;

fn configured(
    operations: ConfiguredOperations<'_, '_>,
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
                        .unwrap_or_else(|| "LSP package configuration canceled".into()),
                ),
            ));
        }
        operation.insert(ToolOperationFinished);
    }
}

type ReleaseOperations<'w, 's, R> = Query<
    'w,
    's,
    (Entity, &'static Selection, &'static OperationState),
    (
        With<Done>,
        With<Accepted<R>>,
        Without<Preparation>,
        Without<Removal>,
        Without<Configuration>,
    ),
>;

fn release<R: Send + Sync + 'static>(
    operations: ReleaseOperations<'_, '_, R>,
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

type OrphanedNative<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static OperationTarget, Has<package::Released>),
    Or<(With<package::Request>, With<removal::Request>)>,
>;

fn orphaned(
    configurations: Query<(Entity, &ConfigurationFor), With<ToolOperationFinished>>,
    owners: Query<(), With<Selection>>,
    native: OrphanedNative<'_, '_>,
    drain: vmux_tool::ToolPackageDrain,
    mut commands: Commands,
) {
    for (entity, owner, released) in &native {
        if owners.contains(owner.entity()) {
            continue;
        }
        if released {
            commands.entity(entity).despawn();
        } else {
            commands.entity(entity).insert(OperationState::Canceled);
        }
    }
    for (entity, owner) in &configurations {
        if !owners.contains(owner.0) && drain.is_drained(entity) {
            commands.entity(entity).despawn();
        }
    }
}

type WakeReady<'w, 's> = Query<
    'w,
    's,
    Entity,
    Or<(
        Added<Preparation>,
        Added<Removal>,
        Added<Configuration>,
        Added<Done>,
    )>,
>;

fn wake(ready: WakeReady<'_, '_>, wake: Option<Res<vmux_ecs::JobWake>>) {
    if !ready.is_empty()
        && let Some(wake) = wake
    {
        wake.0.wake_by_ref();
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::Receipt;
    use super::*;
    use bevy::prelude::{
        Add, App, Commands, Entity, Name, On, Plugin, Query, Res, ResMut, Resource, With,
    };
    use std::time::{Duration, Instant};
    use vmux_ecs::profile::{ActiveProfile, Profile, ProfileDirectories, ProfilePaths};
    use vmux_ecs::{JobRequest, OperationState};
    use vmux_tool::io::Execute;
    use vmux_tool::state::ToolProvider;

    #[derive(Resource)]
    struct NpmEndpoint(PathBuf);

    struct NativeFixturePlugin;

    #[derive(Resource, Default)]
    struct Delivery(Vec<(Entity, Entity)>);

    impl Plugin for NativeFixturePlugin {
        fn build(&self, app: &mut App) {
            app.add_observer(npm_endpoint)
                .add_observer(defer_adopt)
                .add_observer(defer_forget)
                .add_observer(invalidate_after_backup);
        }
    }

    fn defer_adopt(
        trigger: On<Add, ToolOperationRequest<ToolAdoptRequest>>,
        requests: Query<&ToolStoreTarget, With<ConfigurationFor>>,
        mut delivery: ResMut<Delivery>,
        mut commands: Commands,
    ) {
        let entity = trigger.event_target();
        if let Ok(target) = requests.get(entity) {
            delivery.0.push((entity, target.entity()));
            commands.entity(entity).remove::<ToolStoreTarget>();
        }
    }

    fn defer_forget(
        trigger: On<Add, ToolOperationRequest<ToolForgetRequest>>,
        requests: Query<&ToolStoreTarget, With<ConfigurationFor>>,
        mut delivery: ResMut<Delivery>,
        mut commands: Commands,
    ) {
        let entity = trigger.event_target();
        if let Ok(target) = requests.get(entity) {
            delivery.0.push((entity, target.entity()));
            commands.entity(entity).remove::<ToolStoreTarget>();
        }
    }

    fn npm_endpoint(
        trigger: On<Add, JobRequest<Execute>>,
        requests: Query<&JobRequest<Execute>>,
        endpoint: Res<NpmEndpoint>,
        mut commands: Commands,
    ) {
        let Ok(request) = requests.get(trigger.event_target()) else {
            return;
        };
        if request.0.program != "npm" {
            return;
        }
        let mut request = request.0.clone();
        request.program = endpoint.0.to_string_lossy().into_owned();
        commands
            .entity(trigger.event_target())
            .insert(JobRequest(request));
    }

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
        store: ToolStore,
        source: Entity,
        provider: Entity,
        profile: Entity,
        catalog: Entity,
        packages: PackageStore,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let program = directory.path().join("fixture-npm");
            std::fs::write(
                &program,
                r##"#!/usr/bin/env python3
import pathlib, sys
root = pathlib.Path(sys.argv[sys.argv.index("--prefix") + 1])
binary = root / "node_modules/.bin/server"
binary.parent.mkdir(parents=True)
binary.write_text("#!/bin/sh\nprintf language-ready")
"##,
            )
            .unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
            bevy::tasks::IoTaskPool::get_or_init(|| {
                bevy::tasks::TaskPoolBuilder::new().num_threads(2).build()
            });
            let mut app = App::new();
            app.insert_resource(NpmEndpoint(program))
                .init_resource::<Delivery>()
                .add_plugins((
                    bevy::app::TaskPoolPlugin::default(),
                    vmux_tool::ToolRuntimePlugin,
                    LspToolPlugin,
                    NativeFixturePlugin,
                ));
            let paths = ProfilePaths::at(Profile::named("test"), directory.path().join("data"));
            let packages = PackageStore::at(paths.lsp());
            let profile = app
                .world_mut()
                .spawn((
                    Name::new("Test"),
                    ActiveProfile(Profile::named("test")),
                    ProfileDirectories(paths),
                ))
                .id();
            let mut catalog = Catalog::new();
            catalog.ready = true;
            catalog.revision = 1;
            let catalog = app
                .world_mut()
                .spawn((
                    catalog,
                    CatalogSource::new(packages.catalog_path(), Some(profile), None),
                ))
                .id();
            app.world_mut().spawn((
                Package {
                    name: PackageName::parse("server").unwrap(),
                    description: String::new(),
                    languages: Vec::new(),
                    categories: Vec::new(),
                    source_id: "pkg:npm/server@1.2.3".into(),
                    assets: Vec::new(),
                    bin: Default::default(),
                },
                ChildOf(catalog),
            ));
            let store = ToolStore::new(
                directory.path().join("config"),
                directory.path().join("home"),
            );
            let mut manifest = vmux_tool::ToolsManifest::default();
            manifest.set_package("other", "preserved", true);
            manifest.write_to(&store.manifest_path()).unwrap();
            let source = app.world_mut().spawn(store.clone()).id();
            let provider = app
                .world_mut()
                .spawn((
                    ToolProviderId(ToolProvider::new("lsp")),
                    vmux_tool::observation::PackageProvider,
                ))
                .id();
            Self {
                app,
                directory,
                store,
                source,
                provider,
                profile,
                catalog,
                packages,
            }
        }

        fn request<R: Send + Sync + 'static>(&mut self, request: R) -> Entity {
            self.app
                .world_mut()
                .spawn((
                    Operation::bundle("lsp.tool.fixture").unwrap(),
                    ToolOperationRequest(request),
                    ToolStoreTarget::new(self.source),
                    ToolProviderTarget::new(self.provider),
                ))
                .id()
        }

        fn configured(&mut self, operation: Entity) -> Entity {
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.app.world().get::<Configuration>(operation).is_none() {
                self.app.update();
                assert!(
                    Instant::now() < deadline,
                    "LSP native preparation did not reach configuration"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            self.app.world().get::<Configuration>(operation).unwrap().0
        }

        fn finish(&mut self, operation: Entity) {
            let pending = std::mem::take(&mut self.app.world_mut().resource_mut::<Delivery>().0);
            for (entity, store) in pending {
                if let Ok(mut request) = self.app.world_mut().get_entity_mut(entity) {
                    request.insert(ToolStoreTarget::new(store));
                }
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                let world = self.app.world_mut();
                if world.get::<Selection>(operation).is_none()
                    && world
                        .query::<&ConfigurationFor>()
                        .iter(world)
                        .next()
                        .is_none()
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "LSP tool configuration did not drain"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn install_and_update_succeed_only_after_the_managed_manifest_commits() {
        for update in [false, true] {
            let mut fixture = Fixture::new();
            let before = std::fs::read(fixture.store.manifest_path()).unwrap();
            let operation = if update {
                fixture.request(ToolUpdateRequest {
                    provider: ToolProvider::new("lsp"),
                    id: "server".into(),
                })
            } else {
                fixture.request(ToolInstallRequest {
                    provider: ToolProvider::new("lsp"),
                    id: "server".into(),
                })
            };
            let configuration = fixture.configured(operation);
            assert_eq!(
                std::fs::read(fixture.store.manifest_path()).unwrap(),
                before
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ToolOperationSucceeded>(operation)
                    .is_none()
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<package::Lease>(configuration)
                    .is_some()
            );
            fixture.finish(operation);
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ToolOperationSucceeded>(operation)
                    .is_some()
            );
            let saved = vmux_tool::ToolsManifest::read(&fixture.store.manifest_path()).unwrap();
            assert!(saved.contains("lsp", "server"));
            assert!(saved.contains("other", "preserved"));
            let binary = fixture
                .packages
                .bin_path(&PackageName::parse("server").unwrap())
                .unwrap();
            let output = std::process::Command::new(binary).output().unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, b"language-ready");
        }
    }

    #[test]
    fn canceled_or_replaced_configuration_preserves_committed_declarations() {
        for interruption in ["cancel", "profile", "catalog", "provider", "requester"] {
            let mut fixture = Fixture::new();
            let before = std::fs::read(fixture.store.manifest_path()).unwrap();
            let operation = fixture.request(ToolInstallRequest {
                provider: ToolProvider::new("lsp"),
                id: "server".into(),
            });
            fixture.configured(operation);
            match interruption {
                "cancel" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(operation)
                        .insert(OperationState::Canceled);
                }
                "profile" => {
                    let paths = ProfilePaths::at(
                        Profile::named("replacement"),
                        fixture.directory.path().join("replacement-data"),
                    );
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.profile)
                        .insert(ProfileDirectories(paths));
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
                "requester" => {
                    fixture.app.world_mut().despawn(operation);
                }
                _ => unreachable!(),
            }
            fixture.app.update();
            fixture.finish(operation);
            assert_eq!(
                std::fs::read(fixture.store.manifest_path()).unwrap(),
                before,
                "{interruption}"
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ToolOperationSucceeded>(operation)
                    .is_none(),
                "{interruption}"
            );
            let world = fixture.app.world_mut();
            assert!(
                world
                    .query::<&package::Lease>()
                    .iter(world)
                    .next()
                    .is_none(),
                "{interruption}"
            );
        }
    }

    #[test]
    fn uninstall_preserves_replaced_bin_entries_and_commits_forget_after_native_removal() {
        let mut fixture = Fixture::new();
        let name = PackageName::parse("server").unwrap();
        std::fs::create_dir_all(fixture.packages.package_dir(&name)).unwrap();
        std::fs::write(
            fixture.packages.package_dir(&name).join("server"),
            b"installed package",
        )
        .unwrap();
        Receipt {
            name: name.clone(),
            version: Some("1".into()),
            source_id: "fixture".into(),
            bin: std::collections::BTreeMap::from([(
                name.clone(),
                super::super::package_path::PackagePath::parse("server").unwrap(),
            )]),
        }
        .write_to(&fixture.packages.package_dir(&name))
        .unwrap();
        std::fs::create_dir_all(fixture.packages.bin_dir()).unwrap();
        std::fs::write(
            fixture.packages.bin_dir().join("server"),
            b"foreign replacement",
        )
        .unwrap();
        let mut manifest = vmux_tool::ToolsManifest::read(&fixture.store.manifest_path()).unwrap();
        manifest.set_package("lsp", "server", true);
        manifest.write_to(&fixture.store.manifest_path()).unwrap();
        let operation = fixture.request(ToolUninstallRequest {
            provider: ToolProvider::new("lsp"),
            id: "server".into(),
        });
        fixture.configured(operation);
        fixture.finish(operation);
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationSucceeded>(operation)
                .is_some()
        );
        assert!(!fixture.packages.package_dir(&name).exists());
        assert_eq!(
            std::fs::read(fixture.packages.bin_dir().join("server")).unwrap(),
            b"foreign replacement"
        );
        let saved = vmux_tool::ToolsManifest::read(&fixture.store.manifest_path()).unwrap();
        assert!(!saved.contains("lsp", "server"));
        assert!(saved.contains("other", "preserved"));
    }

    #[test]
    fn a_pending_configuration_holds_the_package_scope_against_another_native_mutation() {
        let mut fixture = Fixture::new();
        let operation = fixture.request(ToolInstallRequest {
            provider: ToolProvider::new("lsp"),
            id: "server".into(),
        });
        fixture.configured(operation);
        let other = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("lsp.competing.removal").unwrap(),
                OperationRetained,
                removal::Request {
                    store: fixture.packages.clone(),
                    name: PackageName::parse("server").unwrap(),
                },
            ))
            .id();
        for _ in 0..3 {
            fixture.app.update();
        }
        assert!(
            fixture
                .app
                .world()
                .get::<removal::Completed>(other)
                .is_none()
        );
        assert!(
            fixture
                .packages
                .package_dir(&PackageName::parse("server").unwrap())
                .exists()
        );
        fixture
            .app
            .world_mut()
            .entity_mut(other)
            .insert(OperationState::Canceled);
        fixture.app.update();
        fixture.finish(operation);
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationSucceeded>(operation)
                .is_some()
        );
        assert!(
            fixture
                .packages
                .bin_path(&PackageName::parse("server").unwrap())
                .is_some()
        );
    }

    #[derive(Resource)]
    struct Invalidation(Entity);

    fn invalidate_after_backup(
        trigger: On<Add, vmux_ecs::JobResult<super::super::activation_io::Backup>>,
        receipts: Query<&vmux_ecs::JobResult<super::super::activation_io::Backup>>,
        mut catalogs: Query<&mut Catalog>,
        invalidation: Option<Res<Invalidation>>,
    ) {
        let Some(invalidation) = invalidation else {
            return;
        };
        if let Ok(vmux_ecs::JobResult(Ok(Ok(backup)))) = receipts.get(trigger.event_target())
            && backup.0
        {
            catalogs.get_mut(invalidation.0).unwrap().revision += 1;
        }
    }

    #[test]
    fn a_replaced_catalog_after_backup_restores_the_previous_package_before_commit() {
        let mut fixture = Fixture::new();
        fixture.app.insert_resource(Invalidation(fixture.catalog));
        let previous = fixture
            .packages
            .package_dir(&PackageName::parse("server").unwrap());
        std::fs::create_dir_all(&previous).unwrap();
        std::fs::write(previous.join("sentinel"), b"previous package").unwrap();
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        let operation = fixture.request(ToolInstallRequest {
            provider: ToolProvider::new("lsp"),
            id: "server".into(),
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fixture.app.update();
            if fixture.app.world().get::<OperationState>(operation)
                == Some(&OperationState::Canceled)
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "catalog invalidation did not cancel installation"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        fixture.finish(operation);
        assert_eq!(
            std::fs::read(previous.join("sentinel")).unwrap(),
            b"previous package"
        );
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationSucceeded>(operation)
                .is_none()
        );
    }
}
