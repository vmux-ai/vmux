use std::collections::HashSet;
use std::path::Path;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, block_on, futures_lite::future};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use crossbeam_channel::{Receiver, Sender};
use vmux_core::event::{
    InstallPhase, LspCatalog, LspCatalogRequest, LspInstallProgress, LspInstallRequest,
    LspManagerUiState, LspPackage, LspPackageStatus, LspPkgStatus, LspUninstallRequest,
    LspUpdateRequest,
};
use vmux_core::host::{UiState, UiStatePlugin, UiStateWrite};
use vmux_layout::native_open::HostedPage;

use crate::lsp::catalog::{self, Package};
use crate::lsp::{install, purl, store, target};

pub struct ManagerPlugin;

impl Plugin for ManagerPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.add_plugins(vmux_layout::native_open::HostedPagePlugin::<LspManagerPage>::default())
            .add_plugins(UiStatePlugin::<LspManagerUiState>::default())
            .add_plugins(UiEventPlugin::<(
                LspCatalogRequest,
                LspInstallRequest,
                LspUninstallRequest,
                LspUpdateRequest,
            )>::default())
            .add_observer(on_catalog_request)
            .add_observer(on_install_request)
            .add_observer(on_uninstall_request)
            .add_observer(on_update_request)
            .add_systems(
                Update,
                (start_catalog_jobs, start_install_jobs, start_uninstall_jobs).chain(),
            )
            .add_systems(
                Update,
                (poll_catalog_jobs, poll_install_jobs, poll_uninstall_jobs),
            )
            .add_systems(
                PostUpdate,
                (
                    deliver_catalog_outputs,
                    deliver_progress_outputs,
                    deliver_status_outputs,
                    publish_manager_state,
                )
                    .chain(),
            );
    }
}

const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "lsp",
    title: "Language Servers",
    title_message_id: Some("lsp-title"),
    replaces_command: None,
    keywords: &["lsp", "language", "server", "install", "mason"],
    icon: Some(vmux_core::BuiltinIcon::Server),
    command_bar: true,
};

#[derive(Component, Default)]
#[require(ManagerState, UiState<LspManagerUiState>)]
struct LspManagerPage;

impl HostedPage for LspManagerPage {
    const HOST: &'static str = "lsp";
    const URL: &'static str = "vmux://lsp/";
    const TITLE: &'static str = "Language Servers";
}

#[derive(Component)]
struct ManagerState {
    packages: Vec<LspPackage>,
    progress: Vec<LspInstallProgress>,
    loading: bool,
}

impl Default for ManagerState {
    fn default() -> Self {
        Self {
            packages: Vec::new(),
            progress: Vec::new(),
            loading: true,
        }
    }
}

impl ManagerState {
    fn start_loading(&mut self) {
        self.loading = true;
    }

    fn apply_catalog(&mut self, event: LspCatalog) {
        self.packages = event.packages;
        self.loading = false;
    }

    fn apply_progress(&mut self, event: LspInstallProgress) {
        let name = event.name.clone();
        let phase = event.phase;
        match self.progress.iter_mut().find(|item| item.name == name) {
            Some(current) => *current = event,
            None => self.progress.push(event),
        }
        let Some(package) = self
            .packages
            .iter_mut()
            .find(|package| package.name == name)
        else {
            return;
        };
        package.status = match phase {
            InstallPhase::Failed => LspPkgStatus::Failed,
            InstallPhase::Done => LspPkgStatus::Installed,
            _ => LspPkgStatus::Installing,
        };
    }

    fn apply_status(&mut self, event: LspPackageStatus) {
        let name = event.name;
        if let Some(package) = self
            .packages
            .iter_mut()
            .find(|package| package.name == name)
        {
            package.status = event.status;
            package.version = event.version;
        }
        self.progress.retain(|item| item.name != name);
    }

    fn event(&self) -> LspManagerUiState {
        LspManagerUiState {
            packages: self.packages.clone(),
            progress: self.progress.clone(),
            loading: self.loading,
        }
    }
}

#[derive(Component)]
struct CatalogOutput {
    target: Entity,
    catalog: LspCatalog,
}

#[derive(Component)]
struct PackageProgressOutput {
    target: Entity,
    progress: LspInstallProgress,
}

#[derive(Component)]
struct PackageStatusOutput {
    target: Entity,
    status: LspPackageStatus,
}

#[derive(bevy::ecs::system::SystemParam)]
struct PackageTargets<'w, 's> {
    views: Query<'w, 's, (Entity, &'static crate::host::editor::FileView)>,
}

impl PackageTargets<'_, '_> {
    fn matching(&self, target: Entity, package: &str) -> Vec<Entity> {
        let mut targets = vec![target];
        for (entity, view) in &self.views {
            if view.uses_lsp_package(package) && !targets.contains(&entity) {
                targets.push(entity);
            }
        }
        targets
    }

    fn contains(&self, entity: Entity) -> bool {
        self.views.contains(entity)
    }

    fn uses_package(&self, entity: Entity, package: &str) -> bool {
        self.views
            .get(entity)
            .is_ok_and(|(_, view)| view.uses_lsp_package(package))
    }
}

#[derive(Component)]
struct PendingCatalogJob {
    target: Entity,
    request: LspCatalogRequest,
}

#[derive(Component)]
struct CatalogJob {
    target: Entity,
    task: Task<LspCatalog>,
}

fn start_catalog_jobs(
    pending: Query<(Entity, &PendingCatalogJob), Added<PendingCatalogJob>>,
    mut commands: Commands,
) {
    for (entity, pending) in &pending {
        let target = pending.target;
        let request = pending.request.clone();
        let task = IoTaskPool::get().spawn(async move {
            let root = store::default_root();
            let packages = catalog::ensure_catalog(&root, request.refresh).unwrap_or_default();
            let mut packages = catalog::search(
                &packages,
                &request.query,
                &request.language,
                &request.category,
            )
            .iter()
            .map(|package| package.to_lsp_package(&root))
            .collect::<Vec<_>>();
            if request.installed_only {
                packages.retain(|package| {
                    matches!(
                        package.status,
                        LspPkgStatus::Installed | LspPkgStatus::Outdated
                    )
                });
            }
            LspCatalog { packages }
        });
        commands
            .entity(entity)
            .remove::<PendingCatalogJob>()
            .insert(CatalogJob { target, task });
    }
}

#[derive(Component)]
struct PendingPackageInstall {
    target: Entity,
    name: String,
}

#[derive(Component)]
struct PendingPackageUninstall {
    target: Entity,
    name: String,
}

#[derive(Component)]
struct PackageInstallJob {
    target: Entity,
    name: String,
    progress: Receiver<LspInstallProgress>,
    task: Task<Result<LspPackageStatus, LspInstallProgress>>,
}

#[derive(Component)]
struct PackageUninstallJob {
    target: Entity,
    name: String,
    task: Task<Result<LspPackageStatus, LspInstallProgress>>,
}

fn install_package(
    name: String,
    progress: Sender<LspInstallProgress>,
) -> Result<LspPackageStatus, LspInstallProgress> {
    let root = store::default_root();
    let packages = catalog::ensure_catalog(&root, false).unwrap_or_default();
    let Some(package) = packages
        .iter()
        .find(|package| package.name.as_str() == name)
        .cloned()
    else {
        return Err(LspInstallProgress {
            name,
            phase: InstallPhase::Failed,
            pct: None,
            message: "package not found in catalog".into(),
        });
    };
    let target = target::host_target();
    let progress_name = name.clone();
    let result = install::install(&package, &root, target, |phase, pct, message| {
        let _ = progress.send(LspInstallProgress {
            name: progress_name.clone(),
            phase,
            pct,
            message: message.to_string(),
        });
    });
    match result {
        Ok(receipt) => Ok(LspPackageStatus {
            name,
            status: LspPkgStatus::Installed,
            version: receipt.version,
        }),
        Err(error) => Err(LspInstallProgress {
            name,
            phase: InstallPhase::Failed,
            pct: None,
            message: error,
        }),
    }
}

fn uninstall_package(name: String) -> Result<LspPackageStatus, LspInstallProgress> {
    let root = store::default_root();
    let Ok(package) = crate::lsp::package_path::PackageName::parse(&name) else {
        return Err(LspInstallProgress {
            name,
            phase: InstallPhase::Failed,
            pct: None,
            message: "invalid package name".to_string(),
        });
    };
    if let Err(error) = store::remove(&root, &package) {
        return Err(LspInstallProgress {
            name,
            phase: InstallPhase::Failed,
            pct: None,
            message: format!("uninstall failed: {error}"),
        });
    }
    let status = if matches!(
        store::resolved_command(&root, &name),
        store::Resolution::OnPath
    ) {
        LspPkgStatus::OnPath
    } else {
        LspPkgStatus::Available
    };
    Ok(LspPackageStatus {
        name,
        status,
        version: None,
    })
}

fn start_install_jobs(
    pending: Query<(Entity, &PendingPackageInstall), Added<PendingPackageInstall>>,
    install_jobs: Query<&PackageInstallJob>,
    uninstall_jobs: Query<&PackageUninstallJob>,
    mut commands: Commands,
) {
    let mut names = install_jobs
        .iter()
        .map(|job| job.name.clone())
        .collect::<HashSet<_>>();
    names.extend(uninstall_jobs.iter().map(|job| job.name.clone()));
    for (entity, request) in &pending {
        commands.entity(entity).despawn();
        if !names.insert(request.name.clone()) {
            continue;
        }
        let (progress_sender, progress) = crossbeam_channel::unbounded();
        let name = request.name.clone();
        let task_name = name.clone();
        let task =
            IoTaskPool::get().spawn(async move { install_package(task_name, progress_sender) });
        commands.spawn((
            Name::new(format!("LSP install: {name}")),
            PackageInstallJob {
                target: request.target,
                name,
                progress,
                task,
            },
        ));
    }
}

fn start_uninstall_jobs(
    pending: Query<(Entity, &PendingPackageUninstall), Added<PendingPackageUninstall>>,
    install_jobs: Query<&PackageInstallJob>,
    uninstall_jobs: Query<&PackageUninstallJob>,
    mut commands: Commands,
) {
    let mut names = install_jobs
        .iter()
        .map(|job| job.name.clone())
        .collect::<HashSet<_>>();
    names.extend(uninstall_jobs.iter().map(|job| job.name.clone()));
    for (entity, request) in &pending {
        commands.entity(entity).despawn();
        if !names.insert(request.name.clone()) {
            continue;
        }
        let name = request.name.clone();
        let task_name = name.clone();
        let task = IoTaskPool::get().spawn(async move { uninstall_package(task_name) });
        commands.spawn((
            Name::new(format!("LSP uninstall: {name}")),
            PackageUninstallJob {
                target: request.target,
                name,
                task,
            },
        ));
    }
}

impl Package {
    fn to_lsp_package(&self, root: &Path) -> LspPackage {
        let kind = purl::parse(&self.source_id)
            .map(|source| source.kind)
            .unwrap_or_default();
        let installed = store::is_installed(root, &self.name);
        let on_path = !installed
            && matches!(
                store::resolved_command(root, self.name.as_str()),
                store::Resolution::OnPath
            );
        let catalog_version = purl::parse(&self.source_id).and_then(|source| source.version);
        let installed_version = installed
            .then(|| store::read_receipt(root, &self.name).and_then(|receipt| receipt.version))
            .flatten();
        let outdated = installed
            && installed_version.is_some()
            && catalog_version.is_some()
            && installed_version != catalog_version;
        let status = if outdated {
            LspPkgStatus::Outdated
        } else if installed {
            LspPkgStatus::Installed
        } else if on_path {
            LspPkgStatus::OnPath
        } else {
            LspPkgStatus::Available
        };
        let installable = kind == "github"
            || install::toolchain_for(&kind).is_some_and(crate::lsp::registry::executable_on_path);
        let requires = if installable {
            None
        } else {
            install::toolchain_for(&kind).map(String::from)
        };
        let version = if installed {
            installed_version
        } else {
            catalog_version
        };
        LspPackage {
            name: self.name.as_str().to_string(),
            description: self.description.clone(),
            languages: self.languages.clone(),
            categories: self.categories.clone(),
            status,
            version,
            installable,
            requires,
        }
    }
}

fn on_catalog_request(
    trigger: On<UiInput<LspCatalogRequest>>,
    mut states: Query<&mut ManagerState>,
    jobs: Query<(Entity, &CatalogJob)>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if let Ok(mut state) = states.get_mut(entity) {
        state.start_loading();
    }
    for (job_entity, job) in &jobs {
        if job.target == entity {
            commands.entity(job_entity).despawn();
        }
    }
    commands.spawn((
        Name::new("LSP catalog"),
        PendingCatalogJob {
            target: entity,
            request: trigger.event().payload.clone(),
        },
    ));
}

fn on_install_request(trigger: On<UiInput<LspInstallRequest>>, mut commands: Commands) {
    commands.spawn(PendingPackageInstall {
        target: trigger.event().webview,
        name: trigger.event().payload.name.clone(),
    });
}

fn on_update_request(trigger: On<UiInput<LspUpdateRequest>>, mut commands: Commands) {
    commands.spawn(PendingPackageInstall {
        target: trigger.event().webview,
        name: trigger.event().payload.name.clone(),
    });
}

fn on_uninstall_request(trigger: On<UiInput<LspUninstallRequest>>, mut commands: Commands) {
    commands.spawn(PendingPackageUninstall {
        target: trigger.event().webview,
        name: trigger.event().payload.name.clone(),
    });
}

impl crate::host::editor::FileView {
    fn uses_lsp_package(&self, package: &str) -> bool {
        self.path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(crate::lsp::registry::preferred_package)
            == Some(package)
    }
}

fn poll_catalog_jobs(mut jobs: Query<(Entity, &mut CatalogJob)>, mut commands: Commands) {
    for (entity, mut job) in &mut jobs {
        let Some(catalog) = block_on(future::poll_once(&mut job.task)) else {
            continue;
        };
        commands.spawn(CatalogOutput {
            target: job.target,
            catalog,
        });
        commands.entity(entity).despawn();
    }
}

fn poll_install_jobs(mut jobs: Query<(Entity, &mut PackageInstallJob)>, mut commands: Commands) {
    for (entity, mut job) in &mut jobs {
        for progress in job.progress.try_iter() {
            commands.spawn(PackageProgressOutput {
                target: job.target,
                progress,
            });
        }
        let Some(result) = block_on(future::poll_once(&mut job.task)) else {
            continue;
        };
        match result {
            Ok(status) => {
                commands.spawn(PackageStatusOutput {
                    target: job.target,
                    status,
                });
            }
            Err(progress) => {
                commands.spawn(PackageProgressOutput {
                    target: job.target,
                    progress,
                });
            }
        }
        commands.entity(entity).despawn();
    }
}

fn poll_uninstall_jobs(
    mut jobs: Query<(Entity, &mut PackageUninstallJob)>,
    mut commands: Commands,
) {
    for (entity, mut job) in &mut jobs {
        let Some(result) = block_on(future::poll_once(&mut job.task)) else {
            continue;
        };
        match result {
            Ok(status) => {
                commands.spawn(PackageStatusOutput {
                    target: job.target,
                    status,
                });
            }
            Err(progress) => {
                commands.spawn(PackageProgressOutput {
                    target: job.target,
                    progress,
                });
            }
        }
        commands.entity(entity).despawn();
    }
}

fn deliver_catalog_outputs(
    outputs: Query<(Entity, &CatalogOutput)>,
    mut managers: Query<&mut ManagerState>,
    mut commands: Commands,
) {
    for (output_entity, output) in &outputs {
        if let Ok(mut state) = managers.get_mut(output.target) {
            state.apply_catalog(output.catalog.clone());
        }
        commands.entity(output_entity).despawn();
    }
}

fn deliver_progress_outputs(
    outputs: Query<(Entity, &PackageProgressOutput)>,
    targets: PackageTargets,
    mut managers: Query<&mut ManagerState>,
    mut commands: Commands,
) {
    for (output_entity, output) in &outputs {
        if let Ok(mut state) = managers.get_mut(output.target) {
            state.apply_progress(output.progress.clone());
        }
        for target in targets.matching(output.target, &output.progress.name) {
            if targets.contains(target) {
                commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                    target,
                    &output.progress,
                ));
            }
        }
        commands.entity(output_entity).despawn();
    }
}

fn deliver_status_outputs(
    outputs: Query<(Entity, &PackageStatusOutput)>,
    targets: PackageTargets,
    mut managers: Query<&mut ManagerState>,
    mut commands: Commands,
) {
    for (output_entity, output) in &outputs {
        if let Ok(mut state) = managers.get_mut(output.target) {
            state.apply_status(output.status.clone());
        }
        let matching = targets.matching(output.target, &output.status.name);
        if output.status.status == LspPkgStatus::Installed {
            for target in matching.iter().copied() {
                if targets.uses_package(target, &output.status.name) {
                    commands
                        .entity(target)
                        .remove::<crate::lsp::manager::LspOpened>()
                        .remove::<crate::lsp::manager::LspStatusSent>();
                }
            }
        }
        for target in matching {
            if targets.contains(target) {
                commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                    target,
                    &output.status,
                ));
            }
        }
        commands.entity(output_entity).despawn();
    }
}

fn publish_manager_state(
    pages: Query<(Entity, Ref<ManagerState>), With<LspManagerPage>>,
    mut commands: Commands,
) {
    for (entity, state) in &pages {
        if !state.is_changed() {
            continue;
        }
        commands.trigger(UiStateWrite::<LspManagerUiState>::from_event(
            entity,
            &state.event(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::catalog::Package;

    fn pkg(name: &str, source_id: &str) -> Package {
        Package {
            name: crate::lsp::package_path::PackageName::parse(name).unwrap(),
            description: String::new(),
            languages: vec![],
            categories: vec![],
            source_id: source_id.into(),
            assets: vec![],
            bin: Default::default(),
        }
    }

    #[test]
    fn installability_by_source() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let gh = pkg("zzz-fake-lsp", "pkg:github/x/zzz-fake-lsp@1").to_lsp_package(root);
        assert!(gh.installable);
        assert_eq!(gh.requires, None);
        assert_eq!(gh.status, LspPkgStatus::Available);

        let np = pkg("zzz-fake-ts", "pkg:npm/zzz-fake-ts@1").to_lsp_package(root);
        let npm_present = crate::lsp::registry::executable_on_path("npm");
        assert_eq!(np.installable, npm_present);
        assert_eq!(np.requires.is_some(), !npm_present);

        let uk = pkg("weird", "pkg:weirdsrc/weird@1").to_lsp_package(root);
        assert!(!uk.installable);
        assert_eq!(uk.requires, None);
    }

    #[test]
    fn installed_with_newer_catalog_is_outdated() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(store::packages_dir(root).join("foo")).unwrap();
        let mut bin = std::collections::BTreeMap::new();
        let name = crate::lsp::package_path::PackageName::parse("foo").unwrap();
        bin.insert(
            name.clone(),
            crate::lsp::package_path::PackagePath::parse("foo-bin").unwrap(),
        );
        store::write_receipt(
            root,
            &name,
            &store::Receipt {
                name: name.clone(),
                version: Some("1.0".into()),
                source_id: "pkg:github/x/foo@1.0".into(),
                bin,
            },
        )
        .unwrap();
        let lp = pkg("foo", "pkg:github/x/foo@2.0").to_lsp_package(root);
        assert_eq!(lp.status, LspPkgStatus::Outdated);
        assert_eq!(lp.version.as_deref(), Some("1.0"));
    }

    #[test]
    fn manager_state_applies_progress_and_final_status() {
        let mut state = ManagerState {
            packages: vec![LspPackage {
                name: "rust-analyzer".into(),
                description: String::new(),
                languages: vec!["Rust".into()],
                categories: Vec::new(),
                status: LspPkgStatus::Available,
                version: None,
                installable: true,
                requires: None,
            }],
            ..Default::default()
        };

        state.apply_progress(LspInstallProgress {
            name: "rust-analyzer".into(),
            phase: InstallPhase::Downloading,
            pct: Some(50),
            message: "Downloading".into(),
        });

        assert_eq!(state.packages[0].status, LspPkgStatus::Installing);
        assert_eq!(state.progress.len(), 1);

        state.apply_status(LspPackageStatus {
            name: "rust-analyzer".into(),
            status: LspPkgStatus::Installed,
            version: Some("1.0".into()),
        });

        assert_eq!(state.packages[0].status, LspPkgStatus::Installed);
        assert_eq!(state.packages[0].version.as_deref(), Some("1.0"));
        assert!(state.progress.is_empty());
    }
}
