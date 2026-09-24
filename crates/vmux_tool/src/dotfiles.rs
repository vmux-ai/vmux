use std::io;
use std::path::{Component, Path, PathBuf};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::{Added, Commands, Component as EcsComponent, Entity, Query, With, Without};
use bevy_ecs::schedule::IntoScheduleConfigs;
use serde::{Deserialize, Serialize};
use vmux_core::tool::{ToolAction, ToolProvider};

use crate::manifest::{
    ToolStore, ToolsManifest, expand_user_path, load_manifest_from, write_manifest_to,
};
use crate::{
    ToolActionCompletion, ToolActionRequest, ToolActionRouteSet, ToolOperation,
    ToolOperationPlugin, ToolStoreAction, ToolStoreTarget,
};

pub(crate) struct DotfileToolPlugin;

impl Plugin for DotfileToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ToolOperationPlugin::<DiscoverDotfilePackages>::default(),
            ToolOperationPlugin::<PlanDotfilePackage>::default(),
            ToolOperationPlugin::<ImportDotfiles>::default(),
            ToolOperationPlugin::<ImportAvailableDotfiles>::default(),
            ToolOperationPlugin::<LinkDotfilePackage>::default(),
        ))
        .add_plugins((
            ToolOperationPlugin::<DisableDotfilePackage>::default(),
            ToolOperationPlugin::<UnlinkDotfilePackage>::default(),
            ToolOperationPlugin::<ApplyEnabledDotfiles>::default(),
            ToolOperationPlugin::<AdoptDotfile>::default(),
        ))
        .add_systems(Update, route.in_set(ToolActionRouteSet))
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

fn route(
    requests: Query<
        (Entity, &ToolActionRequest),
        (Added<ToolActionRequest>, With<ToolStoreTarget>),
    >,
    mut commands: Commands,
) {
    for (entity, action) in &requests {
        let request = action.request();
        if request.provider != ToolProvider::Dotfiles {
            continue;
        }
        let id = request.id.trim();
        let value = request.value.trim();
        let mut entity = commands.entity(entity);
        match request.action {
            ToolAction::Import if value.is_empty() => {
                entity.insert((ToolStoreAction, ImportAvailableDotfiles));
            }
            ToolAction::Import => {
                entity.insert((ToolStoreAction, ImportDotfiles::new(value)));
            }
            ToolAction::Adopt if !id.is_empty() && !value.is_empty() => {
                entity.insert((ToolStoreAction, AdoptDotfile::new(value, id)));
            }
            ToolAction::Install | ToolAction::Update | ToolAction::Link if !id.is_empty() => {
                entity.insert((ToolStoreAction, LinkDotfilePackage::new(id)));
            }
            ToolAction::Uninstall | ToolAction::Unlink if !id.is_empty() => {
                entity.insert((ToolStoreAction, DisableDotfilePackage::new(id)));
            }
            _ => {}
        }
    }
}

fn complete_import(
    actions: Query<
        (Entity, &ImportedDotfiles),
        (With<ToolStoreAction>, Without<ToolActionCompletion>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &actions {
        commands
            .entity(entity)
            .insert(ToolActionCompletion::succeeded(format!(
                "imported {} dotfile package(s)",
                output.packages
            )));
    }
}

fn complete_available_import(
    actions: Query<
        (Entity, &ImportedAvailableDotfiles),
        (With<ToolStoreAction>, Without<ToolActionCompletion>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &actions {
        commands
            .entity(entity)
            .insert(ToolActionCompletion::succeeded(format!(
                "imported {} dotfile package(s)",
                output.packages
            )));
    }
}

fn complete_link(
    actions: Query<
        (Entity, &LinkedDotfilePackage),
        (With<ToolStoreAction>, Without<ToolActionCompletion>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &actions {
        commands
            .entity(entity)
            .insert(ToolActionCompletion::succeeded(format!(
                "linked {} file(s)",
                output.files
            )));
    }
}

fn complete_disable(
    actions: Query<
        (Entity, &DisabledDotfilePackage),
        (With<ToolStoreAction>, Without<ToolActionCompletion>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &actions {
        commands
            .entity(entity)
            .insert(ToolActionCompletion::succeeded(format!(
                "unlinked {} file(s)",
                output.files
            )));
    }
}

fn complete_adoption(
    actions: Query<
        (Entity, &AdoptedDotfile),
        (With<ToolStoreAction>, Without<ToolActionCompletion>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &actions {
        commands
            .entity(entity)
            .insert(ToolActionCompletion::succeeded(format!(
                "adopted {}",
                output.path.display()
            )));
    }
}

#[derive(EcsComponent, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiscoverDotfilePackages;

#[derive(EcsComponent, Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscoveredDotfilePackages {
    pub packages: Vec<String>,
}

impl ToolOperation for DiscoverDotfilePackages {
    type Output = DiscoveredDotfilePackages;

    fn execute(&self, store: &ToolStore) -> Result<Self::Output, String> {
        Ok(DiscoveredDotfilePackages {
            packages: store.dotfile_packages()?,
        })
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

impl ToolOperation for PlanDotfilePackage {
    type Output = DotfilePlan;

    fn execute(&self, store: &ToolStore) -> Result<Self::Output, String> {
        store.plan_dotfile_package(&self.package)
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

impl ToolOperation for ImportDotfiles {
    type Output = ImportedDotfiles;

    fn execute(&self, store: &ToolStore) -> Result<Self::Output, String> {
        let packages = store.import_dotfiles(&self.path)?;
        Ok(ImportedDotfiles { packages })
    }
}

#[derive(EcsComponent, Clone, Debug, PartialEq, Eq)]
pub struct ImportAvailableDotfiles;

#[derive(EcsComponent, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportedAvailableDotfiles {
    pub packages: usize,
}

impl ToolOperation for ImportAvailableDotfiles {
    type Output = ImportedAvailableDotfiles;

    fn execute(&self, store: &ToolStore) -> Result<Self::Output, String> {
        let packages = store.dotfile_packages()?;
        let mut manifest = store.load()?;
        let mut imported = 0;
        for package in packages {
            imported += usize::from(!manifest.dotfiles.packages.contains(&package));
            manifest.set_dotfile_package(&package, true);
        }
        store.save(&manifest)?;
        Ok(ImportedAvailableDotfiles { packages: imported })
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

impl ToolOperation for LinkDotfilePackage {
    type Output = LinkedDotfilePackage;

    fn execute(&self, store: &ToolStore) -> Result<Self::Output, String> {
        let mut manifest = store.load()?;
        manifest.set_dotfile_package(&self.package, true);
        store.save(&manifest)?;
        let files = store.apply_dotfile_package(&self.package)?;
        Ok(LinkedDotfilePackage { files })
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

impl ToolOperation for DisableDotfilePackage {
    type Output = DisabledDotfilePackage;

    fn execute(&self, store: &ToolStore) -> Result<Self::Output, String> {
        let files = store.disable_and_unlink_dotfile_package(&self.package)?;
        Ok(DisabledDotfilePackage { files })
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

impl ToolOperation for UnlinkDotfilePackage {
    type Output = UnlinkedDotfilePackage;

    fn execute(&self, store: &ToolStore) -> Result<Self::Output, String> {
        let files = store.unlink_dotfile_package(&self.package)?;
        Ok(UnlinkedDotfilePackage { files })
    }
}

#[derive(EcsComponent, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApplyEnabledDotfiles;

#[derive(EcsComponent, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppliedEnabledDotfiles {
    pub files: usize,
}

impl ToolOperation for ApplyEnabledDotfiles {
    type Output = AppliedEnabledDotfiles;

    fn execute(&self, store: &ToolStore) -> Result<Self::Output, String> {
        let manifest = store.load()?;
        let files = store.apply_enabled_dotfiles(&manifest)?;
        Ok(AppliedEnabledDotfiles { files })
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

impl ToolOperation for AdoptDotfile {
    type Output = AdoptedDotfile;

    fn execute(&self, store: &ToolStore) -> Result<Self::Output, String> {
        let path = store.adopt_dotfile(&self.path, &self.package)?;
        Ok(AdoptedDotfile { path })
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

impl ToolStore {
    pub fn import_dotfiles(&self, path: &Path) -> Result<usize, String> {
        self.migrate_legacy_storage()?;
        let path = self.expand_user_path(path)?;
        import_dotfiles_to(&path, &self.dotfiles_dir(), &self.manifest_path())
    }

    pub fn dotfile_packages(&self) -> Result<Vec<String>, String> {
        self.migrate_legacy_storage()?;
        Ok(dotfile_packages_in(&self.dotfiles_dir()))
    }

    pub fn plan_dotfile_package(&self, package: &str) -> Result<DotfilePlan, String> {
        self.migrate_legacy_storage()?;
        plan_dotfile_package_in(&self.dotfiles_dir(), self.home(), package)
    }

    pub fn apply_dotfile_package(&self, package: &str) -> Result<usize, String> {
        self.migrate_legacy_storage()?;
        apply_dotfile_package_in(&self.dotfiles_dir(), self.home(), package)
    }

    pub fn unlink_dotfile_package(&self, package: &str) -> Result<usize, String> {
        self.migrate_legacy_storage()?;
        unlink_dotfile_package_in(&self.dotfiles_dir(), self.home(), package)
    }

    pub fn disable_and_unlink_dotfile_package(&self, package: &str) -> Result<usize, String> {
        self.migrate_legacy_storage()?;
        disable_and_unlink_dotfile_package_in(
            &self.manifest_path(),
            &self.dotfiles_dir(),
            self.home(),
            package,
        )
    }

    pub fn apply_enabled_dotfiles(&self, manifest: &ToolsManifest) -> Result<usize, String> {
        self.migrate_legacy_storage()?;
        apply_enabled_dotfiles_in(manifest, &self.dotfiles_dir(), self.home())
    }

    pub fn adopt_dotfile(&self, path: &Path, package: &str) -> Result<PathBuf, String> {
        self.migrate_legacy_storage()?;
        adopt_dotfile_in(
            &self.dotfiles_dir(),
            self.home(),
            &self.manifest_path(),
            path,
            package,
        )
    }
}

pub fn dotfiles_dir() -> PathBuf {
    ToolStore::current().dotfiles_dir()
}

pub fn import_dotfiles(path: &Path) -> Result<usize, String> {
    ToolStore::current().import_dotfiles(path)
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
    ToolStore::current().dotfile_packages()
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
    ToolStore::current().plan_dotfile_package(package)
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
    ToolStore::current().apply_dotfile_package(package)
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
    ToolStore::current().unlink_dotfile_package(package)
}

pub fn disable_and_unlink_dotfile_package(package: &str) -> Result<usize, String> {
    ToolStore::current().disable_and_unlink_dotfile_package(package)
}

pub(crate) fn disable_and_unlink_dotfile_package_in(
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
    ToolStore::current().apply_enabled_dotfiles(manifest)
}

pub(crate) fn apply_enabled_dotfiles_in(
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
    ToolStore::current().adopt_dotfile(path, package)
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
