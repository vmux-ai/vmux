use std::path::PathBuf;

use crate::state::{
    ToolAdoptRequest, ToolImportRequest, ToolInstallRequest, ToolLinkRequest, ToolUninstallRequest,
    ToolUnlinkRequest, ToolUpdateRequest,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Added, Commands, Component as EcsComponent, Entity, Query, With, Without};
use bevy_ecs::schedule::IntoScheduleConfigs;
use serde::{Deserialize, Serialize};

use crate::{
    ToolOperationFinished, ToolOperationRequest, ToolOperationRouteSet, ToolOperationSucceeded,
    ToolProviderBinding, ToolProviderTarget, ToolStoreOperation, ToolStoreTarget, ToolTaskSet,
};

pub(crate) struct DotfileToolPlugin;

#[derive(EcsComponent)]
struct DotfileProvider;

impl Plugin for DotfileToolPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(host)]
        app.add_plugins((
            crate::dotfile_inventory::DotfileInventoryPlugin,
            crate::dotfile_ops::DotfileOperationsPlugin,
            crate::dotfile_staging::DotfileImportPlugin,
        ));
        app.add_systems(Startup, spawn_provider)
            .add_systems(
                Update,
                (
                    route_import,
                    route_adopt,
                    route_install,
                    route_update,
                    route_link,
                    route_uninstall,
                    route_unlink,
                )
                    .in_set(ToolOperationRouteSet),
            )
            .add_systems(
                Update,
                (
                    complete_import,
                    complete_available_import,
                    complete_link,
                    complete_disable,
                    complete_adoption,
                )
                    .after(ToolTaskSet::Apply),
            );
    }
}

fn spawn_provider(mut commands: Commands) {
    let provider = commands
        .spawn((
            ToolProviderBinding::new::<crate::Feature>(4),
            DotfileProvider,
        ))
        .id();
    #[cfg(host)]
    commands
        .entity(provider)
        .insert(crate::observation::Handler);
    #[cfg(not(host))]
    let _ = provider;
}

fn route_import(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<ToolImportRequest>,
            &ToolProviderTarget,
        ),
        Added<ToolStoreTarget>,
    >,
    providers: Query<(), With<DotfileProvider>>,
    mut commands: Commands,
) {
    for (entity, operation, provider) in &requests {
        if !providers.contains(provider.entity()) {
            continue;
        }
        let request = &operation.0;
        let value = request.value.trim();
        let mut entity = commands.entity(entity);
        if value.is_empty() {
            entity.insert((ToolStoreOperation, ImportAvailableDotfiles));
        } else {
            entity.insert((ToolStoreOperation, ImportDotfiles::new(value)));
        }
    }
}

fn route_adopt(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<ToolAdoptRequest>,
            &ToolProviderTarget,
        ),
        Added<ToolStoreTarget>,
    >,
    providers: Query<(), With<DotfileProvider>>,
    mut commands: Commands,
) {
    for (entity, operation, provider) in &requests {
        if !providers.contains(provider.entity()) {
            continue;
        }
        let request = &operation.0;
        if request.id.trim().is_empty() || request.value.trim().is_empty() {
            continue;
        }
        commands.entity(entity).insert((
            ToolStoreOperation,
            AdoptDotfile::new(request.value.trim(), request.id.trim()),
        ));
    }
}

fn route_install(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<ToolInstallRequest>,
            &ToolProviderTarget,
        ),
        Added<ToolStoreTarget>,
    >,
    providers: Query<(), With<DotfileProvider>>,
    mut commands: Commands,
) {
    for (entity, operation, provider) in &requests {
        if !providers.contains(provider.entity()) {
            continue;
        }
        let request = &operation.0;
        if !request.id.trim().is_empty() {
            commands.entity(entity).insert((
                ToolStoreOperation,
                LinkDotfilePackage::new(request.id.trim()),
            ));
        }
    }
}

fn route_update(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<ToolUpdateRequest>,
            &ToolProviderTarget,
        ),
        Added<ToolStoreTarget>,
    >,
    providers: Query<(), With<DotfileProvider>>,
    mut commands: Commands,
) {
    for (entity, operation, provider) in &requests {
        if !providers.contains(provider.entity()) {
            continue;
        }
        let request = &operation.0;
        if !request.id.trim().is_empty() {
            commands.entity(entity).insert((
                ToolStoreOperation,
                LinkDotfilePackage::new(request.id.trim()),
            ));
        }
    }
}

fn route_link(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<ToolLinkRequest>,
            &ToolProviderTarget,
        ),
        Added<ToolStoreTarget>,
    >,
    providers: Query<(), With<DotfileProvider>>,
    mut commands: Commands,
) {
    for (entity, operation, provider) in &requests {
        if !providers.contains(provider.entity()) {
            continue;
        }
        let request = &operation.0;
        if !request.id.trim().is_empty() {
            commands.entity(entity).insert((
                ToolStoreOperation,
                LinkDotfilePackage::new(request.id.trim()),
            ));
        }
    }
}

fn route_uninstall(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<ToolUninstallRequest>,
            &ToolProviderTarget,
        ),
        Added<ToolStoreTarget>,
    >,
    providers: Query<(), With<DotfileProvider>>,
    mut commands: Commands,
) {
    for (entity, operation, provider) in &requests {
        if !providers.contains(provider.entity()) {
            continue;
        }
        let request = &operation.0;
        if !request.id.trim().is_empty() {
            commands.entity(entity).insert((
                ToolStoreOperation,
                DisableDotfilePackage::new(request.id.trim()),
            ));
        }
    }
}

fn route_unlink(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<ToolUnlinkRequest>,
            &ToolProviderTarget,
        ),
        Added<ToolStoreTarget>,
    >,
    providers: Query<(), With<DotfileProvider>>,
    mut commands: Commands,
) {
    for (entity, operation, provider) in &requests {
        if !providers.contains(provider.entity()) {
            continue;
        }
        let request = &operation.0;
        if !request.id.trim().is_empty() {
            commands.entity(entity).insert((
                ToolStoreOperation,
                DisableDotfilePackage::new(request.id.trim()),
            ));
        }
    }
}

fn complete_import(
    operations: Query<
        (Entity, &ImportedDotfiles),
        (With<ToolStoreOperation>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &operations {
        commands.entity(entity).insert((
            ToolOperationFinished,
            ToolOperationSucceeded(format!("imported {} dotfile package(s)", output.packages)),
        ));
    }
}

fn complete_available_import(
    operations: Query<
        (Entity, &ImportedAvailableDotfiles),
        (With<ToolStoreOperation>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &operations {
        commands.entity(entity).insert((
            ToolOperationFinished,
            ToolOperationSucceeded(format!("imported {} dotfile package(s)", output.packages)),
        ));
    }
}

fn complete_link(
    operations: Query<
        (Entity, &LinkedDotfilePackage),
        (With<ToolStoreOperation>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &operations {
        commands.entity(entity).insert((
            ToolOperationFinished,
            ToolOperationSucceeded(format!("linked {} file(s)", output.files)),
        ));
    }
}

fn complete_disable(
    operations: Query<
        (Entity, &DisabledDotfilePackage),
        (With<ToolStoreOperation>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &operations {
        commands.entity(entity).insert((
            ToolOperationFinished,
            ToolOperationSucceeded(format!("unlinked {} file(s)", output.files)),
        ));
    }
}

fn complete_adoption(
    operations: Query<
        (Entity, &AdoptedDotfile),
        (With<ToolStoreOperation>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &operations {
        commands.entity(entity).insert((
            ToolOperationFinished,
            ToolOperationSucceeded(format!("adopted {}", output.path.display())),
        ));
    }
}

#[derive(EcsComponent, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiscoverDotfilePackages;

#[derive(EcsComponent, Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct DiscoveredDotfilePackages {
    pub(crate) packages: Vec<String>,
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct PlanDotfilePackage {
    pub(crate) package: String,
}

impl PlanDotfilePackage {
    pub fn new(package: impl Into<String>) -> Self {
        Self {
            package: package.into(),
        }
    }
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct ImportDotfiles {
    pub(crate) path: PathBuf,
}

impl ImportDotfiles {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[derive(EcsComponent, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ImportedDotfiles {
    pub(crate) packages: usize,
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct ImportAvailableDotfiles;

#[derive(EcsComponent, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ImportedAvailableDotfiles {
    pub(crate) packages: usize,
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct LinkDotfilePackage {
    pub(crate) package: String,
}

impl LinkDotfilePackage {
    pub fn new(package: impl Into<String>) -> Self {
        Self {
            package: package.into(),
        }
    }
}

#[derive(EcsComponent, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LinkedDotfilePackage {
    pub(crate) files: usize,
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct DisableDotfilePackage {
    pub(crate) package: String,
}

impl DisableDotfilePackage {
    pub fn new(package: impl Into<String>) -> Self {
        Self {
            package: package.into(),
        }
    }
}

#[derive(EcsComponent, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DisabledDotfilePackage {
    pub(crate) files: usize,
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct UnlinkDotfilePackage {
    pub(crate) package: String,
}

impl UnlinkDotfilePackage {
    pub fn new(package: impl Into<String>) -> Self {
        Self {
            package: package.into(),
        }
    }
}

#[derive(EcsComponent, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct UnlinkedDotfilePackage {
    pub(crate) files: usize,
}

#[derive(EcsComponent, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApplyEnabledDotfiles;

#[derive(EcsComponent, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AppliedEnabledDotfiles {
    pub(crate) files: usize,
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct AdoptDotfile {
    pub(crate) path: PathBuf,
    pub(crate) package: String,
}

impl AdoptDotfile {
    pub fn new(path: impl Into<PathBuf>, package: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            package: package.into(),
        }
    }
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub(super) struct AdoptedDotfile {
    pub(crate) path: PathBuf,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DotfilesManifest {
    #[serde(default)]
    pub packages: Vec<String>,
}

impl DotfilesManifest {
    pub(crate) fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DotfileLinkState {
    Linked,
    Missing,
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DotfileLink {
    pub source: PathBuf,
    pub target: PathBuf,
    pub state: DotfileLinkState,
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct DotfilePlan {
    pub package: String,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    pub links: Vec<DotfileLink>,
}

impl DotfilePlan {
    pub fn linked(&self) -> usize {
        self.links
            .iter()
            .filter(|link| link.state == DotfileLinkState::Linked)
            .count()
    }

    pub fn missing(&self) -> usize {
        self.links
            .iter()
            .filter(|link| link.state == DotfileLinkState::Missing)
            .count()
    }

    pub fn conflicts(&self) -> usize {
        self.links
            .iter()
            .filter(|link| link.state == DotfileLinkState::Conflict)
            .count()
    }
}
