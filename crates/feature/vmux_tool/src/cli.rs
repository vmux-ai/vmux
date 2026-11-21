use std::path::PathBuf;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::OperationState;
use vmux_ecs::cli::{CliInvocation, CliResult};
use vmux_feature::manifest::FeaturePlugin;

use crate::dotfiles::{
    AdoptDotfile, AdoptedDotfile, AppliedEnabledDotfiles, ApplyEnabledDotfiles,
    DisableDotfilePackage, DisabledDotfilePackage, DiscoverDotfilePackages,
    DiscoveredDotfilePackages, DotfilePlan, ImportAvailableDotfiles, ImportDotfiles,
    ImportedAvailableDotfiles, ImportedDotfiles, PlanDotfilePackage,
};
use crate::homebrew::ImportedBrewfile;
use crate::npm::ImportedNpmManifest;
use crate::storage::{ReadRequest, ReadResult, StoragePlugin, StoreLease};
use crate::{
    ImportBrewfile, ImportNpmManifest, ToolOperationFailed, ToolRuntimePlugin, ToolStore,
    ToolStoreTarget, ToolsManifest,
};

pub struct ToolCliPlugin;

impl Plugin for ToolCliPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<StoragePlugin>() {
            app.add_plugins(StoragePlugin);
        }
        if !app.is_plugin_added::<ToolRuntimePlugin>() {
            app.add_plugins(ToolRuntimePlugin);
        }
        if !app.is_plugin_added::<crate::homebrew::HomebrewToolPlugin>() {
            app.add_plugins(crate::homebrew::HomebrewToolPlugin);
        }
        if !app.is_plugin_added::<crate::npm::NpmToolPlugin>() {
            app.add_plugins(crate::npm::NpmToolPlugin);
        }
        if !app.is_plugin_added::<crate::dotfiles::DotfileToolPlugin>() {
            app.add_plugins(crate::dotfiles::DotfileToolPlugin);
        }
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .add_systems(
                Update,
                (
                    route,
                    execute_status,
                    status_cancelled,
                    status_discovered,
                    status_reported,
                    execute_apply,
                    execute_homebrew_import,
                    execute_npm_import,
                    execute_dotfile_import,
                    execute_adopt,
                    execute_unlink,
                    complete_dotfiles,
                    complete_available,
                    complete_adoption,
                    complete_physical,
                    complete_imports,
                )
                    .chain(),
            );
    }
}

#[derive(Component)]
struct ToolStatusRequest;

#[derive(Component)]
struct StatusSource(ToolStore);

#[derive(Component)]
struct StatusSnapshot(ToolsManifest);

#[derive(Component)]
struct StatusDiscovery;

#[derive(Component)]
struct StatusPlan;

#[derive(Component)]
struct Reported;

#[derive(Component)]
struct StatusCancelled;

#[derive(Component)]
struct ToolApplyRequest;

#[derive(Component)]
struct WaitingDotfiles;

#[derive(Component)]
struct WaitingAvailable;

#[derive(Component)]
struct WaitingAdoption;

#[derive(Component)]
struct WaitingPhysical;

#[derive(Component)]
struct HomebrewImportRequest(PathBuf);

#[derive(Component)]
struct NpmImportRequest(PathBuf);

#[derive(Component)]
struct WaitingNpm;

#[derive(Component)]
struct WaitingBrewfile;

#[derive(Component)]
struct DotfileImportRequest(Option<PathBuf>);

#[derive(Component)]
struct DotfileAdoptRequest {
    path: PathBuf,
    package: String,
}

#[derive(Component)]
struct DotfileUnlinkRequest(String);

fn route(
    invocations: Query<(Entity, &CliInvocation, Option<&ToolStore>), Added<CliInvocation>>,
    mut commands: Commands,
) {
    for (entity, invocation, store) in &invocations {
        let store = store.cloned().unwrap_or_else(ToolStore::current);
        let mut entity = commands.entity(entity);
        match invocation.command.as_str() {
            "tool.status" => {
                entity.insert((
                    ToolStatusRequest,
                    store.clone(),
                    StatusSource(store.clone()),
                    ReadRequest { retain_lease: true },
                ));
            }
            "tool.apply" => {
                entity.insert((ToolApplyRequest, store.clone()));
            }
            "tool.import.homebrew" => {
                if let Some(path) = invocation.value_os("path") {
                    entity.insert((HomebrewImportRequest(PathBuf::from(path)), store.clone()));
                } else {
                    entity.insert(CliResult(Err("Brewfile path is required".into())));
                }
            }
            "tool.import.npm" => {
                if let Some(path) = invocation.value_os("path") {
                    entity.insert((NpmImportRequest(PathBuf::from(path)), store.clone()));
                } else {
                    entity.insert(CliResult(Err("package.json path is required".into())));
                }
            }
            "tool.import.dotfiles" => {
                entity.insert((
                    DotfileImportRequest(invocation.value_os("path").map(PathBuf::from)),
                    store.clone(),
                ));
            }
            "tool.adopt" => {
                if let (Some(path), Some(package)) =
                    (invocation.value_os("path"), invocation.value("package"))
                {
                    entity.insert((
                        DotfileAdoptRequest {
                            path: PathBuf::from(path),
                            package: package.to_string(),
                        },
                        store.clone(),
                    ));
                }
            }
            "tool.unlink" => {
                if let Some(package) = invocation.value("package") {
                    entity.insert((DotfileUnlinkRequest(package.to_string()), store.clone()));
                } else {
                    entity.insert(CliResult(Err("dotfile package is required".into())));
                }
            }
            _ => {}
        }
    }
}

fn execute_status(
    requests: Query<
        (
            Entity,
            Option<&ToolStore>,
            &StatusSource,
            &ReadResult,
            Option<&StoreLease>,
            Has<StatusCancelled>,
        ),
        With<ToolStatusRequest>,
    >,
    mut commands: Commands,
) {
    for (entity, store, source, loaded, lease, cancelled) in &requests {
        commands
            .entity(entity)
            .remove::<(ToolStatusRequest, ReadResult)>();
        let manifest = match &loaded.0 {
            Ok(manifest) => manifest,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.clone())))
                    .remove::<(StoreLease, StatusSource, StatusCancelled)>();
                continue;
            }
        };
        let Some(lease) = lease else {
            commands
                .entity(entity)
                .insert(CliResult(Err(
                    "tool status read did not retain its store lease".into(),
                )))
                .remove::<(StatusSource, StatusCancelled)>();
            continue;
        };
        let Some(store) =
            store.filter(|store| !cancelled && **store == source.0 && lease._lease.matches(store))
        else {
            commands
                .entity(entity)
                .insert(CliResult(Err("tool status source changed".into())))
                .remove::<(StoreLease, StatusSource, StatusCancelled)>();
            continue;
        };
        println!("{}", store.root().display());
        for (provider, packages) in &manifest.packages {
            println!("{provider} ({})", packages.len());
            for package in packages {
                println!("  {package}");
            }
        }
        if !manifest.mcp.servers.is_empty() {
            println!("mcp ({})", manifest.mcp.servers.len());
            for (name, server) in &manifest.mcp.servers {
                println!("  {name} · {:?}", server.transport);
            }
        }
        commands.entity(entity).insert((
            StatusSource(store.clone()),
            StatusSnapshot(manifest.clone()),
        ));
        commands.spawn((
            vmux_ecs::Operation::bundle("tool.dotfile.status.discovery").unwrap(),
            vmux_ecs::OperationRetained,
            bevy_ecs::hierarchy::ChildOf(entity),
            StatusDiscovery,
            store.clone(),
            crate::dotfile_ops::SharedReadLease {
                store: store.clone(),
                lease: lease.clone(),
            },
            DiscoverDotfilePackages,
        ));
    }
}

fn status_discovered(
    discovery: Query<
        (
            Entity,
            &ChildOf,
            Option<&DiscoveredDotfilePackages>,
            Option<&ToolOperationFailed>,
            Has<crate::ToolOperationFinished>,
        ),
        With<StatusDiscovery>,
    >,
    status: Query<
        (
            &StatusSource,
            &StatusSnapshot,
            &StoreLease,
            Has<StatusCancelled>,
        ),
        Without<CliResult>,
    >,
    drain: crate::dotfile_ops::DotfileDrain,
    mut commands: Commands,
) {
    for (entity, parent, packages, error, _) in &discovery {
        let Ok((source, snapshot, lease, cancelled)) = status.get(parent.parent()) else {
            continue;
        };
        if cancelled && drain.is_drained(entity) {
            commands
                .entity(parent.parent())
                .insert(CliResult(Err("tool status source changed".into())))
                .remove::<(
                    StatusSource,
                    StatusSnapshot,
                    ToolStore,
                    StatusCancelled,
                    StoreLease,
                )>();
            commands.entity(entity).despawn();
            continue;
        }
        if let Some(error) = error {
            commands
                .entity(parent.parent())
                .insert(CliResult(Err(error.0.clone())))
                .remove::<(StatusSource, StatusSnapshot, ToolStore, StoreLease)>();
            commands.entity(entity).despawn();
            continue;
        }
        let Some(packages) = packages else {
            continue;
        };
        let mut names = packages.packages.clone();
        for name in &snapshot.0.dotfiles.packages {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        names.sort();
        if names.is_empty() {
            commands
                .entity(parent.parent())
                .insert(CliResult::default())
                .remove::<(StatusSource, StatusSnapshot, ToolStore, StoreLease)>();
        } else {
            println!("dotfiles ({})", names.len());
            for (order, name) in names.into_iter().enumerate() {
                commands.spawn((
                    vmux_ecs::Operation::bundle("tool.dotfile.status.plan").unwrap(),
                    vmux_ecs::OperationRetained,
                    ChildOf(parent.parent()),
                    StatusPlan,
                    Name::new(name.clone()),
                    vmux_ecs::Order(
                        u32::try_from(order).expect("bounded dotfile status package count"),
                    ),
                    source.0.clone(),
                    crate::dotfile_ops::SharedReadLease {
                        store: source.0.clone(),
                        lease: lease.clone(),
                    },
                    PlanDotfilePackage::new(name),
                ));
            }
        }
        commands.entity(entity).despawn();
    }
}

fn status_reported(
    status: Query<(Entity, &StatusSnapshot, Has<StatusCancelled>), Without<CliResult>>,
    mut plans: Query<
        (
            Entity,
            &ChildOf,
            &Name,
            &vmux_ecs::Order,
            Option<&DotfilePlan>,
            Option<&ToolOperationFailed>,
            Has<Reported>,
            Has<crate::ToolOperationFinished>,
        ),
        With<StatusPlan>,
    >,
    discovery: Query<&ChildOf, With<StatusDiscovery>>,
    drain: crate::dotfile_ops::DotfileDrain,
    mut commands: Commands,
) {
    for (entity, snapshot, cancelled) in &status {
        if discovery.iter().any(|parent| parent.parent() == entity) {
            continue;
        }
        let mut ordered = Vec::new();
        for (plan, parent, _, order, _, _, reported, _) in &plans {
            if parent.parent() == entity && !reported {
                ordered.push((order.0, plan));
            }
        }
        ordered.sort_by_key(|(order, _)| *order);
        let mut pending = false;
        for (_, plan) in ordered {
            let Ok((_, _, name, _, output, error, _, _)) = plans.get_mut(plan) else {
                continue;
            };
            if cancelled && drain.is_drained(plan) {
            } else if let Some(output) = output {
                println!(
                    "  {}{} · {} linked · {} missing · {} conflicts",
                    name.as_str(),
                    if snapshot
                        .0
                        .dotfiles
                        .packages
                        .iter()
                        .any(|package| package == name.as_str())
                    {
                        " [managed]"
                    } else {
                        ""
                    },
                    output.linked(),
                    output.missing(),
                    output.conflicts()
                );
            } else if let Some(error) = error {
                println!("  {} · {}", name.as_str(), error.0);
            } else {
                pending = true;
                break;
            }
            commands.entity(plan).insert(Reported);
        }
        if pending {
            continue;
        }
        for (plan, parent, ..) in &plans {
            if parent.parent() == entity {
                commands.entity(plan).despawn();
            }
        }
        let result = if cancelled {
            CliResult(Err("tool status source changed".into()))
        } else {
            CliResult::default()
        };
        commands.entity(entity).insert(result).remove::<(
            StatusSource,
            StatusSnapshot,
            ToolStore,
            StatusCancelled,
            StoreLease,
        )>();
    }
}

fn status_cancelled(
    status: Query<
        (
            Entity,
            &StatusSource,
            Option<&ToolStore>,
            Option<&OperationState>,
        ),
        Without<CliResult>,
    >,
    mut children: Query<
        (&ChildOf, &mut OperationState),
        (
            Or<(With<StatusPlan>, With<StatusDiscovery>)>,
            Without<StatusSource>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, source, current, state) in &status {
        let valid = current == Some(&source.0)
            && state.is_none_or(|state| {
                matches!(state, OperationState::Pending | OperationState::Running)
            });
        if valid {
            continue;
        }
        commands.entity(entity).insert(StatusCancelled);
        for (parent, mut state) in &mut children {
            if parent.parent() == entity {
                *state = OperationState::Canceled;
            }
        }
    }
}

fn execute_apply(requests: Query<Entity, Added<ToolApplyRequest>>, mut commands: Commands) {
    for entity in &requests {
        commands
            .entity(entity)
            .remove::<ToolApplyRequest>()
            .insert((WaitingDotfiles, ApplyEnabledDotfiles));
    }
}

fn execute_homebrew_import(
    requests: Query<(Entity, &HomebrewImportRequest, &ToolStore), Added<HomebrewImportRequest>>,
    mut commands: Commands,
) {
    for (entity, request, store) in &requests {
        let store = commands.spawn((store.clone(), ChildOf(entity))).id();
        commands
            .entity(entity)
            .remove::<HomebrewImportRequest>()
            .insert((
                WaitingBrewfile,
                ImportBrewfile::new(request.0.clone()),
                ToolStoreTarget::new(store),
            ));
    }
}

fn execute_npm_import(
    requests: Query<(Entity, &NpmImportRequest, &ToolStore), Added<NpmImportRequest>>,
    mut commands: Commands,
) {
    for (entity, request, store) in &requests {
        let store = commands.spawn((store.clone(), ChildOf(entity))).id();
        commands
            .entity(entity)
            .remove::<NpmImportRequest>()
            .insert((
                WaitingNpm,
                ImportNpmManifest::new(request.0.clone()),
                ToolStoreTarget::new(store),
            ));
    }
}

fn complete_imports(
    requests: Query<
        (
            Entity,
            Option<&ImportedNpmManifest>,
            Option<&ImportedBrewfile>,
            Option<&ToolOperationFailed>,
            Option<&OperationState>,
        ),
        (
            Or<(With<WaitingNpm>, With<WaitingBrewfile>)>,
            Without<ReadRequest>,
            Without<crate::storage::WriteRequest>,
            Without<crate::storage::FileReadRequest>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, npm, brew, failure, state) in &requests {
        let result = if state == Some(&OperationState::Canceled) {
            CliResult(Err("tool operation was canceled".into()))
        } else if let Some(failure) = failure {
            CliResult(Err(failure.0.clone()))
        } else if let Some(npm) = npm {
            println!("imported {} NPM package(s)", npm.packages);
            CliResult::default()
        } else if let Some(brew) = brew {
            println!(
                "imported {} formulae and {} casks",
                brew.formulae, brew.casks
            );
            CliResult::default()
        } else {
            continue;
        };
        commands.entity(entity).insert(result).remove::<(
            WaitingNpm,
            WaitingBrewfile,
            ImportNpmManifest,
            ImportBrewfile,
        )>();
    }
}

fn execute_dotfile_import(
    requests: Query<(Entity, &DotfileImportRequest), Added<DotfileImportRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        let Some(path) = &request.0 else {
            commands
                .entity(entity)
                .remove::<DotfileImportRequest>()
                .insert((WaitingAvailable, ImportAvailableDotfiles));
            continue;
        };
        commands
            .entity(entity)
            .remove::<DotfileImportRequest>()
            .insert((WaitingPhysical, ImportDotfiles::new(path.clone())));
    }
}

fn complete_physical(
    requests: Query<
        (
            Entity,
            Option<&ImportedDotfiles>,
            Option<&ToolOperationFailed>,
            Option<&OperationState>,
            Has<crate::ToolOperationFinished>,
        ),
        With<WaitingPhysical>,
    >,
    mut commands: Commands,
    drain: crate::dotfile_staging::ImportDrain,
) {
    for (entity, imported, failure, state, finished) in &requests {
        if !drain.is_drained(entity) {
            continue;
        }
        let result = if state == Some(&OperationState::Canceled) {
            CliResult(Err("dotfile import canceled".into()))
        } else if !finished {
            continue;
        } else if let Some(failure) = failure {
            CliResult(Err(failure.0.clone()))
        } else if let Some(imported) = imported {
            println!("imported {} dotfile package(s)", imported.packages);
            CliResult::default()
        } else {
            continue;
        };
        commands
            .entity(entity)
            .insert(result)
            .remove::<(WaitingPhysical, ImportDotfiles, ToolStore)>();
    }
}

fn execute_adopt(
    requests: Query<(Entity, &DotfileAdoptRequest), Added<DotfileAdoptRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        commands
            .entity(entity)
            .remove::<DotfileAdoptRequest>()
            .insert((
                WaitingAdoption,
                AdoptDotfile::new(request.path.clone(), request.package.clone()),
            ));
    }
}

fn complete_adoption(
    requests: Query<
        (
            Entity,
            Option<&AdoptedDotfile>,
            Option<&ToolOperationFailed>,
            Option<&OperationState>,
            Has<crate::ToolOperationFinished>,
        ),
        With<WaitingAdoption>,
    >,
    mut commands: Commands,
    drain: crate::dotfile_ops::DotfileDrain,
) {
    for (entity, adopted, failure, state, finished) in &requests {
        if !drain.is_drained(entity) {
            continue;
        }
        let result = if state == Some(&OperationState::Canceled) {
            CliResult(Err("dotfile adoption canceled".into()))
        } else if !finished {
            continue;
        } else if let Some(failure) = failure {
            CliResult(Err(failure.0.clone()))
        } else if let Some(adopted) = adopted {
            println!("{}", adopted.path.display());
            CliResult::default()
        } else {
            continue;
        };
        commands
            .entity(entity)
            .insert(result)
            .remove::<(WaitingAdoption, AdoptDotfile, ToolStore)>();
    }
}

fn execute_unlink(
    requests: Query<(Entity, &DotfileUnlinkRequest), Added<DotfileUnlinkRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        commands
            .entity(entity)
            .remove::<DotfileUnlinkRequest>()
            .insert((
                WaitingDotfiles,
                DisableDotfilePackage::new(request.0.clone()),
            ));
    }
}

fn complete_dotfiles(
    requests: Query<
        (
            Entity,
            Option<&AppliedEnabledDotfiles>,
            Option<&DisabledDotfilePackage>,
            Option<&ToolOperationFailed>,
            Option<&OperationState>,
            Has<crate::ToolOperationFinished>,
        ),
        With<WaitingDotfiles>,
    >,
    mut commands: Commands,
    drain: crate::dotfile_ops::DotfileDrain,
) {
    for (entity, applied, disabled, failure, state, finished) in &requests {
        if !drain.is_drained(entity) {
            continue;
        }
        let result = if state == Some(&OperationState::Canceled) {
            CliResult(Err("dotfile operation canceled".into()))
        } else if !finished {
            continue;
        } else if let Some(failure) = failure {
            CliResult(Err(failure.0.clone()))
        } else if let Some(applied) = applied {
            println!("linked {} file(s)", applied.files);
            CliResult::default()
        } else if let Some(disabled) = disabled {
            println!("unlinked {} file(s)", disabled.files);
            CliResult::default()
        } else {
            continue;
        };
        commands.entity(entity).insert(result).remove::<(
            WaitingDotfiles,
            ApplyEnabledDotfiles,
            DisableDotfilePackage,
            ToolStore,
        )>();
    }
}

fn complete_available(
    requests: Query<
        (
            Entity,
            Option<&ImportedAvailableDotfiles>,
            Option<&ToolOperationFailed>,
            Option<&OperationState>,
            Has<crate::ToolOperationFinished>,
        ),
        With<WaitingAvailable>,
    >,
    mut commands: Commands,
    drain: crate::dotfile_ops::DotfileDrain,
) {
    for (entity, imported, failure, state, finished) in &requests {
        if !drain.is_drained(entity) {
            continue;
        }
        let result = if state == Some(&OperationState::Canceled) {
            CliResult(Err("dotfile import canceled".into()))
        } else if !finished {
            continue;
        } else if let Some(failure) = failure {
            CliResult(Err(failure.0.clone()))
        } else if let Some(imported) = imported {
            println!("imported {} dotfile package(s)", imported.packages);
            CliResult::default()
        } else {
            continue;
        };
        commands.entity(entity).insert(result).remove::<(
            WaitingAvailable,
            ImportAvailableDotfiles,
            ToolStore,
        )>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::time::{Duration, Instant};

    #[test]
    fn cli_adoption_uses_the_selected_home_and_commits_after_linking() {
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(
            directory.path().join("config"),
            directory.path().join("home"),
        );
        ToolsManifest::default()
            .write_to(&store.manifest_path())
            .unwrap();
        std::fs::create_dir_all(store.home()).unwrap();
        let source = store.home().join(".adopt-qa");
        std::fs::write(&source, b"original").unwrap();
        let mut app = App::new();
        app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolCliPlugin));
        let request = app
            .world_mut()
            .spawn((
                store.clone(),
                CliInvocation {
                    command: "tool.adopt".into(),
                    arguments: BTreeMap::from([
                        ("path".into(), vec![source.clone().into_os_string()]),
                        ("package".into(), vec!["shell".into()]),
                    ]),
                },
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.world().get::<CliResult>(request).is_none() {
            app.update();
            assert!(Instant::now() < deadline, "CLI adoption did not drain");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(app.world().get::<CliResult>(request).unwrap().0, Ok(0));
        assert!(source.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(&source).unwrap(), b"original");
        assert_eq!(
            std::fs::read(store.dotfiles_dir().join("shell/.adopt-qa")).unwrap(),
            b"original"
        );
        assert_eq!(
            ToolsManifest::read(&store.manifest_path())
                .unwrap()
                .dotfiles
                .packages,
            ["shell"]
        );
        assert!(store.lease().is_some());
    }

    #[test]
    fn cli_available_import_uses_the_selected_store_and_preserves_other_packages() {
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(
            directory.path().join("config"),
            directory.path().join("home"),
        );
        let mut manifest = ToolsManifest::default();
        manifest.set_package("npm", "preserved", true);
        manifest.write_to(&store.manifest_path()).unwrap();
        let package = store.dotfiles_dir().join("shell");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(package.join(".zshrc"), b"shell").unwrap();
        let mut app = App::new();
        app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolCliPlugin));
        let request = app
            .world_mut()
            .spawn((
                store.clone(),
                CliInvocation {
                    command: "tool.import.dotfiles".into(),
                    arguments: BTreeMap::new(),
                },
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.world().get::<CliResult>(request).is_none() {
            app.update();
            assert!(Instant::now() < deadline, "available import did not drain");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(app.world().get::<CliResult>(request).unwrap().0, Ok(0));
        let saved = ToolsManifest::read(&store.manifest_path()).unwrap();
        assert_eq!(saved.dotfiles.packages, ["shell"]);
        assert_eq!(saved.packages["npm"], ["preserved"]);
        assert!(store.lease().is_some());
    }

    #[test]
    fn cli_physical_import_uses_the_selected_store_and_refuses_duplicate_packages() {
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(
            directory.path().join("config"),
            directory.path().join("home"),
        );
        let mut manifest = ToolsManifest::default();
        manifest.set_package("npm", "preserved", true);
        manifest.write_to(&store.manifest_path()).unwrap();
        let input = directory.path().join("input");
        std::fs::create_dir_all(input.join("shell")).unwrap();
        std::fs::write(input.join("shell/.zshrc"), b"shell").unwrap();
        let mut app = App::new();
        app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolCliPlugin));
        for duplicate in [false, true] {
            let request = app
                .world_mut()
                .spawn((
                    store.clone(),
                    CliInvocation {
                        command: "tool.import.dotfiles".into(),
                        arguments: BTreeMap::from([("path".into(), vec![OsString::from(&input)])]),
                    },
                ))
                .id();
            let deadline = Instant::now() + Duration::from_secs(10);
            while app.world().get::<CliResult>(request).is_none() {
                app.update();
                assert!(
                    Instant::now() < deadline,
                    "physical CLI import did not drain"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(
                app.world().get::<CliResult>(request).unwrap().0.is_err(),
                duplicate
            );
            assert_eq!(
                std::fs::read(store.dotfiles_dir().join("shell/.zshrc")).unwrap(),
                b"shell"
            );
            let saved = ToolsManifest::read(&store.manifest_path()).unwrap();
            assert_eq!(saved.dotfiles.packages, ["shell"]);
            assert_eq!(saved.packages["npm"], ["preserved"]);
            assert!(store.lease().is_some());
            app.world_mut().despawn(request);
        }
    }

    #[test]
    fn cli_apply_and_unlink_share_dotfile_operations_for_the_selected_store() {
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(
            directory.path().join("config"),
            directory.path().join("home"),
        );
        let mut manifest = ToolsManifest::default();
        manifest.set_dotfile_package("shell", true);
        manifest.write_to(&store.manifest_path()).unwrap();
        let source = store.dotfiles_dir().join("shell/.zshrc");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"shell").unwrap();
        let target = store.home().join(".zshrc");
        let mut app = App::new();
        app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolCliPlugin));
        for command in ["tool.apply", "tool.unlink"] {
            let request = app
                .world_mut()
                .spawn((
                    store.clone(),
                    CliInvocation {
                        command: command.into(),
                        arguments: BTreeMap::from([("package".into(), vec!["shell".into()])]),
                    },
                ))
                .id();
            let deadline = Instant::now() + Duration::from_secs(5);
            while app.world().get::<CliResult>(request).is_none() {
                app.update();
                assert!(Instant::now() < deadline, "{command} did not drain");
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(app.world().get::<CliResult>(request).unwrap().0, Ok(0));
            if command == "tool.apply" {
                assert_eq!(std::fs::read(&target).unwrap(), b"shell");
                assert_eq!(
                    vmux_path::PathIdentity::resolve(&target),
                    vmux_path::PathIdentity::resolve(&source)
                );
            } else {
                assert!(target.symlink_metadata().is_err());
                assert!(
                    ToolsManifest::read(&store.manifest_path())
                        .unwrap()
                        .dotfiles
                        .packages
                        .is_empty()
                );
            }
            assert!(store.lease().is_some(), "{command}");
            app.world_mut().despawn(request);
        }
    }

    #[test]
    fn cli_status_uses_canonical_dotfile_plans_for_the_selected_store() {
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(
            directory.path().join("config"),
            directory.path().join("home"),
        );
        let mut manifest = ToolsManifest::default();
        manifest.set_dotfile_package("shell", true);
        manifest.set_dotfile_package("missing", true);
        manifest.write_to(&store.manifest_path()).unwrap();
        let package = store.dotfiles_dir().join("shell");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(package.join(".zshrc"), b"shell").unwrap();
        let before = std::fs::read(store.manifest_path()).unwrap();
        let mut app = App::new();
        app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolCliPlugin));
        let request = app
            .world_mut()
            .spawn((
                store.clone(),
                CliInvocation {
                    command: "tool.status".into(),
                    arguments: BTreeMap::new(),
                },
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.world().get::<CliResult>(request).is_none() {
            app.update();
            assert!(Instant::now() < deadline, "CLI status did not complete");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(app.world().get::<CliResult>(request).unwrap().0, Ok(0));
        assert_eq!(std::fs::read(store.manifest_path()).unwrap(), before);
        assert!(store.lease().is_some());
        assert!(
            app.world_mut()
                .query_filtered::<Entity, With<StatusPlan>>()
                .iter(app.world())
                .next()
                .is_none()
        );
    }

    #[test]
    fn cli_status_cancellation_and_store_replacement_drain_owned_plans() {
        for interruption in ["cancel", "store"] {
            let directory = tempfile::tempdir().unwrap();
            let store = ToolStore::new(
                directory.path().join("config"),
                directory.path().join("home"),
            );
            ToolsManifest::default()
                .write_to(&store.manifest_path())
                .unwrap();
            let package = store.dotfiles_dir().join("shell");
            std::fs::create_dir_all(&package).unwrap();
            std::fs::write(package.join(".zshrc"), b"shell").unwrap();
            let mut app = App::new();
            app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolCliPlugin));
            let request = app
                .world_mut()
                .spawn((
                    store.clone(),
                    CliInvocation {
                        command: "tool.status".into(),
                        arguments: BTreeMap::new(),
                    },
                ))
                .id();
            let deadline = Instant::now() + Duration::from_secs(5);
            while app.world().get::<StatusSnapshot>(request).is_none() {
                app.update();
                assert!(
                    Instant::now() < deadline,
                    "CLI status did not accept its source"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            if interruption == "cancel" {
                app.world_mut()
                    .entity_mut(request)
                    .insert(OperationState::Canceled);
            } else {
                app.world_mut().entity_mut(request).insert(ToolStore::new(
                    directory.path().join("replacement"),
                    directory.path().join("other-home"),
                ));
            }
            while app.world().get::<CliResult>(request).is_none() {
                app.update();
                assert!(
                    Instant::now() < deadline,
                    "cancelled CLI status did not drain"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(
                app.world().get::<CliResult>(request).unwrap().0.is_err(),
                "{interruption}"
            );
            assert!(store.lease().is_some(), "{interruption}");
        }
    }

    #[test]
    fn cli_imports_use_the_selected_store_and_preserve_existing_packages_and_brewfile_comments() {
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(directory.path(), directory.path());
        let mut original = ToolsManifest::default();
        original.set_package("npm", "preserved", true);
        original.write_to(&store.manifest_path()).unwrap();
        let mut app = App::new();
        app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolCliPlugin));
        for (command, path, contents) in [
            (
                "tool.import.homebrew",
                directory.path().join("input.Brewfile"),
                "tap \"homebrew/cask-fonts\"\n# keep\nbrew \"ripgrep\", link: false\n",
            ),
            (
                "tool.import.npm",
                directory.path().join("package.json"),
                r#"{"dependencies":{"eslint":"1"}}"#,
            ),
        ] {
            std::fs::write(&path, contents).unwrap();
            let entity = app
                .world_mut()
                .spawn((
                    store.clone(),
                    CliInvocation {
                        command: command.into(),
                        arguments: BTreeMap::from([("path".into(), vec![OsString::from(path)])]),
                    },
                ))
                .id();
            let deadline = Instant::now() + Duration::from_secs(3);
            while app.world().get::<CliResult>(entity).is_none() {
                app.update();
                assert!(Instant::now() < deadline, "CLI import did not complete");
                std::thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(app.world().get::<CliResult>(entity).unwrap().0, Ok(0));
            app.world_mut().despawn(entity);
        }
        let saved = ToolsManifest::read(&store.manifest_path()).unwrap();
        assert_eq!(saved.packages["npm"], ["eslint", "preserved"]);
        assert_eq!(saved.packages["homebrew-formula"], ["ripgrep"]);
        assert_eq!(
            std::fs::read_to_string(store.brewfile_path()).unwrap(),
            "tap \"homebrew/cask-fonts\"\n# keep\nbrew \"ripgrep\", link: false\n"
        );
        assert!(!store.is_busy());
    }

    #[test]
    fn status_holds_one_store_read_until_all_link_counts_are_reported() {
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(
            directory.path().join("config"),
            directory.path().join("home"),
        );
        let root = store.dotfiles_dir().join("shell");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(store.home()).unwrap();
        std::fs::write(root.join(".zshrc"), b"shell").unwrap();
        std::os::unix::fs::symlink(root.join(".zshrc"), store.home().join(".zshrc")).unwrap();
        let mut manifest = ToolsManifest::default();
        manifest.set_dotfile_package("shell", true);
        manifest.write_to(&store.manifest_path()).unwrap();
        let mut app = App::new();
        app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolCliPlugin));
        let status = app
            .world_mut()
            .spawn((
                store.clone(),
                CliInvocation {
                    command: "tool.status".into(),
                    arguments: BTreeMap::new(),
                },
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.world().get::<StatusSnapshot>(status).is_none() {
            app.update();
            assert!(
                Instant::now() < deadline,
                "status did not accept its store read"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let disable = app
            .world_mut()
            .spawn((
                vmux_ecs::Operation::bundle("test.status.concurrent.disable").unwrap(),
                store.clone(),
                DisableDotfilePackage::new("shell"),
            ))
            .id();
        while app.world().get::<CliResult>(status).is_none() {
            assert!(store.home().join(".zshrc").symlink_metadata().is_ok());
            assert!(
                ToolsManifest::read(&store.manifest_path())
                    .unwrap()
                    .dotfiles
                    .packages
                    .contains(&"shell".to_string())
            );
            app.update();
            assert!(Instant::now() < deadline, "shared status read deadlocked");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(app.world().get::<CliResult>(status).unwrap().0, Ok(0));
        while app.world().get::<DisabledDotfilePackage>(disable).is_none() {
            app.update();
            assert!(
                Instant::now() < deadline,
                "status did not release its lease"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(store.home().join(".zshrc").symlink_metadata().is_err());
        assert!(
            ToolsManifest::read(&store.manifest_path())
                .unwrap()
                .dotfiles
                .packages
                .is_empty()
        );
        assert!(store.lease().is_some());
    }

    #[test]
    fn cancelled_cli_dotfile_requests_return_errors_before_and_after_admission() {
        for (command, kind) in [
            ("tool.apply", "apply"),
            ("tool.unlink", "unlink"),
            ("tool.adopt", "adopt"),
            ("tool.import.dotfiles", "available"),
            ("tool.import.dotfiles", "physical"),
        ] {
            for admitted in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let store = ToolStore::new(
                    directory.path().join("config"),
                    directory.path().join("home"),
                );
                let root = store.dotfiles_dir().join("shell");
                std::fs::create_dir_all(&root).unwrap();
                std::fs::create_dir_all(store.home()).unwrap();
                std::fs::write(root.join(".zshrc"), b"shell").unwrap();
                let original = store.home().join(".adopt-original");
                std::fs::write(&original, b"original").unwrap();
                let input = directory.path().join("input");
                std::fs::create_dir_all(input.join("incoming")).unwrap();
                std::fs::write(input.join("incoming/.profile"), b"incoming").unwrap();
                let mut manifest = ToolsManifest::default();
                manifest.set_dotfile_package("shell", true);
                manifest.write_to(&store.manifest_path()).unwrap();
                let before = std::fs::read(store.manifest_path()).unwrap();
                let blocker = store.lease().unwrap();
                let mut app = App::new();
                app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolCliPlugin));
                let mut arguments =
                    BTreeMap::from([("package".into(), vec![OsString::from("shell")])]);
                if kind == "adopt" {
                    arguments.insert("path".into(), vec![original.clone().into_os_string()]);
                } else if kind == "physical" {
                    arguments.insert("path".into(), vec![input.into_os_string()]);
                }
                let request = app
                    .world_mut()
                    .spawn((
                        store.clone(),
                        CliInvocation {
                            command: command.into(),
                            arguments,
                        },
                    ))
                    .id();
                if admitted {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while app
                        .world()
                        .get::<crate::ToolStoreOperation>(request)
                        .is_none()
                    {
                        app.update();
                        assert!(Instant::now() < deadline, "CLI request was not admitted");
                    }
                }
                app.world_mut()
                    .entity_mut(request)
                    .insert(OperationState::Canceled);
                drop(blocker);
                let deadline = Instant::now() + Duration::from_secs(5);
                while app.world().get::<CliResult>(request).is_none() {
                    app.update();
                    assert!(
                        Instant::now() < deadline,
                        "canceled CLI request did not drain"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(app.world().get::<CliResult>(request).unwrap().0.is_err());
                assert_eq!(std::fs::read(store.manifest_path()).unwrap(), before);
                assert!(store.home().join(".zshrc").symlink_metadata().is_err());
                assert_eq!(std::fs::read(&original).unwrap(), b"original");
                assert!(!store.dotfiles_dir().join("incoming").exists());
                assert!(store.lease().is_some());
            }
        }
    }

    #[test]
    fn status_read_failures_and_pre_cancel_release_the_store() {
        for cancelled in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let store = ToolStore::new(
                directory.path().join("config"),
                directory.path().join("home"),
            );
            std::fs::create_dir_all(store.root()).unwrap();
            std::fs::write(store.manifest_path(), b"not a valid TOML document").unwrap();
            let mut app = App::new();
            app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolCliPlugin));
            let request = app
                .world_mut()
                .spawn((
                    store.clone(),
                    CliInvocation {
                        command: "tool.status".into(),
                        arguments: BTreeMap::new(),
                    },
                ))
                .id();
            if cancelled {
                app.world_mut()
                    .entity_mut(request)
                    .insert(OperationState::Canceled);
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            while app.world().get::<CliResult>(request).is_none() {
                app.update();
                assert!(Instant::now() < deadline, "status failure did not return");
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(app.world().get::<CliResult>(request).unwrap().0.is_err());
            assert!(store.lease().is_some());
        }
    }

    #[test]
    fn status_reports_package_directories_among_more_than_4096_ordinary_entries() {
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(
            directory.path().join("config"),
            directory.path().join("home"),
        );
        std::fs::create_dir_all(store.dotfiles_dir().join("shell")).unwrap();
        for index in 0..4097 {
            std::fs::write(
                store.dotfiles_dir().join(format!("entry-{index}")),
                b"ordinary",
            )
            .unwrap();
        }
        ToolsManifest::default()
            .write_to(&store.manifest_path())
            .unwrap();
        let mut app = App::new();
        app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolCliPlugin));
        let request = app
            .world_mut()
            .spawn((
                store.clone(),
                CliInvocation {
                    command: "tool.status".into(),
                    arguments: BTreeMap::new(),
                },
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.world().get::<CliResult>(request).is_none() {
            app.update();
            assert!(
                Instant::now() < deadline,
                "large-directory status did not complete"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(app.world().get::<CliResult>(request).unwrap().0, Ok(0));
        assert!(store.lease().is_some());
        let before = std::fs::read(store.manifest_path()).unwrap();
        let import = app
            .world_mut()
            .spawn((
                store.clone(),
                CliInvocation {
                    command: "tool.import.dotfiles".into(),
                    arguments: BTreeMap::new(),
                },
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.world().get::<CliResult>(import).is_none() {
            app.update();
            assert!(Instant::now() < deadline, "oversized import did not refuse");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.world().get::<CliResult>(import).unwrap().0.is_err());
        assert_eq!(std::fs::read(store.manifest_path()).unwrap(), before);
        assert!(store.lease().is_some());
    }

    #[test]
    fn missing_cli_import_path_refuses_without_creating_storage() {
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(directory.path(), directory.path());
        let mut app = App::new();
        app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolCliPlugin));
        for command in ["tool.import.homebrew", "tool.import.npm"] {
            let entity = app
                .world_mut()
                .spawn((
                    store.clone(),
                    CliInvocation {
                        command: command.into(),
                        arguments: BTreeMap::new(),
                    },
                ))
                .id();
            app.update();
            assert!(app.world().get::<CliResult>(entity).unwrap().0.is_err());
        }
        assert!(!store.root().exists());
        assert!(!store.is_busy());
    }
}
