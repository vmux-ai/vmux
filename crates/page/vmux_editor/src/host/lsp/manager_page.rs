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
use vmux_core::host::{UiState, UiStatePlugin};
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
            .add_systems(Update, (poll_catalog_jobs, poll_package_jobs))
            .add_systems(
                PostUpdate,
                (deliver_manager_outputs, publish_manager_state).chain(),
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

enum ManagerMsg {
    Catalog(LspCatalog),
    Progress(LspInstallProgress),
    Status(LspPackageStatus),
}

#[derive(Component)]
struct ManagerOutput {
    target: Entity,
    message: ManagerMsg,
}

impl ManagerOutput {
    fn new(target: Entity, message: ManagerMsg) -> Self {
        Self { target, message }
    }

    fn targets(&self, views: &Query<(Entity, &crate::host::editor::FileView)>) -> Vec<Entity> {
        let package = match &self.message {
            ManagerMsg::Catalog(_) => return vec![self.target],
            ManagerMsg::Progress(event) => event.name.as_str(),
            ManagerMsg::Status(event) => event.name.as_str(),
        };
        let mut targets = vec![self.target];
        for (entity, view) in views {
            if view.uses_lsp_package(package) && !targets.contains(&entity) {
                targets.push(entity);
            }
        }
        targets
    }
}

#[derive(Component)]
struct CatalogJob {
    target: Entity,
    task: Task<LspCatalog>,
}

impl CatalogJob {
    fn spawn(target: Entity, request: LspCatalogRequest) -> Self {
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
        Self { target, task }
    }
}

#[derive(Clone, Copy)]
enum PackageOperation {
    Install,
    Uninstall,
}

impl PackageOperation {
    fn run(self, name: String, progress: Sender<LspInstallProgress>) -> ManagerMsg {
        match self {
            Self::Install => Self::install(name, progress),
            Self::Uninstall => Self::uninstall(name),
        }
    }

    fn install(name: String, progress: Sender<LspInstallProgress>) -> ManagerMsg {
        let root = store::default_root();
        let packages = catalog::ensure_catalog(&root, false).unwrap_or_default();
        let Some(package) = packages
            .iter()
            .find(|package| package.name.as_str() == name)
            .cloned()
        else {
            return ManagerMsg::Progress(LspInstallProgress {
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
            Ok(receipt) => ManagerMsg::Status(LspPackageStatus {
                name,
                status: LspPkgStatus::Installed,
                version: receipt.version,
            }),
            Err(error) => ManagerMsg::Progress(LspInstallProgress {
                name,
                phase: InstallPhase::Failed,
                pct: None,
                message: error,
            }),
        }
    }

    fn uninstall(name: String) -> ManagerMsg {
        let root = store::default_root();
        let Ok(package) = crate::lsp::package_path::PackageName::parse(&name) else {
            return ManagerMsg::Progress(LspInstallProgress {
                name,
                phase: InstallPhase::Failed,
                pct: None,
                message: "invalid package name".to_string(),
            });
        };
        if let Err(error) = store::remove(&root, &package) {
            return ManagerMsg::Progress(LspInstallProgress {
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
        ManagerMsg::Status(LspPackageStatus {
            name,
            status,
            version: None,
        })
    }
}

#[derive(Component)]
struct PackageJob {
    target: Entity,
    name: String,
    progress: Receiver<LspInstallProgress>,
    task: Task<ManagerMsg>,
}

impl PackageJob {
    fn spawn(target: Entity, name: String, operation: PackageOperation) -> Self {
        let (progress_sender, progress) = crossbeam_channel::unbounded();
        let task_name = name.clone();
        let task =
            IoTaskPool::get().spawn(async move { operation.run(task_name, progress_sender) });
        Self {
            target,
            name,
            progress,
            task,
        }
    }

    fn start(
        target: Entity,
        name: String,
        operation: PackageOperation,
        jobs: &Query<&Self>,
        commands: &mut Commands,
    ) {
        if jobs.iter().any(|job| job.name == name) {
            return;
        }
        let operation_name = match operation {
            PackageOperation::Install => "install",
            PackageOperation::Uninstall => "uninstall",
        };
        commands.spawn((
            Name::new(format!("LSP {operation_name}: {name}")),
            Self::spawn(target, name, operation),
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
        CatalogJob::spawn(entity, trigger.event().payload.clone()),
    ));
}

fn on_install_request(
    trigger: On<UiInput<LspInstallRequest>>,
    jobs: Query<&PackageJob>,
    mut commands: Commands,
) {
    PackageJob::start(
        trigger.event().webview,
        trigger.event().payload.name.clone(),
        PackageOperation::Install,
        &jobs,
        &mut commands,
    );
}

fn on_update_request(
    trigger: On<UiInput<LspUpdateRequest>>,
    jobs: Query<&PackageJob>,
    mut commands: Commands,
) {
    PackageJob::start(
        trigger.event().webview,
        trigger.event().payload.name.clone(),
        PackageOperation::Install,
        &jobs,
        &mut commands,
    );
}

fn on_uninstall_request(
    trigger: On<UiInput<LspUninstallRequest>>,
    jobs: Query<&PackageJob>,
    mut commands: Commands,
) {
    PackageJob::start(
        trigger.event().webview,
        trigger.event().payload.name.clone(),
        PackageOperation::Uninstall,
        &jobs,
        &mut commands,
    );
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
        let Some(event) = block_on(future::poll_once(&mut job.task)) else {
            continue;
        };
        commands.spawn(ManagerOutput::new(job.target, ManagerMsg::Catalog(event)));
        commands.entity(entity).despawn();
    }
}

fn poll_package_jobs(mut jobs: Query<(Entity, &mut PackageJob)>, mut commands: Commands) {
    for (entity, mut job) in &mut jobs {
        for event in job.progress.try_iter() {
            commands.spawn(ManagerOutput::new(job.target, ManagerMsg::Progress(event)));
        }
        let Some(message) = block_on(future::poll_once(&mut job.task)) else {
            continue;
        };
        commands.spawn(ManagerOutput::new(job.target, message));
        commands.entity(entity).despawn();
    }
}

fn deliver_manager_outputs(
    outputs: Query<(Entity, &ManagerOutput)>,
    views: Query<(Entity, &crate::host::editor::FileView)>,
    mut managers: Query<&mut ManagerState>,
    mut commands: Commands,
) {
    for (output_entity, output) in &outputs {
        match &output.message {
            ManagerMsg::Catalog(ev) => {
                if let Ok(mut state) = managers.get_mut(output.target) {
                    state.apply_catalog(ev.clone());
                }
            }
            ManagerMsg::Progress(ev) => {
                if let Ok(mut state) = managers.get_mut(output.target) {
                    state.apply_progress(ev.clone());
                }
                for target in output.targets(&views) {
                    if views.contains(target) {
                        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(target, ev));
                    }
                }
            }
            ManagerMsg::Status(ev) => {
                if let Ok(mut state) = managers.get_mut(output.target) {
                    state.apply_status(ev.clone());
                }
                let targets = output.targets(&views);
                if ev.status == LspPkgStatus::Installed {
                    for target in targets.iter().copied() {
                        if views
                            .get(target)
                            .is_ok_and(|(_, view)| view.uses_lsp_package(&ev.name))
                        {
                            commands
                                .entity(target)
                                .remove::<crate::lsp::manager::LspOpened>()
                                .remove::<crate::lsp::manager::LspStatusSent>();
                        }
                    }
                }
                for target in targets {
                    if views.contains(target) {
                        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(target, ev));
                    }
                }
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
        UiState::<LspManagerUiState>::write(&mut commands, entity, &state.event());
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
