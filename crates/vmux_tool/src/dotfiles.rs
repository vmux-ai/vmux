use std::io;
use std::path::{Component, Path, PathBuf};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::{Added, Commands, Component as EcsComponent, Entity, Query, With, Without};
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_tasks::IoTaskPool;
use serde::{Deserialize, Serialize};
use vmux_core::tool::{
    ToolAdoptRequest, ToolImportRequest, ToolInstallRequest, ToolLinkRequest, ToolProvider,
    ToolUninstallRequest, ToolUnlinkRequest, ToolUpdateRequest,
};

use crate::manifest::{
    ToolStore, ToolsManifest, expand_user_path, load_manifest_from, write_manifest_to,
};
use crate::{
    ToolOperationFailed, ToolOperationFinished, ToolOperationRequest, ToolOperationRouteFlush,
    ToolOperationRouteSet, ToolOperationSucceeded, ToolOperationTask, ToolStoreOperation,
    ToolStoreTarget, finish_tool_operation,
};

pub(crate) struct DotfileToolPlugin;

impl Plugin for DotfileToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
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
                discover_dotfile_packages_system,
                plan_dotfile_package_system,
                import_dotfiles_system,
                import_available_dotfiles_system,
                link_dotfile_package_system,
                disable_dotfile_package_system,
                unlink_dotfile_package_system,
                apply_enabled_dotfiles_system,
                adopt_dotfile_system,
            )
                .after(ToolOperationRouteFlush),
        )
        .add_systems(
            Update,
            (
                finish_tool_operation::<DiscoveredDotfilePackages>,
                finish_tool_operation::<DotfilePlan>,
                finish_tool_operation::<ImportedDotfiles>,
                finish_tool_operation::<ImportedAvailableDotfiles>,
                finish_tool_operation::<LinkedDotfilePackage>,
                finish_tool_operation::<DisabledDotfilePackage>,
                finish_tool_operation::<UnlinkedDotfilePackage>,
                finish_tool_operation::<AppliedEnabledDotfiles>,
                finish_tool_operation::<AdoptedDotfile>,
            )
                .after(ToolOperationRouteFlush),
        )
        .add_systems(
            Update,
            (
                complete_import,
                complete_available_import,
                complete_link,
                complete_disable,
                complete_adoption,
            ),
        );
    }
}

fn route_import(
    requests: Query<(Entity, &ToolOperationRequest<ToolImportRequest>), Added<ToolStoreTarget>>,
    mut commands: Commands,
) {
    for (entity, operation) in &requests {
        let request = &operation.0;
        if request.provider != ToolProvider::Dotfiles {
            continue;
        }
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
    requests: Query<(Entity, &ToolOperationRequest<ToolAdoptRequest>), Added<ToolStoreTarget>>,
    mut commands: Commands,
) {
    for (entity, operation) in &requests {
        let request = &operation.0;
        if request.provider != ToolProvider::Dotfiles
            || request.id.trim().is_empty()
            || request.value.trim().is_empty()
        {
            continue;
        }
        commands.entity(entity).insert((
            ToolStoreOperation,
            AdoptDotfile::new(request.value.trim(), request.id.trim()),
        ));
    }
}

fn route_install(
    requests: Query<(Entity, &ToolOperationRequest<ToolInstallRequest>), Added<ToolStoreTarget>>,
    mut commands: Commands,
) {
    for (entity, operation) in &requests {
        let request = &operation.0;
        if request.provider == ToolProvider::Dotfiles && !request.id.trim().is_empty() {
            commands.entity(entity).insert((
                ToolStoreOperation,
                LinkDotfilePackage::new(request.id.trim()),
            ));
        }
    }
}

fn route_update(
    requests: Query<(Entity, &ToolOperationRequest<ToolUpdateRequest>), Added<ToolStoreTarget>>,
    mut commands: Commands,
) {
    for (entity, operation) in &requests {
        let request = &operation.0;
        if request.provider == ToolProvider::Dotfiles && !request.id.trim().is_empty() {
            commands.entity(entity).insert((
                ToolStoreOperation,
                LinkDotfilePackage::new(request.id.trim()),
            ));
        }
    }
}

fn route_link(
    requests: Query<(Entity, &ToolOperationRequest<ToolLinkRequest>), Added<ToolStoreTarget>>,
    mut commands: Commands,
) {
    for (entity, operation) in &requests {
        let request = &operation.0;
        if request.provider == ToolProvider::Dotfiles && !request.id.trim().is_empty() {
            commands.entity(entity).insert((
                ToolStoreOperation,
                LinkDotfilePackage::new(request.id.trim()),
            ));
        }
    }
}

fn route_uninstall(
    requests: Query<(Entity, &ToolOperationRequest<ToolUninstallRequest>), Added<ToolStoreTarget>>,
    mut commands: Commands,
) {
    for (entity, operation) in &requests {
        let request = &operation.0;
        if request.provider == ToolProvider::Dotfiles && !request.id.trim().is_empty() {
            commands.entity(entity).insert((
                ToolStoreOperation,
                DisableDotfilePackage::new(request.id.trim()),
            ));
        }
    }
}

fn route_unlink(
    requests: Query<(Entity, &ToolOperationRequest<ToolUnlinkRequest>), Added<ToolStoreTarget>>,
    mut commands: Commands,
) {
    for (entity, operation) in &requests {
        let request = &operation.0;
        if request.provider == ToolProvider::Dotfiles && !request.id.trim().is_empty() {
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
pub struct DiscoveredDotfilePackages {
    pub packages: Vec<String>,
}

fn discover_dotfile_packages_system(
    operations: Query<
        (Entity, &ToolStoreTarget),
        (
            With<DiscoverDotfilePackages>,
            Without<ToolOperationTask<DiscoveredDotfilePackages>>,
            Without<DiscoveredDotfilePackages>,
            Without<ToolOperationFinished>,
        ),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, target) in &operations {
        let Ok(store) = stores.get(target.0).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        commands
            .entity(entity)
            .insert(ToolOperationTask(IoTaskPool::get().spawn(async move {
                store.migrate_legacy_storage()?;
                Ok(DiscoveredDotfilePackages {
                    packages: dotfile_packages_in(&store.dotfiles_dir()),
                })
            })));
    }
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct PlanDotfilePackage {
    package: String,
}

impl PlanDotfilePackage {
    pub fn new(package: impl Into<String>) -> Self {
        Self {
            package: package.into(),
        }
    }
}

fn plan_dotfile_package_system(
    operations: Query<
        (Entity, &PlanDotfilePackage, &ToolStoreTarget),
        (
            Without<ToolOperationTask<DotfilePlan>>,
            Without<DotfilePlan>,
            Without<ToolOperationFinished>,
        ),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, operation, target) in &operations {
        let Ok(store) = stores.get(target.0).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        let package = operation.package.clone();
        commands
            .entity(entity)
            .insert(ToolOperationTask(IoTaskPool::get().spawn(async move {
                store.migrate_legacy_storage()?;
                plan_dotfile_package_in(&store.dotfiles_dir(), store.home(), &package)
            })));
    }
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct ImportDotfiles {
    path: PathBuf,
}

impl ImportDotfiles {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[derive(EcsComponent, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportedDotfiles {
    pub packages: usize,
}

fn import_dotfiles_system(
    operations: Query<
        (Entity, &ImportDotfiles, &ToolStoreTarget),
        (
            Without<ToolOperationTask<ImportedDotfiles>>,
            Without<ImportedDotfiles>,
            Without<ToolOperationFinished>,
        ),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, operation, target) in &operations {
        let Ok(store) = stores.get(target.0).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        let path = operation.path.clone();
        commands
            .entity(entity)
            .insert(ToolOperationTask(IoTaskPool::get().spawn(async move {
                store.migrate_legacy_storage()?;
                let path = store.expand_user_path(&path)?;
                let packages =
                    import_dotfiles_to(&path, &store.dotfiles_dir(), &store.manifest_path())?;
                Ok(ImportedDotfiles { packages })
            })));
    }
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct ImportAvailableDotfiles;

#[derive(EcsComponent, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportedAvailableDotfiles {
    pub packages: usize,
}

fn import_available_dotfiles_system(
    operations: Query<
        (Entity, &ToolStoreTarget),
        (
            With<ImportAvailableDotfiles>,
            Without<ToolOperationTask<ImportedAvailableDotfiles>>,
            Without<ImportedAvailableDotfiles>,
            Without<ToolOperationFinished>,
        ),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, target) in &operations {
        let Ok(store) = stores.get(target.0).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        commands
            .entity(entity)
            .insert(ToolOperationTask(IoTaskPool::get().spawn(async move {
                store.migrate_legacy_storage()?;
                let packages = dotfile_packages_in(&store.dotfiles_dir());
                let mut manifest = store.load()?;
                let mut imported = 0;
                for package in packages {
                    imported += usize::from(!manifest.dotfiles.packages.contains(&package));
                    manifest.set_dotfile_package(&package, true);
                }
                store.save(&manifest)?;
                Ok(ImportedAvailableDotfiles { packages: imported })
            })));
    }
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct LinkDotfilePackage {
    package: String,
}

impl LinkDotfilePackage {
    pub fn new(package: impl Into<String>) -> Self {
        Self {
            package: package.into(),
        }
    }
}

#[derive(EcsComponent, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkedDotfilePackage {
    pub files: usize,
}

fn link_dotfile_package_system(
    operations: Query<
        (Entity, &LinkDotfilePackage, &ToolStoreTarget),
        (
            Without<ToolOperationTask<LinkedDotfilePackage>>,
            Without<LinkedDotfilePackage>,
            Without<ToolOperationFinished>,
        ),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, operation, target) in &operations {
        let Ok(store) = stores.get(target.0).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        let package = operation.package.clone();
        commands
            .entity(entity)
            .insert(ToolOperationTask(IoTaskPool::get().spawn(async move {
                let mut manifest = store.load()?;
                manifest.set_dotfile_package(&package, true);
                store.save(&manifest)?;
                let files =
                    apply_dotfile_package_in(&store.dotfiles_dir(), store.home(), &package)?;
                Ok(LinkedDotfilePackage { files })
            })));
    }
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct DisableDotfilePackage {
    package: String,
}

impl DisableDotfilePackage {
    pub fn new(package: impl Into<String>) -> Self {
        Self {
            package: package.into(),
        }
    }
}

#[derive(EcsComponent, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisabledDotfilePackage {
    pub files: usize,
}

fn disable_dotfile_package_system(
    operations: Query<
        (Entity, &DisableDotfilePackage, &ToolStoreTarget),
        (
            Without<ToolOperationTask<DisabledDotfilePackage>>,
            Without<DisabledDotfilePackage>,
            Without<ToolOperationFinished>,
        ),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, operation, target) in &operations {
        let Ok(store) = stores.get(target.0).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        let package = operation.package.clone();
        commands
            .entity(entity)
            .insert(ToolOperationTask(IoTaskPool::get().spawn(async move {
                store.migrate_legacy_storage()?;
                let files = disable_and_unlink_dotfile_package_in(
                    &store.manifest_path(),
                    &store.dotfiles_dir(),
                    store.home(),
                    &package,
                )?;
                Ok(DisabledDotfilePackage { files })
            })));
    }
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct UnlinkDotfilePackage {
    package: String,
}

impl UnlinkDotfilePackage {
    pub fn new(package: impl Into<String>) -> Self {
        Self {
            package: package.into(),
        }
    }
}

#[derive(EcsComponent, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnlinkedDotfilePackage {
    pub files: usize,
}

fn unlink_dotfile_package_system(
    operations: Query<
        (Entity, &UnlinkDotfilePackage, &ToolStoreTarget),
        (
            Without<ToolOperationTask<UnlinkedDotfilePackage>>,
            Without<UnlinkedDotfilePackage>,
            Without<ToolOperationFinished>,
        ),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, operation, target) in &operations {
        let Ok(store) = stores.get(target.0).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        let package = operation.package.clone();
        commands
            .entity(entity)
            .insert(ToolOperationTask(IoTaskPool::get().spawn(async move {
                store.migrate_legacy_storage()?;
                let files =
                    unlink_dotfile_package_in(&store.dotfiles_dir(), store.home(), &package)?;
                Ok(UnlinkedDotfilePackage { files })
            })));
    }
}

#[derive(EcsComponent, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApplyEnabledDotfiles;

#[derive(EcsComponent, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppliedEnabledDotfiles {
    pub files: usize,
}

fn apply_enabled_dotfiles_system(
    operations: Query<
        (Entity, &ToolStoreTarget),
        (
            With<ApplyEnabledDotfiles>,
            Without<ToolOperationTask<AppliedEnabledDotfiles>>,
            Without<AppliedEnabledDotfiles>,
            Without<ToolOperationFinished>,
        ),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, target) in &operations {
        let Ok(store) = stores.get(target.0).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        commands
            .entity(entity)
            .insert(ToolOperationTask(IoTaskPool::get().spawn(async move {
                let manifest = store.load()?;
                let files =
                    apply_enabled_dotfiles_in(&manifest, &store.dotfiles_dir(), store.home())?;
                Ok(AppliedEnabledDotfiles { files })
            })));
    }
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct AdoptDotfile {
    path: PathBuf,
    package: String,
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
pub struct AdoptedDotfile {
    pub path: PathBuf,
}

fn adopt_dotfile_system(
    operations: Query<
        (Entity, &AdoptDotfile, &ToolStoreTarget),
        (
            Without<ToolOperationTask<AdoptedDotfile>>,
            Without<AdoptedDotfile>,
            Without<ToolOperationFinished>,
        ),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, operation, target) in &operations {
        let Ok(store) = stores.get(target.0).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        let path = operation.path.clone();
        let package = operation.package.clone();
        commands
            .entity(entity)
            .insert(ToolOperationTask(IoTaskPool::get().spawn(async move {
                store.migrate_legacy_storage()?;
                let path = adopt_dotfile_in(
                    &store.dotfiles_dir(),
                    store.home(),
                    &store.manifest_path(),
                    &path,
                    &package,
                )?;
                Ok(AdoptedDotfile { path })
            })));
    }
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

pub fn dotfiles_dir() -> PathBuf {
    ToolStore::current().dotfiles_dir()
}

pub fn import_dotfiles(path: &Path) -> Result<usize, String> {
    let store = ToolStore::current();
    store.migrate_legacy_storage()?;
    let path = store.expand_user_path(path)?;
    import_dotfiles_to(&path, &store.dotfiles_dir(), &store.manifest_path())
}

pub fn import_dotfiles_to(
    path: &Path,
    dotfiles_root: &Path,
    manifest_path: &Path,
) -> Result<usize, String> {
    let source = expand_user_path(path)?;
    if !source.is_dir() {
        return Err(format!(
            "dotfile root is not a directory: {}",
            source.display()
        ));
    }
    if source.starts_with(dotfiles_root) || dotfiles_root.starts_with(&source) {
        return Err("dotfile import source overlaps the Tools root".to_string());
    }
    let mut packages = std::fs::read_dir(&source)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            entry
                .file_name()
                .into_string()
                .ok()
                .map(|name| (name, entry.path()))
        })
        .filter(|(name, _)| valid_package_name(name))
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| left.0.cmp(&right.0));
    if packages.is_empty() {
        return Err(format!("no Stow packages found in {}", source.display()));
    }
    for (name, _) in &packages {
        if dotfiles_root.join(name).symlink_metadata().is_ok() {
            return Err(format!("Tools dotfile package already exists: {name}"));
        }
    }
    let mut manifest = load_manifest_from(manifest_path)?;
    std::fs::create_dir_all(dotfiles_root).map_err(|error| error.to_string())?;
    let mut staged = Vec::new();
    for (name, package_source) in &packages {
        let temporary = dotfiles_root.join(format!(".{name}.import-{}", std::process::id()));
        if temporary.symlink_metadata().is_ok() {
            std::fs::remove_dir_all(&temporary).map_err(|error| error.to_string())?;
        }
        if let Err(error) = copy_directory(package_source, &temporary) {
            for path in &staged {
                let _ = std::fs::remove_dir_all(path);
            }
            let _ = std::fs::remove_dir_all(&temporary);
            return Err(error);
        }
        staged.push(temporary);
    }
    let mut installed = Vec::new();
    for ((name, _), temporary) in packages.iter().zip(&staged) {
        let destination = dotfiles_root.join(name);
        if let Err(error) = std::fs::rename(temporary, &destination) {
            for path in &staged {
                let _ = std::fs::remove_dir_all(path);
            }
            for path in &installed {
                let _ = std::fs::remove_dir_all(path);
            }
            return Err(error.to_string());
        }
        installed.push(destination);
    }
    for (name, _) in &packages {
        manifest.set_dotfile_package(name, true);
    }
    if let Err(error) = write_manifest_to(manifest_path, &manifest) {
        for path in &installed {
            let _ = std::fs::remove_dir_all(path);
        }
        return Err(error);
    }
    Ok(packages.len())
}

pub fn dotfile_packages() -> Result<Vec<String>, String> {
    let store = ToolStore::current();
    store.migrate_legacy_storage()?;
    Ok(dotfile_packages_in(&store.dotfiles_dir()))
}

pub fn dotfile_packages_in(root: &Path) -> Vec<String> {
    let mut packages = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid_package_name(name))
        .collect::<Vec<_>>();
    packages.sort_by_key(|package| package.to_ascii_lowercase());
    packages
}

pub fn plan_dotfile_package(package: &str) -> Result<DotfilePlan, String> {
    let store = ToolStore::current();
    store.migrate_legacy_storage()?;
    plan_dotfile_package_in(&store.dotfiles_dir(), store.home(), package)
}

pub fn plan_dotfile_package_in(
    dotfiles_root: &Path,
    home: &Path,
    package: &str,
) -> Result<DotfilePlan, String> {
    validate_package_name(package)?;
    let package_root = dotfiles_root.join(package);
    if !package_root.is_dir() {
        return Err(format!("dotfile package does not exist: {package}"));
    }
    let mut sources = Vec::new();
    collect_files(&package_root, &mut sources).map_err(|error| error.to_string())?;
    sources.sort();
    let links = sources
        .into_iter()
        .filter_map(|source| {
            let relative = source.strip_prefix(&package_root).ok()?;
            let target = home.join(relative);
            let state = link_state(&source, &target);
            Some(DotfileLink {
                source,
                target,
                state,
            })
        })
        .collect();
    Ok(DotfilePlan {
        package: package.to_string(),
        links,
    })
}

pub fn apply_dotfile_package(package: &str) -> Result<usize, String> {
    let store = ToolStore::current();
    store.migrate_legacy_storage()?;
    apply_dotfile_package_in(&store.dotfiles_dir(), store.home(), package)
}

pub fn apply_dotfile_package_in(
    dotfiles_root: &Path,
    home: &Path,
    package: &str,
) -> Result<usize, String> {
    let plan = plan_dotfile_package_in(dotfiles_root, home, package)?;
    apply_dotfile_plan(&plan).map(|created| created.len())
}

fn apply_dotfile_plan(plan: &DotfilePlan) -> Result<Vec<PathBuf>, String> {
    if plan.conflicts() > 0 {
        return Err(format!(
            "dotfile package {} has {} conflict(s)",
            plan.package,
            plan.conflicts()
        ));
    }
    let mut created = Vec::new();
    for link in plan
        .links
        .iter()
        .filter(|link| link.state == DotfileLinkState::Missing)
    {
        let result = (|| -> io::Result<()> {
            if let Some(parent) = link.target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            create_relative_symlink(&link.source, &link.target)
        })();
        if let Err(error) = result {
            for target in created.iter().rev() {
                let _ = std::fs::remove_file(target);
            }
            return Err(error.to_string());
        }
        created.push(link.target.clone());
    }
    Ok(created)
}

pub fn unlink_dotfile_package(package: &str) -> Result<usize, String> {
    let store = ToolStore::current();
    store.migrate_legacy_storage()?;
    unlink_dotfile_package_in(&store.dotfiles_dir(), store.home(), package)
}

pub fn disable_and_unlink_dotfile_package(package: &str) -> Result<usize, String> {
    let store = ToolStore::current();
    store.migrate_legacy_storage()?;
    disable_and_unlink_dotfile_package_in(
        &store.manifest_path(),
        &store.dotfiles_dir(),
        store.home(),
        package,
    )
}

pub fn disable_and_unlink_dotfile_package_in(
    manifest_path: &Path,
    dotfiles_root: &Path,
    home: &Path,
    package: &str,
) -> Result<usize, String> {
    validate_package_name(package)?;
    let mut manifest = load_manifest_from(manifest_path)?;
    let links = if dotfiles_root.join(package).is_dir() {
        plan_dotfile_package_in(dotfiles_root, home, package)?
            .links
            .into_iter()
            .filter(|link| link.state == DotfileLinkState::Linked)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut removed = Vec::new();
    for link in links {
        if let Err(error) = std::fs::remove_file(&link.target) {
            let rollback = restore_dotfile_links(&removed);
            return Err(match rollback {
                Ok(()) => error.to_string(),
                Err(rollback) => format!("{error}; failed to restore dotfile links: {rollback}"),
            });
        }
        removed.push(link);
    }
    manifest.set_dotfile_package(package, false);
    if let Err(error) = write_manifest_to(manifest_path, &manifest) {
        let rollback = restore_dotfile_links(&removed);
        return Err(match rollback {
            Ok(()) => error,
            Err(rollback) => format!("{error}; failed to restore dotfile links: {rollback}"),
        });
    }
    Ok(removed.len())
}

fn restore_dotfile_links(links: &[DotfileLink]) -> Result<(), String> {
    let mut errors = Vec::new();
    for link in links.iter().rev() {
        if let Err(error) = create_relative_symlink(&link.source, &link.target) {
            errors.push(format!("{}: {error}", link.target.display()));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join(", "))
    }
}

pub fn unlink_dotfile_package_in(
    dotfiles_root: &Path,
    home: &Path,
    package: &str,
) -> Result<usize, String> {
    let plan = plan_dotfile_package_in(dotfiles_root, home, package)?;
    let mut removed = 0;
    for link in plan
        .links
        .iter()
        .filter(|link| link.state == DotfileLinkState::Linked)
    {
        std::fs::remove_file(&link.target).map_err(|error| error.to_string())?;
        removed += 1;
    }
    Ok(removed)
}

pub fn apply_enabled_dotfiles(manifest: &ToolsManifest) -> Result<usize, String> {
    let store = ToolStore::current();
    store.migrate_legacy_storage()?;
    apply_enabled_dotfiles_in(manifest, &store.dotfiles_dir(), store.home())
}

pub fn apply_enabled_dotfiles_in(
    manifest: &ToolsManifest,
    dotfiles_root: &Path,
    home: &Path,
) -> Result<usize, String> {
    let mut plans = Vec::new();
    for package in &manifest.dotfiles.packages {
        plans.push(plan_dotfile_package_in(dotfiles_root, home, package)?);
    }
    if let Some(plan) = plans.iter().find(|plan| plan.conflicts() > 0) {
        return Err(format!(
            "dotfile package {} has {} conflict(s)",
            plan.package,
            plan.conflicts()
        ));
    }
    let mut created = Vec::new();
    for plan in &plans {
        match apply_dotfile_plan(plan) {
            Ok(links) => created.extend(links),
            Err(error) => {
                for target in created.iter().rev() {
                    let _ = std::fs::remove_file(target);
                }
                return Err(error);
            }
        }
    }
    Ok(created.len())
}

pub fn adopt_dotfile(path: &Path, package: &str) -> Result<PathBuf, String> {
    let store = ToolStore::current();
    store.migrate_legacy_storage()?;
    adopt_dotfile_in(
        &store.dotfiles_dir(),
        store.home(),
        &store.manifest_path(),
        path,
        package,
    )
}

pub fn adopt_dotfile_in(
    dotfiles_root: &Path,
    home: &Path,
    manifest_path: &Path,
    path: &Path,
    package: &str,
) -> Result<PathBuf, String> {
    validate_package_name(package)?;
    let path = if let Ok(relative) = path.strip_prefix("~") {
        home.join(relative)
    } else if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    if !path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_file())
    {
        return Err(format!("dotfile is not a file: {}", path.display()));
    }
    if path.starts_with(dotfiles_root) {
        return Err("dotfile is already inside the Tools directory".to_string());
    }
    let relative = path
        .strip_prefix(home)
        .map_err(|_| format!("dotfile must be inside {}", home.display()))?;
    if relative.as_os_str().is_empty() || contains_parent_component(relative) {
        return Err("invalid dotfile path".to_string());
    }
    let destination = dotfiles_root.join(package).join(relative);
    if destination.exists() || destination.symlink_metadata().is_ok() {
        return Err(format!(
            "Tools dotfile already exists: {}",
            destination.display()
        ));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::rename(&path, &destination).map_err(|error| error.to_string())?;
    if let Err(error) = create_relative_symlink(&destination, &path) {
        let _ = std::fs::rename(&destination, &path);
        return Err(error.to_string());
    }
    let mut manifest = load_manifest_from(manifest_path)?;
    manifest.set_dotfile_package(package, true);
    if let Err(error) = write_manifest_to(manifest_path, &manifest) {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::rename(&destination, &path);
        return Err(error);
    }
    Ok(destination)
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
        } else {
            return Err(format!(
                "unsupported entry in dotfile package: {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

fn valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
}

fn validate_package_name(name: &str) -> Result<(), String> {
    valid_package_name(name)
        .then_some(())
        .ok_or_else(|| format!("invalid dotfile package name: {name}"))
}

fn collect_files(directory: &Path, output: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_files(&path, output)?;
        } else if file_type.is_file() {
            output.push(path);
        }
    }
    Ok(())
}

fn link_state(source: &Path, target: &Path) -> DotfileLinkState {
    let Ok(metadata) = target.symlink_metadata() else {
        return DotfileLinkState::Missing;
    };
    if !metadata.file_type().is_symlink() {
        return DotfileLinkState::Conflict;
    }
    let Ok(link) = std::fs::read_link(target) else {
        return DotfileLinkState::Conflict;
    };
    let resolved = if link.is_absolute() {
        link
    } else {
        target.parent().unwrap_or(Path::new("/")).join(link)
    };
    if vmux_path::PathIdentity::resolve(&resolved) == vmux_path::PathIdentity::resolve(source) {
        DotfileLinkState::Linked
    } else {
        DotfileLinkState::Conflict
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn contains_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| component == Component::ParentDir)
}

fn create_relative_symlink(source: &Path, target: &Path) -> io::Result<()> {
    let parent = target.parent().unwrap_or(Path::new("/"));
    let relative = relative_path(parent, source);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(relative, target)
    }
    #[cfg(not(unix))]
    {
        std::os::windows::fs::symlink_file(relative, target)
    }
}

fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from = normalize(from);
    let to = normalize(to);
    let from_components = from.components().collect::<Vec<_>>();
    let to_components = to.components().collect::<Vec<_>>();
    let common = from_components
        .iter()
        .zip(&to_components)
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = PathBuf::new();
    for _ in common..from_components.len() {
        relative.push("..");
    }
    for component in &to_components[common..] {
        relative.push(component.as_os_str());
    }
    relative
}
