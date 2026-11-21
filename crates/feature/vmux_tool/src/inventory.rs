use bevy::prelude::*;
use vmux_ecs::{EntityTarget, Operation, OperationRetained, OperationState, OperationTarget};
use vmux_feature::manifest::{FeatureId, FeatureManifest};

use crate::catalog::{Catalog, Received};
use crate::state::{
    ToolOperationKind, ToolProvider, ToolProviderMetadata, ToolStatus, ToolsSnapshot,
};
use crate::storage::{ReadRequest, ReadResult, WriteRequest, WriteResult};
use crate::{
    ToolProviderBinding, ToolProviderId, ToolProviderTarget, ToolStore, ToolTaskSet, ToolsManifest,
};

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::catalog::CatalogPlugin)
            .add_observer(refresh)
            .add_systems(Update, route_import.in_set(crate::ToolOperationRouteSet))
            .add_systems(PostUpdate, observation_wake)
            .configure_sets(
                Update,
                InventorySet
                    .after(ToolTaskSet::Cancel)
                    .before(ToolTaskSet::Start),
            )
            .add_systems(Startup, spawn_registry)
            .add_systems(
                Update,
                (
                    (
                        bind,
                        ApplyDeferred,
                        start_import,
                        retire_imports,
                        request_scan,
                        load_scans,
                        retire_observations,
                        cancel_observation_io,
                        drain_observations,
                        select_scans,
                        start_scans,
                    )
                        .chain(),
                    (
                        refuse_unhandled,
                        reconcile_observations,
                        project_observations,
                        publish_observations,
                        commit_scans,
                        finish_commits,
                        commit_imports,
                        finish_import_commits,
                        drain_imports,
                    )
                        .chain(),
                )
                    .chain()
                    .in_set(InventorySet),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct InventorySet;

#[derive(EntityEvent)]
pub struct RefreshRequest {
    #[event_target]
    pub entity: Entity,
    pub refresh_catalogs: bool,
}

fn refresh(trigger: On<RefreshRequest>, mut registry: Query<(&mut Registry, &mut Catalog)>) {
    let Ok((mut state, mut catalog)) = registry.get_mut(trigger.event().entity) else {
        return;
    };
    if trigger.event().refresh_catalogs || !catalog.loaded {
        state.dirty = true;
        state.refresh_catalogs |= trigger.event().refresh_catalogs;
        state.generation = state.generation.wrapping_add(1);
        if trigger.event().refresh_catalogs && catalog.loaded {
            catalog.loaded = false;
            catalog.revision = catalog.revision.wrapping_add(1);
        }
    }
}

fn bind(
    providers: Query<(Entity, &ToolProviderBinding), Without<ToolProviderId>>,
    manifests: Query<(&FeatureId, &FeatureManifest)>,
    mut commands: Commands,
) {
    for (entity, binding) in &providers {
        let Some(manifest) = manifests
            .iter()
            .find_map(|(feature, manifest)| (*feature == binding.feature()).then_some(manifest))
        else {
            continue;
        };
        let Some(provider) = manifest.tool_providers.get(binding.index()) else {
            continue;
        };
        commands.entity(entity).insert((
            Name::new(format!("{} tool provider", provider.title)),
            ToolProviderId(ToolProvider::new(provider.id.clone())),
        ));
    }
}

fn spawn_registry(registry: Query<Entity, With<Registry>>, mut commands: Commands) {
    if !registry.is_empty() {
        return;
    }
    commands.spawn((
        Name::new("Tool registry"),
        Registry::default(),
        Catalog::default(),
        ToolStore::current(),
        ToolsManifest::default(),
        Sequence::default(),
    ));
}

#[derive(Component)]
pub struct Registry {
    pub(crate) dirty: bool,
    pub(crate) refresh_catalogs: bool,
    pub(crate) generation: u64,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            dirty: false,
            refresh_catalogs: false,
            generation: 1,
        }
    }
}

#[derive(Component)]
pub(crate) struct Scan {
    generation: u64,
    refresh_catalogs: bool,
}

#[derive(Component)]
pub struct Manifest {
    original: Option<ToolsManifest>,
    pub desired: ToolsManifest,
}

#[derive(Component)]
struct ScanSnapshot(ToolsSnapshot);

impl ScanSnapshot {
    fn error(&mut self, error: &str) {
        if !self.0.error.is_empty() {
            self.0.error.push('\n');
        }
        self.0.error.push_str(error);
    }

    fn count(&mut self) {
        let mut installed = 0;
        let mut updates = 0;
        let mut conflicts = 0;
        for category in &self.0.categories {
            for item in &category.items {
                match item.status {
                    ToolStatus::Installed => installed += 1,
                    ToolStatus::Outdated => {
                        installed += 1;
                        updates += 1;
                    }
                    ToolStatus::Conflict => conflicts += 1,
                    _ => {}
                }
            }
        }
        self.0.installed = installed;
        self.0.updates = updates;
        self.0.conflicts = conflicts;
    }

    fn management(&mut self, committed: &ToolsManifest) {
        for category in &mut self.0.categories {
            for item in &mut category.items {
                match item.provider.id() {
                    "dotfiles" => {
                        item.managed = committed.dotfiles.packages.contains(&item.id);
                        if !item.managed {
                            item.operations
                                .retain(|operation| *operation != ToolOperationKind::Unlink);
                        }
                    }
                    "mcp" => {
                        item.managed = committed.mcp.servers.contains_key(&item.id);
                        if !item.managed {
                            item.operations
                                .retain(|operation| *operation != ToolOperationKind::Forget);
                        }
                    }
                    _ => {
                        item.managed = committed.contains(item.provider.id(), &item.id);
                        if item.managed {
                            item.operations
                                .retain(|operation| *operation != ToolOperationKind::Adopt);
                        } else if matches!(
                            item.status,
                            ToolStatus::Installed | ToolStatus::Outdated
                        ) && !item.operations.contains(&ToolOperationKind::Adopt)
                        {
                            item.operations.insert(0, ToolOperationKind::Adopt);
                        }
                    }
                }
            }
        }
    }
}

#[derive(Component)]
struct ScanProvider {
    id: ToolProviderId,
}

#[derive(Component)]
struct Observed;

#[derive(Component, Default)]
pub(crate) struct Sequence(u64);

impl Sequence {
    pub(crate) fn next(&mut self) -> u64 {
        let order = self.0;
        self.0 = self.0.wrapping_add(1);
        order
    }
}

#[derive(Component, Default)]
pub(crate) struct PendingToolOperation;

fn request_scan(
    mut registry: Query<(Entity, &mut Registry, &mut Catalog, &ToolStore)>,
    providers: Query<(Entity, &ToolProviderId), With<crate::observation::Handler>>,
    features: Query<&FeatureManifest>,
    scans: Query<(), With<Scan>>,
    tool_operations: Query<&OperationState, With<EntityTarget<ToolStore>>>,
    pending_tool_operations: Query<(), With<PendingToolOperation>>,
    mut commands: Commands,
) {
    let Ok((registry_entity, mut state, mut catalog, store)) = registry.single_mut() else {
        return;
    };
    if !state.dirty
        || !scans.is_empty()
        || tool_operations
            .iter()
            .any(|state| matches!(state, OperationState::Pending | OperationState::Running))
        || !pending_tool_operations.is_empty()
    {
        return;
    }
    let mut metadata = Vec::new();
    for feature in &features {
        for entry in &feature.tool_providers {
            metadata.push(ToolProviderMetadata {
                provider: ToolProvider::new(entry.id.clone()),
                title: entry.title.clone(),
                title_message_id: entry.title_message_id.clone(),
                route_title: entry.route_title.clone(),
                route_title_message_id: entry.route_title_message_id.clone(),
                short_label: entry.short_label.clone(),
                route: entry.route.clone(),
                rank: entry.rank,
                thumbnails: entry.thumbnails,
                apply: entry.apply,
                brewfile: entry.brewfile,
            });
        }
    }
    metadata.sort_by_key(|entry| entry.rank);
    metadata.dedup_by(|left, right| left.provider == right.provider);
    if catalog.providers != metadata {
        catalog.providers = metadata.clone();
        catalog.revision = catalog.revision.wrapping_add(1);
    }
    let Some(lease) = store.lease() else {
        return;
    };
    let scan = commands
        .spawn((
            Name::new("Tool inventory scan"),
            Operation::bundle("tool.inventory").unwrap().running(),
            OperationTarget::new(registry_entity),
            OperationRetained,
            crate::storage::StoreLease { _lease: lease },
            store.clone(),
            ReadRequest { retain_lease: true },
            Scan {
                generation: state.generation,
                refresh_catalogs: state.refresh_catalogs,
            },
            ScanSnapshot(ToolsSnapshot {
                loaded: true,
                root: store.root().to_string_lossy().into_owned(),
                providers: metadata,
                ..Default::default()
            }),
        ))
        .id();
    for (entity, id) in &providers {
        commands.spawn((
            Name::new(format!("Observe {}", id.0.id())),
            ChildOf(scan),
            ToolProviderTarget::new(entity),
            ScanProvider { id: id.clone() },
        ));
    }
    state.dirty = false;
    state.refresh_catalogs = false;
}

fn load_scans(
    mut scans: Query<
        (
            Entity,
            &ReadResult,
            &OperationState,
            Option<&OperationTarget>,
            &mut ScanSnapshot,
            Option<&ImportSink>,
        ),
        With<Scan>,
    >,
    registry: Query<&ToolsManifest, With<Registry>>,
    mut commands: Commands,
) {
    for (entity, result, state, target, mut snapshot, import) in &mut scans {
        commands.entity(entity).remove::<ReadResult>();
        if import.is_none()
            && (*state != OperationState::Running
                || !target.is_some_and(|target| registry.contains(target.entity())))
        {
            commands.entity(entity).despawn();
            continue;
        }
        let manifest = match &result.0 {
            Ok(manifest) => Manifest {
                original: Some(manifest.clone()),
                desired: manifest.clone(),
            },
            Err(error) => {
                snapshot.error(error);
                Manifest {
                    original: None,
                    desired: target
                        .and_then(|target| registry.get(target.entity()).ok())
                        .cloned()
                        .unwrap_or_default(),
                }
            }
        };
        commands.entity(entity).insert(manifest);
    }
}

#[derive(Component)]
struct Selected;

fn select_scans(
    scans: Query<(Entity, &OperationState), With<Manifest>>,
    pending: Query<
        (
            Entity,
            &ChildOf,
            &ScanProvider,
            Has<crate::observation::Observation>,
            Has<Selected>,
        ),
        Without<Observed>,
    >,
    mut commands: Commands,
) {
    for (entity, state) in &scans {
        let mut selected: Option<(Entity, &ScanProvider)> = None;
        let mut busy = false;
        for (child, parent, provider, observing, chosen) in &pending {
            if parent.parent() != entity {
                continue;
            }
            if observing || chosen {
                busy = true;
                break;
            }
            if *state != OperationState::Running {
                commands.entity(child).despawn();
                continue;
            }
            if selected
                .as_ref()
                .is_none_or(|(_, current)| provider.id.0 < current.id.0)
            {
                selected = Some((child, provider));
            }
        }
        if !busy && let Some((child, _)) = selected {
            commands.entity(child).insert(Selected);
        }
    }
}

fn start_scans(
    selected: Query<(Entity, &ChildOf, Option<&ToolProviderTarget>, &ScanProvider), With<Selected>>,
    scans: Query<(&Scan, &OperationState)>,
    providers: Query<&ToolProviderId, With<crate::observation::Handler>>,
    mut commands: Commands,
) {
    for (child, parent, target, accepted) in &selected {
        commands.entity(child).remove::<Selected>();
        let Ok((scan, state)) = scans.get(parent.parent()) else {
            commands.entity(child).despawn();
            continue;
        };
        if *state != OperationState::Running {
            commands.entity(child).despawn();
            continue;
        }
        commands.entity(child).insert((
            crate::observation::Observation,
            OperationRetained,
            Operation::bundle("tool.provider.observe")
                .unwrap()
                .running(),
            OperationTarget::new(parent.parent()),
        ));
        let Some(target) =
            target.filter(|target| providers.get(target.entity()).ok() == Some(&accepted.id))
        else {
            commands.entity(child).insert(crate::observation::Finished {
                error: Some("tool provider is unavailable".into()),
            });
            continue;
        };
        commands.trigger(crate::observation::ObserveProviderRequest {
            entity: target.entity(),
            observation: child,
            refresh_catalogs: scan.refresh_catalogs,
        });
    }
}

fn refuse_unhandled(
    requests: Query<
        Entity,
        (
            Added<crate::observation::Observation>,
            Without<crate::observation::Accepted>,
            Without<crate::observation::Finished>,
        ),
    >,
    mut commands: Commands,
) {
    for entity in &requests {
        commands
            .entity(entity)
            .insert(crate::observation::Finished {
                error: Some("tool provider did not accept inventory observation".into()),
            });
    }
}

fn retire_observations(
    mut observations: Query<
        (
            Entity,
            &ChildOf,
            Option<&EntityTarget<ToolProviderId>>,
            &ScanProvider,
            &mut OperationState,
        ),
        (With<crate::observation::Observation>, Without<Scan>),
    >,
    scans: Query<
        (
            &Scan,
            Option<&OperationTarget>,
            &ToolStore,
            &OperationState,
            Has<ImportSink>,
        ),
        With<Scan>,
    >,
    registry: Query<(&Registry, &ToolStore)>,
    stores: Query<&ToolStore>,
    providers: Query<&ToolProviderId>,
) {
    for (_, parent, provider, accepted, mut state) in &mut observations {
        let valid = match scans.get(parent.parent()) {
            Ok((scan, target, store, state, import)) if *state == OperationState::Running => {
                target.is_some_and(|target| {
                    if import {
                        stores.get(target.entity()).ok() == Some(store)
                    } else {
                        registry
                            .get(target.entity())
                            .is_ok_and(|(registry, current)| {
                                registry.generation == scan.generation && store == current
                            })
                    }
                }) && provider.and_then(|provider| providers.get(provider.entity()).ok())
                    == Some(&accepted.id)
            }
            _ => false,
        };
        if !valid {
            *state = OperationState::Canceled;
        }
    }
}

fn cancel_observation_io(
    observations: Query<&OperationState, With<crate::observation::Observation>>,
    mut operations: Query<
        (
            &EntityTarget<crate::observation::Observation>,
            &mut OperationState,
        ),
        Without<crate::observation::Observation>,
    >,
) {
    for (target, mut state) in &mut operations {
        if observations.get(target.entity()).ok() != Some(&OperationState::Running) {
            *state = OperationState::Canceled;
        }
    }
}

fn drain_observations(
    observations: Query<
        (Entity, &OperationState),
        (With<crate::observation::Observation>, Without<Observed>),
    >,
    native: crate::observation::InFlight,
    mut commands: Commands,
) {
    for (entity, state) in &observations {
        if *state == OperationState::Canceled && !native.pending(entity) {
            commands.entity(entity).insert(Observed);
        }
    }
}

fn observation_wake(
    changes: Query<Entity, Or<(Added<Observed>, Added<crate::observation::Finished>)>>,
    wake: Option<Res<vmux_ecs::JobWake>>,
) {
    if !changes.is_empty()
        && let Some(wake) = wake
    {
        wake.0.wake_by_ref();
    }
}

#[derive(Component)]
struct Reconciled;

#[derive(Component)]
struct ProjectedCategory(crate::state::ToolCategory);

fn reconcile_observations(
    observations: Query<
        (
            Entity,
            &ChildOf,
            &ScanProvider,
            &OperationState,
            &crate::observation::Finished,
        ),
        (Without<Observed>, Without<Reconciled>),
    >,
    facts: Query<
        (
            &ChildOf,
            &crate::observation::Id,
            &crate::observation::Status,
            Has<crate::observation::Package>,
            Has<crate::observation::Dotfile>,
            Has<crate::observation::Discovered>,
        ),
        With<crate::observation::Item>,
    >,
    mut scans: Query<(&mut Manifest, &mut ScanSnapshot)>,
    mut commands: Commands,
) {
    for (entity, parent, provider, state, finished) in &observations {
        if *state != OperationState::Running {
            continue;
        }
        let Ok((mut manifest, mut snapshot)) = scans.get_mut(parent.parent()) else {
            continue;
        };
        if let Some(error) = &finished.error {
            snapshot.error(&format!("{}: {error}", provider.id.0.id()));
            commands.entity(entity).insert(Observed);
            continue;
        }
        for (owner, id, status, package, dotfile, discovered) in &facts {
            if owner.parent() != entity {
                continue;
            }
            if package && matches!(status.0, ToolStatus::Installed | ToolStatus::Outdated) {
                manifest
                    .desired
                    .set_package(provider.id.0.id(), &id.0, true);
            }
            if dotfile && discovered {
                manifest.desired.set_dotfile_package(&id.0, true);
            }
        }
        commands.entity(entity).insert(Reconciled);
    }
}

fn project_observations(
    observations: Query<
        (Entity, &ChildOf, &ScanProvider, &OperationState),
        (
            With<Reconciled>,
            Without<ProjectedCategory>,
            Without<Observed>,
        ),
    >,
    facts: Query<
        (
            &ChildOf,
            &crate::observation::Id,
            &Name,
            &crate::observation::Version,
            &crate::observation::Icon,
            &crate::observation::Detail,
            &crate::observation::Status,
            &crate::observation::Operations,
            Option<&crate::observation::Removable>,
            Has<crate::observation::Package>,
        ),
        With<crate::observation::Item>,
    >,
    scans: Query<&Manifest>,
    mut commands: Commands,
) {
    for (entity, parent, provider, state) in &observations {
        if *state != OperationState::Running {
            continue;
        }
        let Ok(manifest) = scans.get(parent.parent()) else {
            continue;
        };
        let mut items = Vec::new();
        for (owner, id, name, version, icon, detail, status, operations, removable, package) in
            &facts
        {
            if owner.parent() != entity {
                continue;
            }
            let managed = match provider.id.0.id() {
                "dotfiles" => manifest.desired.dotfiles.packages.contains(&id.0),
                "mcp" => manifest.desired.mcp.servers.contains_key(&id.0),
                _ => manifest.desired.contains(provider.id.0.id(), &id.0),
            };
            let operations = if package {
                crate::observation::Package::operations(
                    status.0,
                    managed,
                    removable.is_some_and(|removable| removable.0),
                )
            } else {
                operations.0.clone()
            };
            items.push(crate::state::ToolItem {
                provider: provider.id.0.clone(),
                id: id.0.clone(),
                name: name.as_str().to_string(),
                version: version.0.clone(),
                icon: icon.0.clone(),
                detail: detail.0.clone(),
                status: status.0,
                managed,
                operations,
            });
        }
        commands
            .entity(entity)
            .insert(ProjectedCategory(crate::state::ToolCategory {
                provider: provider.id.0.clone(),
                items,
            }));
    }
}

fn publish_observations(
    mut observations: Query<
        (
            Entity,
            &ChildOf,
            &OperationState,
            &mut ProjectedCategory,
            Option<&crate::observation::Diagnostics>,
            Option<&EntityTarget<ToolProviderId>>,
        ),
        Without<Observed>,
    >,
    providers: Query<Has<crate::observation::PackageProvider>>,
    mut scans: Query<(&Manifest, &mut ScanSnapshot)>,
    mut commands: Commands,
) {
    for (entity, parent, state, mut category, diagnostics, target) in &mut observations {
        if *state != OperationState::Running {
            continue;
        }
        let Ok((manifest, mut snapshot)) = scans.get_mut(parent.parent()) else {
            continue;
        };
        let package = target
            .and_then(|target| providers.get(target.entity()).ok())
            .unwrap_or(false);
        if package {
            for id in manifest.desired.managed_packages(category.0.provider.id()) {
                if category.0.items.iter().any(|item| item.id == id) {
                    continue;
                }
                let provider = category.0.provider.clone();
                category.0.items.push(crate::state::ToolItem {
                    provider,
                    id: id.clone(),
                    name: id,
                    icon: None,
                    version: None,
                    detail: "Declared in tools.toml".into(),
                    status: ToolStatus::Missing,
                    managed: true,
                    operations: vec![ToolOperationKind::Install, ToolOperationKind::Forget],
                });
            }
        }
        category.0.items.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
                .then_with(|| left.name.cmp(&right.name))
        });
        if let Some(diagnostics) = diagnostics {
            for error in &diagnostics.0 {
                snapshot.error(error);
            }
        }
        snapshot.0.categories.push(category.0.clone());
        commands.entity(entity).insert(Observed);
    }
}

fn commit_scans(
    mut scans: Query<
        (
            Entity,
            &Scan,
            Option<&OperationTarget>,
            &OperationState,
            &ToolStore,
            &Manifest,
            &mut ScanSnapshot,
        ),
        (
            Without<WriteRequest>,
            Without<WriteResult>,
            Without<ImportSink>,
        ),
    >,
    pending: Query<(
        Entity,
        &ChildOf,
        &ScanProvider,
        Option<&EntityTarget<ToolProviderId>>,
        Has<Observed>,
    )>,
    providers: Query<&ToolProviderId>,
    native: crate::observation::InFlight,
    mut registry: Query<(&mut Registry, &ToolStore, &mut ToolsManifest)>,
    mut commands: Commands,
) {
    for (entity, scan, target, state, store, manifest, mut snapshot) in &mut scans {
        if pending.iter().any(|(child, parent, _, _, observed)| {
            parent.parent() == entity && (!observed || native.pending(child))
        }) {
            continue;
        }
        let Some(target) = target else {
            commands.entity(entity).despawn();
            continue;
        };
        let Ok((mut registry, current_store, mut committed)) = registry.get_mut(target.entity())
        else {
            commands.entity(entity).despawn();
            continue;
        };
        if *state != OperationState::Running
            || scan.generation != registry.generation
            || store != current_store
            || pending.iter().any(|(_, parent, provider, target, _)| {
                parent.parent() == entity
                    && target.and_then(|target| providers.get(target.entity()).ok())
                        != Some(&provider.id)
            })
        {
            registry.dirty = true;
            commands.entity(entity).despawn();
            continue;
        }
        if let Some(original) = &manifest.original {
            if original != &manifest.desired {
                commands.entity(entity).insert(WriteRequest {
                    manifest: manifest.desired.clone(),
                    retain_lease: true,
                });
                continue;
            }
            committed.clone_from(&manifest.desired);
        }
        snapshot.management(&committed);
        snapshot.count();
        commands.trigger(Received {
            entity: target.entity(),
            snapshot: snapshot.0.clone(),
        });
        commands.entity(entity).despawn();
    }
}

fn finish_commits(
    mut scans: Query<
        (
            Entity,
            &Scan,
            Option<&OperationTarget>,
            &OperationState,
            &ToolStore,
            &Manifest,
            &mut ScanSnapshot,
            &WriteResult,
        ),
        Without<ImportSink>,
    >,
    observed: Query<(
        &ChildOf,
        &ScanProvider,
        Option<&EntityTarget<ToolProviderId>>,
    )>,
    providers: Query<&ToolProviderId>,
    mut registry: Query<(&mut Registry, &ToolStore, &mut ToolsManifest)>,
    mut commands: Commands,
) {
    for (entity, scan, target, state, store, manifest, mut snapshot, result) in &mut scans {
        commands.entity(entity).despawn();
        let Some(target) = target else {
            continue;
        };
        let Ok((mut registry, current_store, mut committed)) = registry.get_mut(target.entity())
        else {
            continue;
        };
        if *state != OperationState::Running
            || scan.generation != registry.generation
            || store != current_store
            || observed.iter().any(|(parent, provider, target)| {
                parent.parent() == entity
                    && target.and_then(|target| providers.get(target.entity()).ok())
                        != Some(&provider.id)
            })
        {
            registry.dirty = true;
            continue;
        }
        match &result.0 {
            Ok(()) => committed.clone_from(&manifest.desired),
            Err(error) => {
                snapshot.error(&format!("Tools: {error}"));
                if let Some(original) = &manifest.original {
                    committed.clone_from(original);
                }
            }
        }
        snapshot.management(&committed);
        snapshot.count();
        commands.trigger(Received {
            entity: target.entity(),
            snapshot: snapshot.0.clone(),
        });
    }
}

#[derive(Component)]
struct ImportRequest;

#[derive(Component)]
struct ImportWaiting {
    scan: Entity,
}

#[derive(Component)]
struct ImportSink {
    requester: Entity,
    store: Entity,
    provider: Entity,
    id: ToolProviderId,
}

#[derive(Component)]
struct ImportRejected(String);

#[derive(Component)]
struct ImportOutcome(Result<String, String>);

#[derive(bevy::ecs::system::SystemParam)]
struct ImportWitness<'w, 's> {
    requests: Query<
        'w,
        's,
        (
            &'static OperationState,
            Option<&'static EntityTarget<ToolStore>>,
            Option<&'static EntityTarget<ToolProviderId>>,
            &'static ImportWaiting,
            Has<crate::ToolOperationFinished>,
        ),
        Without<Scan>,
    >,
    stores: Query<'w, 's, &'static ToolStore>,
    providers: Query<
        'w,
        's,
        &'static ToolProviderId,
        (
            With<crate::observation::Handler>,
            With<crate::observation::PackageProvider>,
        ),
    >,
}

impl ImportWitness<'_, '_> {
    fn valid(
        &self,
        scan: Entity,
        sink: &ImportSink,
        captured: &ToolStore,
        target: Option<&OperationTarget>,
    ) -> bool {
        let Ok((state, store, provider, waiting, finished)) = self.requests.get(sink.requester)
        else {
            return false;
        };
        *state == OperationState::Running
            && !finished
            && waiting.scan == scan
            && store.is_some_and(|store| store.entity() == sink.store)
            && provider.is_some_and(|provider| provider.entity() == sink.provider)
            && target.is_some_and(|target| target.entity() == sink.store)
            && self.stores.get(sink.store).ok() == Some(captured)
            && self.providers.get(sink.provider).ok() == Some(&sink.id)
    }
}

fn route_import(
    requests: Query<
        (
            Entity,
            &crate::ToolOperationRequest<crate::state::ToolImportRequest>,
            &EntityTarget<ToolProviderId>,
        ),
        Added<EntityTarget<ToolStore>>,
    >,
    providers: Query<
        (),
        (
            With<crate::observation::Handler>,
            With<crate::observation::PackageProvider>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, request, target) in &requests {
        if request.0.value.trim().is_empty() && providers.contains(target.entity()) {
            commands
                .entity(entity)
                .insert((crate::ToolStoreOperation, ImportRequest))
                .remove::<crate::ExternalToolOperation>();
        }
    }
}

fn start_import(
    requests: Query<
        (
            Entity,
            Option<&EntityTarget<ToolStore>>,
            Option<&EntityTarget<ToolProviderId>>,
            &OperationState,
        ),
        (
            With<ImportRequest>,
            Without<ImportWaiting>,
            Without<crate::ToolOperationFinished>,
        ),
    >,
    stores: Query<&ToolStore>,
    providers: Query<
        &ToolProviderId,
        (
            With<crate::observation::Handler>,
            With<crate::observation::PackageProvider>,
        ),
    >,
    mut commands: Commands,
) {
    for (requester, store_target, provider_target, state) in &requests {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let accepted = match (store_target, provider_target) {
            (Some(store), Some(provider)) => {
                match (stores.get(store.entity()), providers.get(provider.entity())) {
                    (Ok(captured), Ok(id)) => {
                        Some((store.entity(), provider.entity(), captured, id))
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        let Some((store_entity, provider_entity, store, id)) = accepted else {
            commands
                .entity(requester)
                .insert((
                    crate::ToolOperationFailed("tool import owner is unavailable".into()),
                    crate::ToolOperationFinished,
                ))
                .remove::<ImportRequest>();
            continue;
        };
        let Some(lease) = store.lease() else {
            continue;
        };
        let scan = commands
            .spawn((
                Name::new("Import installed tools"),
                Operation::bundle("tool.inventory.import")
                    .unwrap()
                    .running(),
                OperationRetained,
                OperationTarget::new(store_entity),
                store.clone(),
                crate::storage::StoreLease { _lease: lease },
                ReadRequest { retain_lease: true },
                Scan {
                    generation: 0,
                    refresh_catalogs: false,
                },
                ScanSnapshot(ToolsSnapshot {
                    loaded: true,
                    root: store.root().to_string_lossy().into_owned(),
                    ..Default::default()
                }),
                ImportSink {
                    requester,
                    store: store_entity,
                    provider: provider_entity,
                    id: id.clone(),
                },
            ))
            .id();
        commands.spawn((
            Name::new("Observe installed tools"),
            ChildOf(scan),
            EntityTarget::<ToolProviderId>::new(provider_entity),
            ScanProvider { id: id.clone() },
        ));
        commands.entity(requester).insert((
            ImportWaiting { scan },
            OperationRetained,
            OperationState::Running,
        ));
    }
}

fn retire_imports(
    mut scans: Query<
        (
            Entity,
            &ImportSink,
            &ToolStore,
            Option<&OperationTarget>,
            &mut OperationState,
        ),
        With<Scan>,
    >,
    witness: ImportWitness,
    mut commands: Commands,
) {
    for (entity, sink, store, target, mut state) in &mut scans {
        if *state == OperationState::Running && !witness.valid(entity, sink, store, target) {
            *state = OperationState::Canceled;
            commands
                .entity(entity)
                .insert(ImportRejected("tool import owner changed".into()));
        }
    }
}

impl Manifest {
    fn imported(&self, provider: &ToolProvider) -> Result<String, String> {
        let Some(original) = &self.original else {
            return Err("tool configuration could not be read".into());
        };
        let imported = self
            .desired
            .managed_packages(provider.id())
            .len()
            .saturating_sub(original.managed_packages(provider.id()).len());
        Ok(format!("imported {imported} {} item(s)", provider.id()))
    }
}

fn commit_imports(
    scans: Query<
        (
            Entity,
            &ImportSink,
            &Manifest,
            &OperationState,
            Option<&OperationTarget>,
            &ToolStore,
            &ScanSnapshot,
        ),
        (
            Without<WriteRequest>,
            Without<WriteResult>,
            Without<ImportOutcome>,
            Without<ReadRequest>,
        ),
    >,
    pending: Query<(Entity, &ChildOf, Has<Observed>), With<ScanProvider>>,
    native: crate::observation::InFlight,
    witness: ImportWitness,
    mut commands: Commands,
) {
    for (entity, sink, manifest, state, target, store, snapshot) in &scans {
        if *state != OperationState::Running
            || !witness.valid(entity, sink, store, target)
            || pending.iter().any(|(child, parent, observed)| {
                parent.parent() == entity && (!observed || native.pending(child))
            })
        {
            continue;
        }
        if !snapshot.0.error.is_empty() {
            commands
                .entity(entity)
                .insert(ImportOutcome(Err(snapshot.0.error.clone())));
            continue;
        }
        let result = manifest.imported(&sink.id.0);
        if result.is_err() || manifest.original.as_ref() == Some(&manifest.desired) {
            commands.entity(entity).insert(ImportOutcome(result));
        } else {
            commands.entity(entity).insert(WriteRequest {
                manifest: manifest.desired.clone(),
                retain_lease: true,
            });
        }
    }
}

fn finish_import_commits(
    scans: Query<(Entity, &ImportSink, &Manifest, &WriteResult), Without<ImportOutcome>>,
    mut commands: Commands,
) {
    for (entity, sink, manifest, result) in &scans {
        let result = match &result.0 {
            Ok(()) => manifest.imported(&sink.id.0),
            Err(error) => Err(error.clone()),
        };
        commands.entity(entity).insert(ImportOutcome(result));
    }
}

fn drain_imports(
    scans: Query<(
        Entity,
        &ImportSink,
        &ToolStore,
        Option<&OperationTarget>,
        &OperationState,
        Option<&Manifest>,
        Option<&ImportOutcome>,
        Option<&ImportRejected>,
        Has<ReadRequest>,
        Has<WriteRequest>,
    )>,
    pending: Query<(Entity, &ChildOf, Has<Observed>), With<ScanProvider>>,
    native: crate::observation::InFlight,
    witness: ImportWitness,
    mut registry: Query<(&mut Registry, &ToolStore, &mut ToolsManifest)>,
    mut changed: MessageWriter<crate::ToolInventoryChanged>,
    mut commands: Commands,
) {
    for (entity, sink, store, target, state, manifest, outcome, rejection, reading, writing) in
        &scans
    {
        if reading
            || writing
            || pending.iter().any(|(child, parent, observed)| {
                parent.parent() == entity && (!observed || native.pending(child))
            })
        {
            continue;
        }
        let valid = *state == OperationState::Running && witness.valid(entity, sink, store, target);
        if valid && outcome.is_none() {
            continue;
        }
        for (mut registry, current, mut committed) in &mut registry {
            if current != store {
                continue;
            }
            if valid
                && outcome.is_some_and(|outcome| outcome.0.is_ok())
                && let Some(manifest) = manifest
            {
                committed.clone_from(&manifest.desired);
            }
            registry.dirty = true;
            registry.generation = registry.generation.wrapping_add(1);
        }
        let Ok((request_state, _, _, waiting, _)) = witness.requests.get(sink.requester) else {
            commands.entity(entity).despawn();
            continue;
        };
        if waiting.scan != entity {
            commands.entity(entity).despawn();
            continue;
        }
        let mut requester = commands.entity(sink.requester);
        requester.remove::<(ImportWaiting, ImportRequest, OperationRetained)>();
        if *request_state == OperationState::Running {
            let result = if valid {
                outcome.unwrap().0.clone()
            } else {
                Err(rejection
                    .map(|error| error.0.clone())
                    .unwrap_or_else(|| "tool import was canceled".into()))
            };
            match result {
                Ok(message) => {
                    requester.insert((
                        crate::package::Published,
                        crate::ToolOperationSucceeded(message),
                    ));
                    changed.write(crate::ToolInventoryChanged(sink.id.0.clone()));
                }
                Err(error) => {
                    requester.insert(crate::ToolOperationFailed(error));
                }
            }
            requester.insert(crate::ToolOperationFinished);
        }
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Inventory;
    use bevy::prelude::{
        App, Commands, Component, Entity, Name, On, Plugin, Query, Res, Resource, Update, With,
    };
    use bevy::tasks::IoTaskPool;
    use std::os::unix::process::ExitStatusExt;

    #[derive(Resource, Clone)]
    struct ImportProcess {
        root: std::path::PathBuf,
    }

    impl vmux_ecs::Job for ImportProcess {
        type Output = Result<Option<crate::process::ProcessOutput>, String>;

        fn run(self) -> Self::Output {
            std::fs::write(self.root.join("started"), "").unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while self.root.join("hold").exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "import fixture was not released"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            if self.root.join("fail").exists() {
                return Err("fixture provider refused the scan".into());
            }
            let stdout = std::fs::read(self.root.join("output.json")).unwrap();
            Ok(Some(crate::process::ProcessOutput(std::process::Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout,
                stderr: Vec::new(),
            })))
        }
    }

    struct ImportProcessPlugin;

    impl Plugin for ImportProcessPlugin {
        fn build(&self, app: &mut App) {
            app.add_plugins(vmux_ecs::JobPlugin::<ImportProcess>::default())
                .add_systems(
                    Update,
                    intercept_import_process
                        .after(ToolTaskSet::Start)
                        .before(vmux_ecs::JobSet::Submit),
                )
                .add_systems(Update, receive_import_process.before(ToolTaskSet::Start));
        }
    }

    fn intercept_import_process(
        requests: Query<(Entity, &vmux_ecs::JobRequest<crate::io::Execute>)>,
        process: Res<ImportProcess>,
        mut commands: Commands,
    ) {
        for (entity, request) in &requests {
            assert_eq!(request.0.program, "npm");
            commands
                .entity(entity)
                .remove::<vmux_ecs::JobRequest<crate::io::Execute>>()
                .insert(vmux_ecs::JobRequest(process.clone()));
        }
    }

    fn receive_import_process(
        mut results: Query<(Entity, &mut vmux_ecs::JobResult<ImportProcess>)>,
        process: Res<ImportProcess>,
        mut commands: Commands,
    ) {
        for (entity, mut result) in &mut results {
            let result = std::mem::replace(&mut result.0, Err("consumed".into()));
            let mut entity = commands.entity(entity);
            entity
                .remove::<vmux_ecs::JobResult<ImportProcess>>()
                .insert(vmux_ecs::JobResult::<crate::io::Execute>(result));
            if process.root.join("diagnostic").exists() {
                entity.insert(crate::observation::Diagnostics(vec![
                    "fixture observation was incomplete".into(),
                ]));
            }
        }
    }

    struct ImportFixture {
        app: App,
        directory: tempfile::TempDir,
        store: ToolStore,
        registry: Entity,
        provider: Entity,
        requester: Entity,
    }

    impl ImportFixture {
        fn new(source: &str) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let store = ToolStore::new(directory.path(), directory.path());
            std::fs::create_dir_all(store.root()).unwrap();
            std::fs::write(store.manifest_path(), source).unwrap();
            std::fs::write(directory.path().join("hold"), "").unwrap();
            std::fs::write(
                directory.path().join("output.json"),
                br#"{"dependencies":{"alpha":{"version":"1"},"beta":{"version":"2"}}}"#,
            )
            .unwrap();
            let mut app = App::new();
            app.add_plugins((
                bevy::app::TaskPoolPlugin::default(),
                crate::ToolRuntimePlugin,
                crate::npm::NpmToolPlugin,
                ImportProcessPlugin,
            ))
            .insert_resource(ImportProcess {
                root: directory.path().into(),
            });
            let registry = app
                .world_mut()
                .spawn((
                    Registry::default(),
                    Catalog::default(),
                    store.clone(),
                    ToolsManifest::default(),
                    Sequence::default(),
                ))
                .id();
            let provider = app
                .world_mut()
                .spawn((
                    crate::npm::NpmProvider,
                    ToolProviderId(ToolProvider::new("npm")),
                    crate::observation::Handler,
                    crate::observation::PackageProvider,
                ))
                .id();
            let requester = app
                .world_mut()
                .spawn((
                    crate::ToolOperationRequest(crate::state::ToolImportRequest {
                        provider: ToolProvider::new("npm"),
                        value: String::new(),
                    }),
                    EntityTarget::<ToolStore>::new(registry),
                    EntityTarget::<ToolProviderId>::new(provider),
                ))
                .id();
            Self {
                app,
                directory,
                store,
                registry,
                provider,
                requester,
            }
        }

        fn wait_started(&mut self) -> Entity {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !self.directory.path().join("started").exists() {
                self.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "import provider did not start"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            self.app
                .world()
                .get::<ImportWaiting>(self.requester)
                .unwrap()
                .scan
        }

        fn drain(&mut self, scan: Entity) {
            std::fs::remove_file(self.directory.path().join("hold")).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while self.app.world().get_entity(scan).is_ok() {
                self.app.update();
                assert!(std::time::Instant::now() < deadline, "import did not drain");
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    }

    #[test]
    fn installed_import_saves_observed_packages_and_preserves_other_declarations() {
        let mut fixture = ImportFixture::new(
            "version = 1\n[packages]\nnpm = ['declared']\nfixture = ['unrelated']\n",
        );
        let scan = fixture.wait_started();
        fixture.drain(scan);
        let saved = ToolsManifest::read(&fixture.store.manifest_path()).unwrap();
        assert_eq!(saved.packages["npm"], ["alpha", "beta", "declared"]);
        assert_eq!(saved.packages["fixture"], ["unrelated"]);
        assert_eq!(
            fixture
                .app
                .world()
                .get::<crate::ToolOperationSucceeded>(fixture.requester)
                .unwrap()
                .0,
            "imported 2 npm item(s)"
        );
        assert_eq!(
            fixture.app.world().get::<OperationState>(fixture.requester),
            Some(&OperationState::Succeeded)
        );
        assert_eq!(
            fixture.app.world().get::<ToolsManifest>(fixture.registry),
            Some(&saved)
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn installed_import_rejects_late_results_after_owner_replacement_or_cancellation() {
        let source = "version = 1\n";
        for interruption in [
            "cancel",
            "provider",
            "provider-target",
            "store-target",
            "store",
            "requester",
            "replacement",
        ] {
            let mut fixture = ImportFixture::new(source);
            let scan = fixture.wait_started();
            match interruption {
                "cancel" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.requester)
                        .insert(OperationState::Canceled);
                }
                "provider" => {
                    fixture.app.world_mut().despawn(fixture.provider);
                }
                "provider-target" => {
                    let replacement = fixture
                        .app
                        .world_mut()
                        .spawn((
                            ToolProviderId(ToolProvider::new("npm")),
                            crate::observation::Handler,
                            crate::observation::PackageProvider,
                        ))
                        .id();
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.requester)
                        .insert(EntityTarget::<ToolProviderId>::new(replacement));
                }
                "store-target" => {
                    let replacement = fixture.app.world_mut().spawn(fixture.store.clone()).id();
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.requester)
                        .insert(EntityTarget::<ToolStore>::new(replacement));
                }
                "store" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.registry)
                        .insert(ToolStore::new(
                            fixture.directory.path().join("replacement"),
                            fixture.directory.path(),
                        ));
                }
                "requester" => {
                    fixture.app.world_mut().despawn(fixture.requester);
                }
                "replacement" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.requester)
                        .insert(ImportWaiting {
                            scan: Entity::PLACEHOLDER,
                        });
                }
                _ => unreachable!(),
            }
            for _ in 0..4 {
                fixture.app.update();
            }
            assert!(
                fixture.app.world().get_entity(scan).is_ok(),
                "{interruption}: native scan must remain owned"
            );
            assert!(
                fixture.store.lease().is_none(),
                "{interruption}: lease released before IO drain"
            );
            fixture.drain(scan);
            assert_eq!(
                std::fs::read_to_string(fixture.store.manifest_path()).unwrap(),
                source,
                "{interruption}"
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<crate::ToolOperationSucceeded>(fixture.requester)
                    .is_none(),
                "{interruption}"
            );
            assert!(fixture.store.lease().is_some(), "{interruption}");
            if interruption == "replacement" {
                assert_eq!(
                    fixture
                        .app
                        .world()
                        .get::<ImportWaiting>(fixture.requester)
                        .unwrap()
                        .scan,
                    Entity::PLACEHOLDER
                );
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<OperationRetained>(fixture.requester)
                        .is_some()
                );
            } else if fixture.app.world().get_entity(fixture.requester).is_ok() {
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<OperationRetained>(fixture.requester)
                        .is_none(),
                    "{interruption}"
                );
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<ImportWaiting>(fixture.requester)
                        .is_none(),
                    "{interruption}"
                );
            }
        }
    }

    #[test]
    fn queued_import_writes_refuse_replaced_owners_and_preserve_committed_configuration() {
        for interruption in ["cancel", "requester", "provider", "store"] {
            let mut fixture = ImportFixture::new("version = 1\n");
            let scan = fixture.wait_started();
            std::fs::remove_file(fixture.directory.path().join("hold")).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while fixture.app.world().get::<WriteRequest>(scan).is_none() {
                fixture.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "import did not accept persistence"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            assert!(fixture.store.lease().is_none());
            match interruption {
                "cancel" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.requester)
                        .insert(OperationState::Canceled);
                }
                "requester" => {
                    fixture.app.world_mut().despawn(fixture.requester);
                }
                "provider" => {
                    fixture.app.world_mut().despawn(fixture.provider);
                }
                "store" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.registry)
                        .insert(ToolStore::new(
                            fixture.directory.path().join("replacement"),
                            fixture.directory.path(),
                        ));
                }
                _ => unreachable!(),
            }
            while fixture.app.world().get_entity(scan).is_ok() {
                fixture.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "accepted import write did not drain"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            let saved = ToolsManifest::read(&fixture.store.manifest_path()).unwrap();
            assert!(!saved.contains("npm", "alpha"), "{interruption}");
            assert!(
                fixture
                    .app
                    .world()
                    .get::<crate::ToolOperationSucceeded>(fixture.requester)
                    .is_none(),
                "{interruption}"
            );
            assert!(
                !fixture
                    .app
                    .world()
                    .get::<ToolsManifest>(fixture.registry)
                    .unwrap()
                    .contains("npm", "alpha"),
                "{interruption}"
            );
            if interruption != "store" {
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<Registry>(fixture.registry)
                        .unwrap()
                        .dirty,
                    "{interruption}"
                );
            }
            assert!(fixture.store.lease().is_some(), "{interruption}");
        }
    }

    #[test]
    fn installed_import_reports_provider_errors_and_diagnostics_without_partial_saves() {
        let source = "version = 1\n";
        for failure in ["fail", "diagnostic"] {
            let mut fixture = ImportFixture::new(source);
            let scan = fixture.wait_started();
            std::fs::write(fixture.directory.path().join(failure), "").unwrap();
            fixture.drain(scan);
            assert_eq!(
                std::fs::read_to_string(fixture.store.manifest_path()).unwrap(),
                source
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<crate::ToolOperationFailed>(fixture.requester)
                    .unwrap()
                    .0
                    .contains("fixture")
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<crate::ToolOperationSucceeded>(fixture.requester)
                    .is_none()
            );
            assert!(fixture.store.lease().is_some());
        }
    }

    #[test]
    fn installed_import_reports_failed_persistence_and_preserves_committed_state() {
        let mut fixture = ImportFixture::new("version = 1\n");
        let scan = fixture.wait_started();
        std::fs::remove_file(fixture.store.manifest_path()).unwrap();
        std::fs::create_dir(fixture.store.manifest_path()).unwrap();
        fixture.drain(scan);
        assert!(
            fixture
                .app
                .world()
                .get::<crate::ToolOperationFailed>(fixture.requester)
                .is_some()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<crate::ToolOperationSucceeded>(fixture.requester)
                .is_none()
        );
        assert!(
            !fixture
                .app
                .world()
                .get::<ToolsManifest>(fixture.registry)
                .unwrap()
                .contains("npm", "alpha")
        );
        assert!(fixture.store.lease().is_some());
    }

    #[derive(Component)]
    struct FixtureProvider;

    #[derive(Clone)]
    struct FixtureRead {
        root: std::path::PathBuf,
    }

    impl vmux_ecs::Job for FixtureRead {
        type Output = Result<(), String>;

        fn run(self) -> Self::Output {
            std::fs::write(self.root.join("started"), "").unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while self.root.join("hold").exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "fixture observation timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            if self.root.join("fail").exists() {
                return Err("fixture provider failed".into());
            }
            Ok(())
        }
    }

    struct FixtureObservationPlugin;

    impl Plugin for FixtureObservationPlugin {
        fn build(&self, app: &mut App) {
            app.add_plugins(vmux_ecs::JobPlugin::<FixtureRead>::default())
                .add_observer(observe_fixture)
                .add_systems(Update, receive_fixture.in_set(ToolTaskSet::Start));
        }
    }

    fn observe_fixture(
        trigger: On<crate::observation::ObserveProviderRequest>,
        providers: Query<(), With<FixtureProvider>>,
        observations: Query<&ChildOf, With<crate::observation::Observation>>,
        scans: Query<&ToolStore, With<Scan>>,
        mut commands: Commands,
    ) {
        let request = trigger.event();
        if !providers.contains(request.entity) {
            return;
        }
        let store = scans
            .get(observations.get(request.observation).unwrap().parent())
            .unwrap();
        commands.entity(request.observation).insert((
            crate::observation::Accepted,
            vmux_ecs::JobRequest(FixtureRead { root: store.root() }),
        ));
    }

    fn receive_fixture(
        results: Query<(
            Entity,
            &ScanProvider,
            &vmux_ecs::JobResult<FixtureRead>,
            &OperationState,
        )>,
        mut commands: Commands,
    ) {
        for (entity, provider, result, state) in &results {
            if *state != OperationState::Running {
                continue;
            }
            commands
                .entity(entity)
                .remove::<vmux_ecs::JobResult<FixtureRead>>();
            match &result.0 {
                Err(error) | Ok(Err(error)) => {
                    commands
                        .entity(entity)
                        .insert(crate::observation::Finished {
                            error: Some(error.clone()),
                        });
                    continue;
                }
                Ok(Ok(())) => {}
            }
            let operations = match provider.id.0.id() {
                "dotfiles" => vec![ToolOperationKind::Link, ToolOperationKind::Unlink],
                "mcp" => vec![ToolOperationKind::Forget],
                _ => vec![ToolOperationKind::Uninstall],
            };
            let item = commands
                .spawn((
                    ChildOf(entity),
                    crate::observation::ObservedTool::new(
                        "observed".into(),
                        "Observed".into(),
                        Some("1".into()),
                        Some("fixture.svg".into()),
                        String::new(),
                        ToolStatus::Installed,
                        operations,
                    ),
                ))
                .id();
            if !matches!(provider.id.0.id(), "dotfiles" | "mcp") {
                commands.entity(item).insert((
                    crate::observation::Package,
                    crate::observation::Removable(true),
                ));
            }
            commands
                .entity(entity)
                .insert(crate::observation::Finished { error: None });
        }
    }

    struct InventoryFixture {
        app: App,
        directory: tempfile::TempDir,
        registry: Entity,
        provider: Entity,
    }

    impl InventoryFixture {
        fn new(source: &str, hold: bool, provider: &str) -> Self {
            IoTaskPool::get_or_init(|| bevy::tasks::TaskPoolBuilder::new().num_threads(2).build());
            let directory = tempfile::tempdir().unwrap();
            let store = ToolStore::new(directory.path(), directory.path());
            std::fs::create_dir_all(store.root()).unwrap();
            std::fs::write(store.manifest_path(), source).unwrap();
            if hold {
                std::fs::write(store.root().join("hold"), "").unwrap();
            }
            let mut app = App::new();
            app.add_plugins((crate::ToolRuntimePlugin, FixtureObservationPlugin));
            let registry = app
                .world_mut()
                .spawn((
                    Name::new("Fixture tool registry"),
                    Registry::default(),
                    Catalog::default(),
                    store,
                    ToolsManifest::default(),
                    Sequence::default(),
                ))
                .id();
            let provider = app
                .world_mut()
                .spawn((
                    ToolProviderId(ToolProvider::new(provider)),
                    FixtureProvider,
                    crate::observation::Handler,
                ))
                .id();
            app.world_mut().trigger(RefreshRequest {
                entity: registry,
                refresh_catalogs: false,
            });
            app.update();
            Self {
                app,
                directory,
                registry,
                provider,
            }
        }

        fn scan(&mut self) -> Entity {
            self.app
                .world_mut()
                .query_filtered::<Entity, With<Scan>>()
                .single(self.app.world())
                .unwrap()
        }

        fn wait_started(&mut self) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !self.directory.path().join("tools/started").exists() {
                self.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "fixture scan did not start"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        fn drain(&mut self, entity: Entity) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while self.app.world().get_entity(entity).is_ok() {
                self.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "fixture scan did not drain"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        fn snapshot(&mut self) -> ToolsSnapshot {
            let mut state = bevy::ecs::system::SystemState::<Inventory>::new(self.app.world_mut());
            state
                .get(self.app.world())
                .unwrap()
                .snapshot(self.registry)
                .unwrap()
        }
    }

    struct ScanWake {
        output: std::sync::mpsc::SyncSender<()>,
        source: vmux_ecs::JobWake,
    }

    impl std::task::Wake for ScanWake {
        fn wake(self: std::sync::Arc<Self>) {
            self.source.0.wake_by_ref();
            let _ = self.output.try_send(());
        }

        fn wake_by_ref(self: &std::sync::Arc<Self>) {
            self.source.0.wake_by_ref();
            let _ = self.output.try_send(());
        }
    }

    #[test]
    fn completed_provider_observation_wakes_the_next_inventory_step() {
        IoTaskPool::get_or_init(|| bevy::tasks::TaskPoolBuilder::new().num_threads(2).build());
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(directory.path(), directory.path());
        std::fs::create_dir_all(store.root()).unwrap();
        std::fs::write(store.manifest_path(), "version = 1\n").unwrap();
        std::fs::write(store.root().join("hold"), "").unwrap();
        let (sender, receiver) = std::sync::mpsc::sync_channel(32);
        let mut app = App::new();
        app.add_plugins((crate::ToolRuntimePlugin, FixtureObservationPlugin));
        let source = app
            .world()
            .get_resource::<vmux_ecs::JobWake>()
            .cloned()
            .unwrap_or_else(|| vmux_ecs::JobWake(std::task::Waker::noop().clone()));
        app.insert_resource(vmux_ecs::JobWake(std::task::Waker::from(
            std::sync::Arc::new(ScanWake {
                output: sender,
                source,
            }),
        )));
        let registry = app
            .world_mut()
            .spawn((
                Registry::default(),
                Catalog::default(),
                store.clone(),
                ToolsManifest::default(),
                Sequence::default(),
            ))
            .id();
        for provider in ["alpha", "beta"] {
            app.world_mut().spawn((
                ToolProviderId(ToolProvider::new(provider)),
                FixtureProvider,
                crate::observation::Handler,
            ));
        }
        app.world_mut().trigger(RefreshRequest {
            entity: registry,
            refresh_catalogs: false,
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !store.root().join("started").exists() {
            app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        while receiver.try_recv().is_ok() {}
        std::fs::remove_file(store.root().join("hold")).unwrap();
        while !app.world().get::<Catalog>(registry).unwrap().loaded {
            receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("completed observation must wake its continuation");
            app.update();
            assert!(std::time::Instant::now() < deadline);
        }
        assert!(!app.world().get::<ToolStore>(registry).unwrap().is_busy());
    }

    #[test]
    fn inventory_management_uses_dotfile_and_mcp_declarations_without_package_adoption() {
        for (provider, source, managed, operations) in [
            (
                "dotfiles",
                "version = 1\n[dotfiles]\npackages = ['observed']\n",
                true,
                vec![ToolOperationKind::Link, ToolOperationKind::Unlink],
            ),
            (
                "dotfiles",
                "version = 1\n",
                false,
                vec![ToolOperationKind::Link],
            ),
            (
                "mcp",
                "version = 1\n[mcp.servers.observed]\ntransport = 'stdio'\ncommand = 'server'\n",
                true,
                vec![ToolOperationKind::Forget],
            ),
            ("mcp", "version = 1\n", false, Vec::new()),
        ] {
            let mut fixture = InventoryFixture::new(source, false, provider);
            let scan = fixture.scan();
            fixture.drain(scan);
            let snapshot = fixture.snapshot();
            let item = &snapshot.categories[0].items[0];
            assert_eq!(item.managed, managed, "{provider}");
            assert_eq!(item.operations, operations, "{provider}");
            assert!(snapshot.error.is_empty(), "{provider}");
        }
    }

    #[test]
    fn inventory_commits_observed_packages_after_read_and_preserves_existing_declarations() {
        let mut fixture = InventoryFixture::new(
            "version = 1\n[packages]\nfixture = ['existing']\n",
            false,
            "fixture",
        );
        let scan = fixture.scan();
        fixture.drain(scan);
        let store = fixture
            .app
            .world()
            .get::<ToolStore>(fixture.registry)
            .unwrap();
        let saved: ToolsManifest =
            toml::from_str(&std::fs::read_to_string(store.manifest_path()).unwrap()).unwrap();
        assert!(saved.contains("fixture", "existing"));
        assert!(saved.contains("fixture", "observed"));
        assert!(store.lease().is_some());
        assert!(
            fixture
                .app
                .world()
                .get::<ToolsManifest>(fixture.registry)
                .unwrap()
                .contains("fixture", "observed")
        );
        let snapshot = fixture.snapshot();
        assert_eq!(snapshot.installed, 1);
        assert!(snapshot.categories[0].items[0].managed);
        assert!(snapshot.error.is_empty());
    }

    #[test]
    fn obsolete_inventory_results_cannot_save_or_transfer_to_replacement_owners() {
        for interruption in ["generation", "store", "registry", "provider", "cancel"] {
            let source = "version = 1\n[packages]\nfixture = ['existing']\n";
            let mut fixture = InventoryFixture::new(source, true, "fixture");
            let scan = fixture.scan();
            fixture.wait_started();
            let store = fixture
                .app
                .world()
                .get::<ToolStore>(fixture.registry)
                .unwrap()
                .clone();
            assert!(store.lease().is_none());
            match interruption {
                "generation" => {
                    fixture
                        .app
                        .world_mut()
                        .get_mut::<Registry>(fixture.registry)
                        .unwrap()
                        .generation += 1;
                }
                "store" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.registry)
                        .insert(ToolStore::new(
                            fixture.directory.path().join("replacement"),
                            fixture.directory.path(),
                        ));
                }
                "registry" => {
                    fixture.app.world_mut().despawn(fixture.registry);
                }
                "provider" => {
                    fixture.app.world_mut().despawn(fixture.provider);
                }
                "cancel" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(scan)
                        .insert(OperationState::Canceled);
                }
                _ => unreachable!(),
            }
            fixture.app.update();
            assert!(store.lease().is_none(), "{interruption}");
            std::fs::remove_file(store.root().join("hold")).unwrap();
            fixture.drain(scan);
            assert_eq!(
                std::fs::read_to_string(store.manifest_path()).unwrap(),
                source,
                "{interruption}"
            );
            assert!(store.lease().is_some(), "{interruption}");
            if let Some(manifest) = fixture.app.world().get::<ToolsManifest>(fixture.registry) {
                assert!(!manifest.contains("fixture", "observed"), "{interruption}");
            }
        }
    }

    #[test]
    fn unreadable_inventory_never_overwrites_configuration_or_claims_managed_packages() {
        let source = "invalid = [";
        let mut fixture = InventoryFixture::new(source, false, "fixture");
        let scan = fixture.scan();
        fixture.drain(scan);
        let store = fixture
            .app
            .world()
            .get::<ToolStore>(fixture.registry)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(store.manifest_path()).unwrap(),
            source
        );
        let snapshot = fixture.snapshot();
        assert!(!snapshot.error.is_empty());
        let item = &snapshot.categories[0].items[0];
        assert!(!item.managed);
        assert!(item.operations.contains(&ToolOperationKind::Adopt));
    }

    #[test]
    fn failed_inventory_providers_cannot_commit_partial_manifest_changes() {
        let source = "version = 1\n";
        let mut fixture = InventoryFixture::new(source, true, "fixture");
        let scan = fixture.scan();
        fixture.wait_started();
        let store = fixture
            .app
            .world()
            .get::<ToolStore>(fixture.registry)
            .unwrap()
            .clone();
        std::fs::write(store.root().join("fail"), "").unwrap();
        std::fs::remove_file(store.root().join("hold")).unwrap();
        fixture.drain(scan);
        assert_eq!(
            std::fs::read_to_string(store.manifest_path()).unwrap(),
            source
        );
        assert!(
            !fixture
                .app
                .world()
                .get::<ToolsManifest>(fixture.registry)
                .unwrap()
                .contains("fixture", "observed")
        );
        let snapshot = fixture.snapshot();
        assert!(snapshot.error.contains("fixture provider failed"));
        assert!(snapshot.categories.is_empty());
    }

    #[test]
    fn queued_inventory_writes_refuse_cancellation_and_drain_before_releasing_the_store_lease() {
        for interruption in ["generation", "registry", "provider", "store", "cancel"] {
            let mut fixture = InventoryFixture::new("version = 1\n", false, "fixture");
            let scan = fixture.scan();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while fixture.app.world().get::<WriteRequest>(scan).is_none() {
                fixture.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "scan did not request a write"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            let store = fixture
                .app
                .world()
                .get::<ToolStore>(fixture.registry)
                .unwrap()
                .clone();
            assert!(store.lease().is_none(), "{interruption}");
            match interruption {
                "generation" => {
                    fixture
                        .app
                        .world_mut()
                        .get_mut::<Registry>(fixture.registry)
                        .unwrap()
                        .generation += 1;
                }
                "registry" => {
                    fixture.app.world_mut().despawn(fixture.registry);
                }
                "provider" => {
                    fixture.app.world_mut().despawn(fixture.provider);
                }
                "store" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.registry)
                        .insert(ToolStore::new(
                            fixture.directory.path().join("replacement"),
                            fixture.directory.path(),
                        ));
                }
                "cancel" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(scan)
                        .insert(OperationState::Canceled);
                }
                _ => unreachable!(),
            }
            fixture.drain(scan);
            assert!(store.lease().is_some(), "{interruption}");
            if let Some(manifest) = fixture.app.world().get::<ToolsManifest>(fixture.registry) {
                assert!(!manifest.contains("fixture", "observed"), "{interruption}");
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<Registry>(fixture.registry)
                        .unwrap()
                        .dirty
                );
            }
            let saved: ToolsManifest =
                toml::from_str(&std::fs::read_to_string(store.manifest_path()).unwrap()).unwrap();
            assert_eq!(
                saved.contains("fixture", "observed"),
                interruption != "cancel",
                "{interruption}"
            );
        }
    }
}
