use std::collections::BTreeSet;
use std::path::PathBuf;

use bevy::prelude::*;
use vmux_ecs::{EntityTarget, Operation, OperationRetained, OperationState};
use vmux_job::{JobRequest, JobResult};

use crate::io::{InspectLink, ReadDirectory};
use crate::observation::{Accepted, Finished, Observation, ObserveProviderRequest, ObservedTool};
use crate::state::{ToolOperationKind, ToolStatus};
use crate::tool_inventory::{Manifest, Scan};
use crate::{ToolProviderId, ToolStore, ToolTaskSet};

pub(crate) struct DotfileInventoryPlugin;

impl Plugin for DotfileInventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(observe).add_systems(
            Update,
            (packages, directories, links, publish_packages, publish)
                .chain()
                .in_set(ToolTaskSet::Start),
        );
    }
}

#[derive(Component)]
struct Listing;

#[derive(Component)]
struct Observing;

#[derive(Component)]
struct Paths {
    root: PathBuf,
    home: PathBuf,
}

#[derive(Component)]
struct Package;

#[derive(Component)]
struct PackageRoot(PathBuf);

#[derive(Component)]
struct Discovered;

#[derive(Component, Default)]
struct EntryCount(usize);

#[derive(Component)]
struct PackageError(String);

#[derive(Component)]
struct PackageReady;

#[derive(Component)]
struct Directory {
    package: Entity,
    depth: usize,
}

#[derive(Component)]
struct DirectoryDone;

#[derive(Component)]
struct Link;

fn observe(
    trigger: On<ObserveProviderRequest>,
    providers: Query<&ToolProviderId>,
    observations: Query<(&ChildOf, &OperationState), With<Observation>>,
    scans: Query<&ToolStore, With<Scan>>,
    mut commands: Commands,
) {
    let request = trigger.event();
    if !providers
        .get(request.entity)
        .is_ok_and(|provider| provider.0.id() == "dotfiles")
    {
        return;
    }
    let Ok((parent, state)) = observations.get(request.observation) else {
        return;
    };
    if *state != OperationState::Running {
        return;
    }
    let Ok(store) = scans.get(parent.parent()) else {
        return;
    };
    commands.entity(request.observation).insert((
        Accepted,
        Listing,
        Paths {
            root: store.dotfiles_dir(),
            home: store.home().to_path_buf(),
        },
        JobRequest(ReadDirectory {
            path: store.dotfiles_dir(),
            limit: 4096,
        }),
    ));
}

fn packages(
    requests: Query<
        (
            Entity,
            &ChildOf,
            &Paths,
            &OperationState,
            &JobResult<ReadDirectory>,
        ),
        With<Listing>,
    >,
    manifests: Query<&Manifest>,
    mut commands: Commands,
) {
    for (entity, parent, paths, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Listing, JobResult<ReadDirectory>)>();
        let entries = match &result.0 {
            Ok(Ok(entries)) => entries,
            Ok(Err(error)) | Err(error) => {
                commands.entity(entity).insert(Finished {
                    error: Some(error.clone()),
                });
                continue;
            }
        };
        if entries.exists && !entries.is_directory {
            commands.entity(entity).insert(Finished {
                error: Some("dotfile root is not a directory".into()),
            });
            continue;
        }
        let Ok(manifest) = manifests.get(parent.parent()) else {
            continue;
        };
        let mut discovered = BTreeSet::new();
        for entry in &entries.entries {
            let Some(name) = entry.name.to_str() else {
                continue;
            };
            if entry.file_type.is_dir()
                && !name.is_empty()
                && name != "."
                && name != ".."
                && !name.starts_with('.')
                && !name.contains(['/', '\\'])
            {
                discovered.insert(name.to_string());
            }
        }
        let mut names = discovered.clone();
        names.extend(manifest.desired.dotfiles.packages.iter().cloned());
        for name in names {
            let root = paths.root.join(&name);
            let package = commands
                .spawn((
                    Name::new(name.clone()),
                    crate::observation::Id(name.clone()),
                    Package,
                    PackageRoot(root.clone()),
                    EntryCount::default(),
                    ChildOf(entity),
                    EntityTarget::<Observation>::new(entity),
                    OperationRetained,
                    Operation::bundle("tool.dotfiles.observe")
                        .unwrap()
                        .running(),
                ))
                .id();
            if discovered.contains(&name) {
                commands.entity(package).insert(Discovered);
            }
            if name.is_empty()
                || name == "."
                || name == ".."
                || name.starts_with('.')
                || name.contains(['/', '\\'])
            {
                commands.entity(package).insert(PackageError(format!(
                    "invalid dotfile package name: {name}"
                )));
            } else {
                commands.entity(package).insert((
                    Directory { package, depth: 0 },
                    JobRequest(ReadDirectory {
                        path: root,
                        limit: 4096,
                    }),
                ));
            }
        }
        commands.entity(entity).insert(Observing);
    }
}

fn directories(
    requests: Query<
        (
            Entity,
            &Directory,
            &EntityTarget<Observation>,
            &OperationState,
            &JobResult<ReadDirectory>,
        ),
        Without<DirectoryDone>,
    >,
    paths: Query<&Paths>,
    mut packages: Query<(&PackageRoot, &mut EntryCount), With<Package>>,
    mut commands: Commands,
) {
    for (entity, directory, target, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<JobResult<ReadDirectory>>()
            .insert((DirectoryDone, OperationState::Succeeded));
        let Ok((root, mut count)) = packages.get_mut(directory.package) else {
            continue;
        };
        let Ok(paths) = paths.get(target.entity()) else {
            continue;
        };
        let entries = match &result.0 {
            Ok(Ok(entries)) if entries.exists && entries.is_directory => entries,
            Ok(Ok(_)) => {
                commands
                    .entity(directory.package)
                    .insert(PackageError("dotfile package does not exist".into()));
                continue;
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(directory.package)
                    .insert(PackageError(error.clone()));
                continue;
            }
        };
        for entry in &entries.entries {
            count.0 += 1;
            if count.0 > 65536 || directory.depth >= 64 {
                commands.entity(directory.package).insert(PackageError(
                    "dotfile package exceeds traversal bounds".into(),
                ));
                break;
            }
            if entry.file_type.is_dir() {
                commands.spawn((
                    Name::new("Dotfile directory"),
                    ChildOf(directory.package),
                    EntityTarget::<Observation>::new(target.entity()),
                    OperationRetained,
                    Operation::bundle("tool.dotfiles.directory")
                        .unwrap()
                        .running(),
                    Directory {
                        package: directory.package,
                        depth: directory.depth + 1,
                    },
                    JobRequest(ReadDirectory {
                        path: entry.path.clone(),
                        limit: 4096,
                    }),
                ));
            } else if entry.file_type.is_file() {
                let Ok(relative) = entry.path.strip_prefix(&root.0) else {
                    continue;
                };
                commands.spawn((
                    Name::new("Dotfile link"),
                    Link,
                    ChildOf(directory.package),
                    EntityTarget::<Observation>::new(target.entity()),
                    OperationRetained,
                    Operation::bundle("tool.dotfiles.link").unwrap().running(),
                    JobRequest(InspectLink {
                        source: entry.path.clone(),
                        target: paths.home.join(relative),
                    }),
                ));
            }
        }
    }
}

fn links(
    requests: Query<(Entity, &OperationState, &JobResult<InspectLink>), With<Link>>,
    mut commands: Commands,
) {
    for (entity, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let status = match &result.0 {
            Ok(Ok(facts)) if facts.target_type.is_none() => ToolStatus::Missing,
            Ok(Ok(facts))
                if facts.target_type.is_some_and(|kind| kind.is_symlink())
                    && facts.target.as_ref() == Some(&facts.source) =>
            {
                ToolStatus::Installed
            }
            Ok(Ok(_)) => ToolStatus::Conflict,
            _ => ToolStatus::Missing,
        };
        commands
            .entity(entity)
            .remove::<JobResult<InspectLink>>()
            .insert((
                crate::observation::Status(status),
                OperationState::Succeeded,
            ));
    }
}

fn publish_packages(
    packages: Query<
        (
            Entity,
            &Name,
            &crate::observation::Id,
            &ChildOf,
            Has<Discovered>,
            Option<&PackageError>,
        ),
        (With<Package>, Without<PackageReady>),
    >,
    directories: Query<(&Directory, Has<DirectoryDone>)>,
    links: Query<(&ChildOf, Option<&crate::observation::Status>), With<Link>>,
    observations: Query<(&ChildOf, &OperationState), With<Observation>>,
    manifests: Query<&Manifest>,
    mut commands: Commands,
) {
    for (entity, name, id, parent, discovered, error) in &packages {
        let Ok((scan, state)) = observations.get(parent.parent()) else {
            continue;
        };
        if *state != OperationState::Running {
            continue;
        }
        if directories
            .iter()
            .any(|(directory, done)| directory.package == entity && !done)
            || links
                .iter()
                .any(|(parent, status)| parent.parent() == entity && status.is_none())
        {
            continue;
        }
        let Ok(manifest) = manifests.get(scan.parent()) else {
            continue;
        };
        let managed = discovered || manifest.desired.dotfiles.packages.contains(&id.0);
        let mut linked = 0;
        let mut missing = 0;
        let mut conflicts = 0;
        for (owner, status) in &links {
            if owner.parent() != entity {
                continue;
            }
            match status.unwrap().0 {
                ToolStatus::Installed => linked += 1,
                ToolStatus::Missing => missing += 1,
                _ => conflicts += 1,
            }
        }
        let (status, detail, operations) = if let Some(error) = error {
            (
                ToolStatus::Missing,
                error.0.clone(),
                if managed {
                    vec![ToolOperationKind::Unlink]
                } else {
                    Vec::new()
                },
            )
        } else {
            let status = if conflicts > 0 {
                ToolStatus::Conflict
            } else if missing > 0 {
                if managed {
                    ToolStatus::Missing
                } else {
                    ToolStatus::Available
                }
            } else {
                ToolStatus::Installed
            };
            (
                status,
                format!("{linked} linked · {missing} missing · {conflicts} conflicts"),
                if managed {
                    vec![ToolOperationKind::Link, ToolOperationKind::Unlink]
                } else {
                    vec![ToolOperationKind::Link]
                },
            )
        };
        commands.entity(entity).insert((
            ObservedTool::new(
                id.0.clone(),
                name.as_str().to_string(),
                None,
                None,
                detail,
                status,
                operations,
            ),
            crate::observation::Dotfile,
            PackageReady,
        ));
        if discovered {
            commands
                .entity(entity)
                .insert(crate::observation::Discovered);
        }
    }
}

fn publish(
    observations: Query<(Entity, &OperationState), (With<Observing>, Without<Finished>)>,
    packages: Query<(&ChildOf, Has<PackageReady>), With<Package>>,
    mut commands: Commands,
) {
    for (entity, state) in &observations {
        if *state == OperationState::Running
            && packages
                .iter()
                .all(|(parent, ready)| parent.parent() != entity || ready)
        {
            commands.entity(entity).insert(Finished { error: None });
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn directory_and_link_observations_preserve_home_files_and_adopt_discovered_packages() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        let store = ToolStore::new(directory.path().join("config"), home.clone());
        let package = store.dotfiles_dir().join("shell");
        std::fs::create_dir_all(package.join("config")).unwrap();
        std::fs::create_dir_all(home.join("config")).unwrap();
        std::fs::write(package.join(".shellrc"), "linked").unwrap();
        std::fs::write(package.join("config/settings"), "source").unwrap();
        std::fs::write(package.join("missing"), "missing").unwrap();
        std::fs::write(home.join("config/settings"), "keep this file").unwrap();
        std::os::unix::fs::symlink(package.join(".shellrc"), home.join(".shellrc")).unwrap();
        let mut original = crate::ToolsManifest::default();
        original.set_dotfile_package("missing-declared", true);
        original.write_to(&store.manifest_path()).unwrap();
        let mut app = App::new();
        app.add_plugins((
            bevy::app::TaskPoolPlugin::default(),
            crate::ToolRuntimePlugin,
            crate::dotfiles::DotfileToolPlugin,
        ));
        let registry = app
            .world_mut()
            .spawn((
                crate::tool_inventory::Registry::default(),
                crate::catalog::Catalog::default(),
                store.clone(),
                crate::ToolsManifest::default(),
                crate::tool_inventory::Sequence::default(),
            ))
            .id();
        app.world_mut().spawn((
            ToolProviderId(crate::state::ToolProvider::new("dotfiles")),
            crate::observation::Handler,
        ));
        app.world_mut()
            .trigger(crate::tool_inventory::RefreshRequest {
                entity: registry,
                refresh_catalogs: false,
            });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !app
            .world()
            .get::<crate::catalog::Catalog>(registry)
            .unwrap()
            .loaded
        {
            app.update();
            assert!(
                std::time::Instant::now() < deadline,
                "dotfile observation did not drain"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let mut state =
            bevy::ecs::system::SystemState::<crate::catalog::Inventory>::new(app.world_mut());
        let snapshot = state.get(app.world()).unwrap().snapshot(registry).unwrap();
        let items = &snapshot.categories[0].items;
        let shell = items.iter().find(|item| item.id == "shell").unwrap();
        assert_eq!(shell.status, ToolStatus::Conflict);
        assert!(shell.managed);
        assert_eq!(shell.detail, "1 linked · 1 missing · 1 conflicts");
        assert_eq!(
            shell.operations,
            [ToolOperationKind::Link, ToolOperationKind::Unlink]
        );
        assert_eq!(
            items
                .iter()
                .find(|item| item.id == "missing-declared")
                .unwrap()
                .status,
            ToolStatus::Missing
        );
        assert_eq!(
            std::fs::read_to_string(home.join("config/settings")).unwrap(),
            "keep this file"
        );
        assert!(!home.join("missing").exists());
        assert_eq!(
            std::fs::read_link(home.join(".shellrc")).unwrap(),
            package.join(".shellrc")
        );
        let saved = crate::ToolsManifest::read(&store.manifest_path()).unwrap();
        assert_eq!(saved.dotfiles.packages, ["missing-declared", "shell"]);
        assert!(store.lease().is_some());
    }
}
