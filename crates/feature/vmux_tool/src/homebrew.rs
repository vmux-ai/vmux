use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::state::{
    ToolImportRequest, ToolOperationKey, ToolOperationKind, ToolProvider, ToolStatus,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;

use vmux_ecs::OperationState;

use crate::manifest::{ToolStore, ToolsManifest};
use crate::process::ToolProcess;
use crate::{
    ToolInventory, ToolInventoryItem, ToolOperationFailed, ToolOperationFinished,
    ToolOperationRequest, ToolOperationRouteSet, ToolOperationSucceeded, ToolOperator,
    ToolProviderBinding, ToolProviderSnapshot, ToolProviderTarget, ToolScanner, ToolStoreOperation,
    ToolStoreTarget, ToolTaskSet,
};

pub(crate) struct HomebrewToolPlugin;

impl Plugin for HomebrewToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_systems(Update, route.in_set(ToolOperationRouteSet))
            .add_systems(
                Update,
                (import, read, commit, save)
                    .chain()
                    .in_set(ToolTaskSet::Start),
            )
            .add_systems(Update, drain.after(ToolTaskSet::Apply))
            .add_systems(Update, finish.after(ToolTaskSet::Apply));
    }
}

#[derive(Component, Clone, Copy)]
enum HomebrewPackage {
    Formula,
    Cask,
}

impl HomebrewPackage {
    fn scan_formulae(
        provider: &ToolProvider,
        store: &ToolStore,
        manifest: &mut ToolsManifest,
        refresh: bool,
    ) -> Result<ToolProviderSnapshot, String> {
        Self::Formula.scan(provider, store, manifest, refresh)
    }

    fn scan_casks(
        provider: &ToolProvider,
        store: &ToolStore,
        manifest: &mut ToolsManifest,
        refresh: bool,
    ) -> Result<ToolProviderSnapshot, String> {
        Self::Cask.scan(provider, store, manifest, refresh)
    }

    fn operate_formula(
        store: &ToolStore,
        operation: &ToolOperationKey,
        value: &str,
    ) -> Result<String, String> {
        Self::Formula.operate(store, operation, value)
    }

    fn operate_cask(
        store: &ToolStore,
        operation: &ToolOperationKey,
        value: &str,
    ) -> Result<String, String> {
        Self::Cask.operate(store, operation, value)
    }

    fn is_cask(self) -> bool {
        matches!(self, Self::Cask)
    }

    fn detail(self) -> &'static str {
        match self {
            Self::Formula => "Homebrew formula",
            Self::Cask => "Homebrew cask",
        }
    }

    fn scan(
        self,
        provider: &ToolProvider,
        _store: &ToolStore,
        manifest: &mut ToolsManifest,
        refresh: bool,
    ) -> Result<ToolProviderSnapshot, String> {
        Ok(
            ToolInventory::new(provider.clone(), self.inventory(refresh)?)
                .reconcile(manifest)
                .into(),
        )
    }

    fn inventory(self, refresh: bool) -> Result<Vec<ToolInventoryItem>, String> {
        let Some(brew) = ToolProcess::find("brew") else {
            return Ok(Vec::new());
        };
        let mut args = vec!["list"];
        args.push(if self.is_cask() {
            "--cask"
        } else {
            "--formula"
        });
        args.push("--versions");
        let output = brew.output(&args, true)?;
        let outdated = if refresh {
            let mut outdated_args = vec!["outdated"];
            outdated_args.push(if self.is_cask() {
                "--cask"
            } else {
                "--formula"
            });
            brew.output(&outdated_args, false)
                .map(|output| Self::names(&output.stdout))
                .unwrap_or_default()
        } else {
            BTreeSet::new()
        };
        let mut items = Vec::new();
        for (name, version) in Self::versions(&output.stdout) {
            let status = if outdated.contains(&name) {
                ToolStatus::Outdated
            } else {
                ToolStatus::Installed
            };
            let removable = !self.is_cask() || name != "vmux";
            items.push(ToolInventoryItem {
                id: name.clone(),
                name,
                icon: None,
                version,
                detail: self.detail().to_string(),
                status,
                removable,
            });
        }
        Ok(items)
    }

    fn versions(bytes: &[u8]) -> Vec<(String, Option<String>)> {
        let mut versions = Vec::new();
        for line in String::from_utf8_lossy(bytes).lines() {
            let mut fields = line.split_whitespace();
            let Some(name) = fields.next() else {
                continue;
            };
            let version = fields.collect::<Vec<_>>().join(" ");
            versions.push((name.to_string(), (!version.is_empty()).then_some(version)));
        }
        versions
    }

    fn names(bytes: &[u8]) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for line in String::from_utf8_lossy(bytes).lines() {
            let Some(name) = line.split_whitespace().next() else {
                continue;
            };
            names.insert(name.to_string());
        }
        names
    }

    fn operate(
        self,
        store: &ToolStore,
        operation: &ToolOperationKey,
        value: &str,
    ) -> Result<String, String> {
        let id = operation.item_id.trim();
        match operation.kind {
            ToolOperationKind::Install => {
                self.command("install", id)?;
                store.set_managed_package(&operation.provider, id, true)?;
                Ok(format!("{id} installed"))
            }
            ToolOperationKind::Update => {
                self.command("upgrade", id)?;
                store.set_managed_package(&operation.provider, id, true)?;
                Ok(format!("{id} updated"))
            }
            ToolOperationKind::Uninstall => {
                self.command("uninstall", id)?;
                store.set_managed_package(&operation.provider, id, false)?;
                Ok(format!("{id} removed"))
            }
            ToolOperationKind::Forget => {
                store.set_managed_package(&operation.provider, id, false)?;
                Ok(format!("{id} removed from tools.toml"))
            }
            ToolOperationKind::Adopt => {
                store.set_managed_package(&operation.provider, id, true)?;
                Ok(format!("{id} is now managed"))
            }
            ToolOperationKind::Import if value.trim().is_empty() => {
                self.import_installed(store, &operation.provider)
            }
            _ => Err(format!(
                "{} does not support {:?}",
                operation.provider.id(),
                operation.kind
            )),
        }
    }

    fn command(self, operation: &str, id: &str) -> Result<(), String> {
        if id.is_empty() {
            return Err("package name is required".to_string());
        }
        let brew = ToolProcess::find("brew").ok_or_else(|| "brew is not installed".to_string())?;
        let args = if self.is_cask() {
            vec![operation, "--cask", id]
        } else {
            vec![operation, id]
        };
        brew.output(&args, true)?;
        Ok(())
    }

    fn import_installed(
        self,
        store: &ToolStore,
        provider: &ToolProvider,
    ) -> Result<String, String> {
        let mut manifest = store.load()?;
        let before = manifest.managed_packages(provider.id()).len();
        let _ = self.scan(provider, store, &mut manifest, false)?;
        let imported = manifest
            .managed_packages(provider.id())
            .len()
            .saturating_sub(before);
        store.save(&manifest)?;
        let kind = if self.is_cask() { "cask" } else { "formula" };
        Ok(format!("imported {imported} {kind} item(s)"))
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        ToolProviderBinding::new::<crate::Feature>(0),
        HomebrewPackage::Formula,
        ToolScanner::new(HomebrewPackage::scan_formulae),
        ToolOperator::new(HomebrewPackage::operate_formula),
    ));
    commands.spawn((
        ToolProviderBinding::new::<crate::Feature>(1),
        HomebrewPackage::Cask,
        ToolScanner::new(HomebrewPackage::scan_casks),
        ToolOperator::new(HomebrewPackage::operate_cask),
    ));
}

fn route(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<ToolImportRequest>,
            &ToolProviderTarget,
        ),
        Added<ToolStoreTarget>,
    >,
    providers: Query<(), With<HomebrewPackage>>,
    mut commands: Commands,
) {
    for (entity, operation, provider) in &requests {
        if !providers.contains(provider.entity()) {
            continue;
        }
        let request = &operation.0;
        let path = request.value.trim();
        if path.is_empty() {
            continue;
        }
        commands
            .entity(entity)
            .insert((ToolStoreOperation, ImportBrewfile::new(path)));
    }
}

fn finish(
    operations: Query<
        (Entity, &ImportedBrewfile),
        (With<ToolStoreOperation>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &operations {
        commands.entity(entity).insert((
            ToolOperationFinished,
            ToolOperationSucceeded(format!(
                "imported {} formulae and {} casks",
                output.formulae, output.casks
            )),
        ));
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ImportBrewfile {
    path: PathBuf,
}

impl ImportBrewfile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ImportedBrewfile {
    pub(super) formulae: usize,
    pub(super) casks: usize,
}

#[derive(Component)]
struct BrewPlan {
    formulae: usize,
    casks: usize,
}

#[derive(Component)]
struct Importing;

fn import(
    operations: Query<
        (
            Entity,
            &ImportBrewfile,
            &vmux_ecs::EntityTarget<ToolStore>,
            Option<&OperationState>,
        ),
        (Without<Importing>, Without<ToolOperationFinished>),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, _, target, state) in &operations {
        if state.is_some_and(|state| {
            !matches!(state, OperationState::Pending | OperationState::Running)
        }) {
            continue;
        }
        let Ok(store) = stores.get(target.entity()) else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".into()),
            ));
            continue;
        };
        commands.entity(entity).insert((
            Importing,
            store.clone(),
            vmux_ecs::OperationRetained,
            crate::storage::ReadRequest { retain_lease: true },
        ));
    }
}

fn read(
    operations: Query<
        (
            Entity,
            &ImportBrewfile,
            &ToolStore,
            &crate::storage::ReadResult,
        ),
        (With<Importing>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, input, store, result) in &operations {
        commands
            .entity(entity)
            .remove::<crate::storage::ReadResult>();
        let manifest = match &result.0 {
            Ok(manifest) => manifest.clone(),
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ToolOperationFinished, ToolOperationFailed(error.clone())));
                continue;
            }
        };
        let path = match store.expand_user_path(&input.path) {
            Ok(path) => path,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ToolOperationFinished, ToolOperationFailed(error)));
                continue;
            }
        };
        commands
            .entity(entity)
            .insert((manifest, crate::storage::FileReadRequest(path)));
    }
}

fn commit(
    mut operations: Query<
        (
            Entity,
            &ImportBrewfile,
            &crate::storage::FileReadResult,
            &mut ToolsManifest,
        ),
        (With<Importing>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, input, source, mut manifest) in &mut operations {
        commands
            .entity(entity)
            .remove::<crate::storage::FileReadResult>();
        let source = match &source.0 {
            Ok(source) => source,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ToolOperationFinished, ToolOperationFailed(error.clone())));
                continue;
            }
        };
        let document = BrewfileImport::parse(source);
        if document.formulae.is_empty() && document.casks.is_empty() {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed(format!(
                    "no formulae or casks found in {}",
                    input.path.display()
                )),
            ));
            continue;
        }
        let formulae = manifest.add_packages("homebrew-formula", &document.formulae);
        let casks = manifest.add_packages("homebrew-cask", &document.casks);
        commands.entity(entity).insert((
            BrewPlan { formulae, casks },
            crate::storage::BrewfileTemplate(source.clone()),
            crate::storage::WriteRequest {
                manifest: manifest.clone(),
                retain_lease: true,
            },
        ));
    }
}

fn save(
    operations: Query<
        (Entity, &BrewPlan, &crate::storage::WriteResult),
        (With<Importing>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, plan, result) in &operations {
        commands
            .entity(entity)
            .remove::<crate::storage::WriteResult>();
        match &result.0 {
            Ok(()) => {
                commands.entity(entity).insert(ImportedBrewfile {
                    formulae: plan.formulae,
                    casks: plan.casks,
                });
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ToolOperationFinished, ToolOperationFailed(error.clone())));
            }
        }
        commands.entity(entity).remove::<(
            Importing,
            BrewPlan,
            ToolStore,
            ToolsManifest,
            crate::storage::StoreLease,
            vmux_ecs::OperationRetained,
        )>();
    }
}

fn drain(
    operations: Query<
        Entity,
        (
            With<Importing>,
            With<ToolOperationFinished>,
            Without<crate::storage::ReadRequest>,
            Without<crate::storage::WriteRequest>,
            Without<crate::storage::FileReadRequest>,
        ),
    >,
    mut commands: Commands,
) {
    for entity in &operations {
        commands.entity(entity).remove::<(
            Importing,
            BrewPlan,
            ToolStore,
            ToolsManifest,
            crate::storage::StoreLease,
            vmux_ecs::OperationRetained,
            crate::storage::ReadResult,
            crate::storage::WriteResult,
            crate::storage::FileReadResult,
        )>();
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrewfileImport {
    pub formulae: Vec<String>,
    pub casks: Vec<String>,
}

impl BrewfileImport {
    pub fn parse(source: &str) -> Self {
        let mut import = Self::default();
        for line in source.lines() {
            if let Some(name) = Self::quoted_call(line, "brew") {
                import.formulae.push(name);
            } else if let Some(name) = Self::quoted_call(line, "cask") {
                import.casks.push(name);
            }
        }
        ToolsManifest::normalize_names(&mut import.formulae);
        ToolsManifest::normalize_names(&mut import.casks);
        import
    }

    fn quoted_call(line: &str, call: &str) -> Option<String> {
        let line = line.trim_start();
        let rest = line.strip_prefix(call)?;
        if !rest.starts_with(char::is_whitespace) {
            return None;
        }
        let rest = rest.trim_start();
        let quote = rest.chars().next()?;
        if !matches!(quote, '\'' | '"') {
            return None;
        }
        let mut escaped = false;
        let mut name = String::new();
        for character in rest[quote.len_utf8()..].chars() {
            if escaped {
                name.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote {
                return (!name.is_empty()).then_some(name);
            } else {
                name.push(character);
            }
        }
        None
    }
}

impl ToolsManifest {
    fn set_packages(&mut self, provider: &str, packages: Vec<String>) {
        if packages.is_empty() {
            self.packages.remove(provider);
        } else {
            self.packages.insert(provider.to_string(), packages);
        }
        self.normalize();
    }

    pub(crate) fn sync_brewfile(&mut self, path: &Path) -> Result<(), String> {
        let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        let imported = BrewfileImport::parse(&source);
        self.set_packages("homebrew-formula", imported.formulae);
        self.set_packages("homebrew-cask", imported.casks);
        Ok(())
    }

    pub(crate) fn write_brewfile_to(&self, path: &Path) -> Result<(), String> {
        let formulae = self
            .packages
            .get("homebrew-formula")
            .cloned()
            .unwrap_or_default();
        let casks = self
            .packages
            .get("homebrew-cask")
            .cloned()
            .unwrap_or_default();
        if formulae.is_empty() && casks.is_empty() && !path.exists() {
            return Ok(());
        }
        let existing = std::fs::read_to_string(path).unwrap_or_default();
        let source = Self::merged_brewfile(&existing, &formulae, &casks);
        vmux_path::AtomicFile::write(path, source.as_bytes()).map_err(|error| error.to_string())
    }

    fn merged_brewfile(source: &str, formulae: &[String], casks: &[String]) -> String {
        let desired_formulae = formulae.iter().map(String::as_str).collect::<BTreeSet<_>>();
        let desired_casks = casks.iter().map(String::as_str).collect::<BTreeSet<_>>();
        let mut seen_formulae = BTreeSet::new();
        let mut seen_casks = BTreeSet::new();
        let mut lines = Vec::new();
        for line in source.lines() {
            if let Some(name) = BrewfileImport::quoted_call(line, "brew") {
                if desired_formulae.contains(name.as_str()) {
                    seen_formulae.insert(name);
                    lines.push(line.to_string());
                }
            } else if let Some(name) = BrewfileImport::quoted_call(line, "cask") {
                if desired_casks.contains(name.as_str()) {
                    seen_casks.insert(name);
                    lines.push(line.to_string());
                }
            } else {
                lines.push(line.to_string());
            }
        }
        for package in formulae {
            if !seen_formulae.contains(package) {
                lines.push(format!("brew {:?}", package));
            }
        }
        for package in casks {
            if !seen_casks.contains(package) {
                lines.push(format!("cask {:?}", package));
            }
        }
        if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }
}

impl ToolStore {
    pub(crate) fn write_brewfile(&self, manifest: &ToolsManifest) -> Result<(), String> {
        manifest.write_brewfile_to(&self.brewfile_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inventory_versions() {
        assert_eq!(
            HomebrewPackage::versions(b"ripgrep 14.1.1\nopenssl@3 3.5.0 3.5.1\n"),
            vec![
                ("ripgrep".to_string(), Some("14.1.1".to_string())),
                ("openssl@3".to_string(), Some("3.5.0 3.5.1".to_string())),
            ]
        );
    }
}
