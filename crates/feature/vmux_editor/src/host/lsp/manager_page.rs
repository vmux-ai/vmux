use bevy::prelude::*;
use std::collections::HashSet;
use std::time::Duration;
use vmux_ecs::event::{
    InstallPhase, LspInstallNotice, LspInstallProgress, LspPackageStatus, LspPkgStatus,
};

use crate::lsp::catalog::{CatalogReady, Package};
use crate::lsp::registry::LspRegistry;
use crate::lsp::{install_runtime, store};
use vmux_ecs::profile::CurrentProfile;
use vmux_ecs::{Operation, OperationRetained, OperationState};

const DONE_NOTICE: Duration = Duration::from_millis(2_500);
const FAILED_NOTICE: Duration = Duration::from_millis(6_000);

pub struct ManagerPlugin;

impl Plugin for ManagerPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<install_runtime::InstallPlugin>() {
            app.add_plugins(install_runtime::InstallPlugin);
        }
        app.add_message::<PackageInstallRequest>()
            .add_systems(
                Update,
                (enqueue_package_installs, start_install_jobs)
                    .chain()
                    .before(poll_install_jobs),
            )
            .add_systems(
                Update,
                poll_install_jobs.before(install_runtime::InstallSet),
            )
            .add_systems(Update, clear_notices)
            .add_systems(
                PostUpdate,
                (
                    deliver_progress_outputs,
                    ApplyDeferred,
                    deliver_status_outputs,
                )
                    .chain(),
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

#[derive(Component)]
struct PackageNotice {
    target: Entity,
}

#[derive(Component)]
struct PackageNoticeTimer(Timer);

#[derive(bevy::ecs::system::SystemParam)]
struct PackageTargets<'w, 's> {
    views: Query<'w, 's, (Entity, &'static crate::host::editor::FileView)>,
    registry: Single<'w, 's, &'static LspRegistry>,
}

impl PackageTargets<'_, '_> {
    fn matching(&self, target: Entity, package: &str) -> Vec<Entity> {
        let mut targets = vec![target];
        for (entity, view) in &self.views {
            if self.view_uses_package(view, package) && !targets.contains(&entity) {
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
            .is_ok_and(|(_, view)| self.view_uses_package(view, package))
    }

    fn view_uses_package(&self, view: &crate::host::editor::FileView, package: &str) -> bool {
        view.path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(|extension| self.registry.package(extension))
            == Some(package)
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct PackageNotices<'w, 's> {
    active: Query<'w, 's, (Entity, &'static PackageNotice)>,
}

impl PackageNotices<'_, '_> {
    fn publish(
        &self,
        target: Entity,
        progress: LspInstallProgress,
        installed: bool,
        duration: Option<Duration>,
        commands: &mut Commands,
    ) {
        for (entity, notice) in &self.active {
            if notice.target == target {
                commands.entity(entity).despawn();
            }
        }
        commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
            target,
            &LspInstallNotice {
                progress: Some(progress),
                installed,
            },
        ));
        let mut notice =
            commands.spawn((Name::new("LSP install notice"), PackageNotice { target }));
        if let Some(duration) = duration {
            notice.insert(PackageNoticeTimer(Timer::new(duration, TimerMode::Once)));
        }
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
    store: store::PackageStore,
    work: Entity,
    reported: bool,
}

fn start_install_jobs(
    pending: Query<(Entity, &PendingPackageInstall)>,
    catalogs: Query<(), With<CatalogReady>>,
    packages: Query<(Entity, &Package)>,
    install_jobs: Query<&PackageInstallJob>,
    profile: CurrentProfile,
    mut retries: MessageWriter<install_runtime::Retry>,
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
            for job in &install_jobs {
                if job.name == request.name {
                    retries.write(install_runtime::Retry { entity: job.work });
                }
            }
            continue;
        }
        let Some((package, _)) = packages
            .iter()
            .find(|(_, package)| package.name.as_str() == request.name)
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
        let name = request.name.clone();
        let store = match profile.paths() {
            Some(paths) => store::PackageStore::at(paths.lsp()),
            None => store::PackageStore::lsp(),
        };
        let work = commands
            .spawn((
                Name::new(format!("LSP install: {name}")),
                Operation::bundle("lsp.package.install").unwrap(),
                OperationRetained,
                install_runtime::Request {
                    package,
                    store: store.clone(),
                },
            ))
            .id();
        commands.spawn((
            Name::new(format!("LSP install: {name}")),
            PackageInstallJob {
                target: request.target,
                name,
                store,
                work,
                reported: false,
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

fn poll_install_jobs(
    mut jobs: Query<(Entity, &mut PackageInstallJob)>,
    progress: Query<&install_runtime::Progress, Changed<install_runtime::Progress>>,
    completed: Query<&install_runtime::Completed>,
    released: Query<(), With<install_runtime::Released>>,
    views: Query<(), With<crate::host::editor::FileView>>,
    profile: CurrentProfile,
    mut commands: Commands,
) {
    let store = match profile.paths() {
        Some(paths) => store::PackageStore::at(paths.lsp()),
        None => store::PackageStore::lsp(),
    };
    for (entity, mut job) in &mut jobs {
        let current = views.contains(job.target) && store == job.store;
        if !current {
            commands
                .entity(job.work)
                .try_insert(OperationState::Canceled);
        }
        if current && let Ok(progress) = progress.get(job.work) {
            commands.spawn(PackageProgressOutput {
                target: job.target,
                progress: progress.0.clone(),
            });
        }
        if current
            && !job.reported
            && let Ok(result) = completed.get(job.work)
        {
            match &result.0 {
                Ok(receipt) => {
                    commands.spawn(PackageStatusOutput {
                        target: job.target,
                        status: LspPackageStatus {
                            name: job.name.clone(),
                            status: LspPkgStatus::Installed,
                            version: receipt.version.clone(),
                        },
                    });
                }
                Err(error) => {
                    commands.spawn(PackageProgressOutput {
                        target: job.target,
                        progress: LspInstallProgress {
                            name: job.name.clone(),
                            phase: InstallPhase::Failed,
                            pct: None,
                            message: error.clone(),
                        },
                    });
                }
            }
            job.reported = true;
        }
        if released.contains(job.work) {
            commands.entity(job.work).despawn();
            commands.entity(entity).despawn();
        }
    }
}

fn deliver_progress_outputs(
    outputs: Query<(Entity, &PackageProgressOutput)>,
    targets: PackageTargets,
    notices: PackageNotices,
    mut commands: Commands,
) {
    for (output_entity, output) in &outputs {
        for target in targets.matching(output.target, &output.progress.name) {
            if targets.contains(target) {
                let duration = match output.progress.phase {
                    InstallPhase::Done => Some(DONE_NOTICE),
                    InstallPhase::Failed => Some(FAILED_NOTICE),
                    _ => None,
                };
                notices.publish(
                    target,
                    output.progress.clone(),
                    false,
                    duration,
                    &mut commands,
                );
            }
        }
        commands.entity(output_entity).despawn();
    }
}

fn deliver_status_outputs(
    outputs: Query<(Entity, &PackageStatusOutput)>,
    targets: PackageTargets,
    notices: PackageNotices,
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
                notices.publish(
                    target,
                    LspInstallProgress {
                        name: output.status.name.clone(),
                        phase: InstallPhase::Done,
                        pct: Some(100),
                        message: String::new(),
                    },
                    true,
                    Some(DONE_NOTICE),
                    &mut commands,
                );
            }
        }
        commands.entity(output_entity).despawn();
    }
}

fn clear_notices(
    time: Res<Time>,
    mut notices: Query<(Entity, &PackageNotice, &mut PackageNoticeTimer)>,
    targets: PackageTargets,
    mut commands: Commands,
) {
    for (entity, notice, mut timer) in &mut notices {
        timer.0.tick(time.delta());
        if !timer.0.just_finished() {
            continue;
        }
        if targets.contains(notice.target) {
            commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
                notice.target,
                &LspInstallNotice::default(),
            ));
        }
        commands.entity(entity).despawn();
    }
}
