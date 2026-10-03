use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, block_on, futures_lite::future};
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashSet;
use vmux_ecs::event::{InstallPhase, LspInstallProgress, LspPackageStatus, LspPkgStatus};
#[cfg(test)]
use vmux_path::Executable;

use crate::lsp::catalog::{CatalogReady, Package};
use crate::lsp::registry::ServerSpec;
use crate::lsp::{store, target::PlatformTarget};

pub struct ManagerPlugin;

impl Plugin for ManagerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PackageInstallRequest>()
            .add_systems(
                Update,
                (enqueue_package_installs, start_install_jobs).chain(),
            )
            .add_systems(Update, poll_install_jobs)
            .add_systems(
                PostUpdate,
                (deliver_progress_outputs, deliver_status_outputs).chain(),
            );
    }
}

#[derive(Clone, Message)]
pub(crate) struct PackageInstallRequest {
    pub target: Entity,
    pub name: String,
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
struct PendingPackageInstall {
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

fn install_package(
    package: Package,
    progress: Sender<LspInstallProgress>,
) -> Result<LspPackageStatus, LspInstallProgress> {
    let store = store::PackageStore::lsp();
    let name = package.name.as_str().to_string();
    let target = PlatformTarget::current();
    let progress_name = name.clone();
    let result = package.install(&store, target, |phase, pct, message| {
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

fn start_install_jobs(
    pending: Query<(Entity, &PendingPackageInstall)>,
    catalogs: Query<(), With<CatalogReady>>,
    packages: Query<&Package>,
    install_jobs: Query<&PackageInstallJob>,
    mut commands: Commands,
) {
    if catalogs.is_empty() {
        return;
    }
    let mut names = install_jobs
        .iter()
        .map(|job| job.name.clone())
        .collect::<HashSet<_>>();
    for (entity, request) in &pending {
        commands.entity(entity).despawn();
        if !names.insert(request.name.clone()) {
            continue;
        }
        let Some(package) = packages
            .iter()
            .find(|package| package.name.as_str() == request.name)
            .cloned()
        else {
            commands.spawn(PackageProgressOutput {
                target: request.target,
                progress: LspInstallProgress {
                    name: request.name.clone(),
                    phase: InstallPhase::Failed,
                    pct: None,
                    message: "package not found in catalog".into(),
                },
            });
            continue;
        };
        let (progress_sender, progress) = crossbeam_channel::unbounded();
        let name = request.name.clone();
        let task =
            IoTaskPool::get().spawn(async move { install_package(package, progress_sender) });
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

fn enqueue_package_installs(
    mut requests: MessageReader<PackageInstallRequest>,
    mut commands: Commands,
) {
    for request in requests.read() {
        commands.spawn(PendingPackageInstall {
            target: request.target,
            name: request.name.clone(),
        });
    }
}

impl crate::host::editor::FileView {
    fn uses_lsp_package(&self, package: &str) -> bool {
        self.path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(ServerSpec::preferred_package)
            == Some(package)
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

fn deliver_progress_outputs(
    outputs: Query<(Entity, &PackageProgressOutput)>,
    targets: PackageTargets,
    mut commands: Commands,
) {
    for (output_entity, output) in &outputs {
        for target in targets.matching(output.target, &output.progress.name) {
            if targets.contains(target) {
                commands.trigger(vmux_ecs::host::FileUiStateWrite::from_event(
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
    mut commands: Commands,
) {
    for (output_entity, output) in &outputs {
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
                commands.trigger(vmux_ecs::host::FileUiStateWrite::from_event(
                    target,
                    &output.status,
                ));
            }
        }
        commands.entity(output_entity).despawn();
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
        let store = store::PackageStore::at(tmp.path());
        let gh = pkg("zzz-fake-lsp", "pkg:github/x/zzz-fake-lsp@1").snapshot(&store);
        assert!(gh.installable);
        assert_eq!(gh.requires, None);
        assert_eq!(gh.status, LspPkgStatus::Available);

        let np = pkg("zzz-fake-ts", "pkg:npm/zzz-fake-ts@1").snapshot(&store);
        let npm_present = Executable::find("npm").is_some();
        assert_eq!(np.installable, npm_present);
        assert_eq!(np.requires.is_some(), !npm_present);

        let uk = pkg("weird", "pkg:weirdsrc/weird@1").snapshot(&store);
        assert!(!uk.installable);
        assert_eq!(uk.requires, None);
    }

    #[test]
    fn installed_with_newer_catalog_is_outdated() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store::PackageStore::at(tmp.path());
        std::fs::create_dir_all(store.packages_dir().join("foo")).unwrap();
        let mut bin = std::collections::BTreeMap::new();
        let name = crate::lsp::package_path::PackageName::parse("foo").unwrap();
        bin.insert(
            name.clone(),
            crate::lsp::package_path::PackagePath::parse("foo-bin").unwrap(),
        );
        store
            .write_receipt(
                &name,
                &store::Receipt {
                    name: name.clone(),
                    version: Some("1.0".into()),
                    source_id: "pkg:github/x/foo@1.0".into(),
                    bin,
                },
            )
            .unwrap();
        let lp = pkg("foo", "pkg:github/x/foo@2.0").snapshot(&store);
        assert_eq!(lp.status, LspPkgStatus::Outdated);
        assert_eq!(lp.version.as_deref(), Some("1.0"));
    }
}
