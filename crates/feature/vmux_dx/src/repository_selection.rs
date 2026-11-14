use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::PathBuf;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::Order;
use vmux_ecs::cli::{CliInvocation, CliResult};

use crate::Completed;
use crate::repository::Checkout;
use crate::repository_io::Repository;

pub struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (request, seed, locate, inspect, choose, finish)
                .chain()
                .in_set(Stage::Resolve),
        );
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Stage {
    Resolve,
}

#[derive(Component)]
pub struct Input {
    pub path: PathBuf,
    pub branch: Option<OsString>,
}

#[derive(Component)]
struct Seeded;

#[derive(Component)]
struct Folder(PathBuf);

#[derive(Component)]
struct Directory {
    client: bool,
    cloud: bool,
}

#[derive(Component)]
struct Location(Entity);

#[derive(Component)]
struct Inspected;

#[derive(Component)]
struct Worktree {
    branch: Option<OsString>,
    cloud: bool,
}

#[derive(Component)]
struct Choice(Entity);

type Incoming<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static CliInvocation),
    (Without<Input>, Without<Checkout>, Without<CliResult>),
>;

fn request(requests: Incoming, mut commands: Commands) {
    for (entity, invocation) in &requests {
        if !invocation.command.starts_with("dx.")
            || invocation.is("dx.help")
            || invocation.is("dx.update")
        {
            continue;
        }
        let path = match invocation.value_os("root") {
            Some(path) => PathBuf::from(path),
            None => match std::env::current_dir() {
                Ok(path) => path,
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(CliResult(Err(error.to_string())));
                    continue;
                }
            },
        };
        commands.entity(entity).insert(Input {
            path,
            branch: invocation.value_os("branch").map(OsString::from),
        });
    }
}

type Seeding<'w, 's> =
    Query<'w, 's, (Entity, &'static Input), (Without<Seeded>, Without<CliResult>)>;

fn seed(requests: Seeding, mut commands: Commands) {
    for (entity, input) in &requests {
        let mut path = match input.path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
                continue;
            }
        };
        let mut order = 0;
        loop {
            commands.spawn((
                Folder(path.clone()),
                Directory {
                    client: path.join("crates/app/vmux_cli/Cargo.toml").is_file(),
                    cloud: path.join(".gitmodules").is_file() && path.join("Cargo.toml").is_file(),
                },
                Order(order),
                ChildOf(entity),
                Completed,
            ));
            if !path.pop() {
                break;
            }
            order += 1;
        }
        commands.entity(entity).insert(Seeded);
    }
}

type Unlocated<'w, 's> =
    Query<'w, 's, Entity, (With<Seeded>, Without<Location>, Without<CliResult>)>;

fn locate(
    requests: Unlocated,
    directories: Query<(Entity, &Directory, &Order, &ChildOf)>,
    mut commands: Commands,
) {
    for request in &requests {
        let mut nearest = None;
        for (entity, directory, order, parent) in &directories {
            if parent.parent() != request || !directory.client && !directory.cloud {
                continue;
            }
            if nearest.is_none_or(|(_, previous)| order.0 < previous) {
                nearest = Some((entity, order.0));
            }
        }
        match nearest {
            Some((entity, _)) => {
                commands.entity(request).insert(Location(entity));
            }
            None => {
                commands.entity(request).insert(CliResult(Err(
                    "Not inside a Vmux repository; use --root PATH".into(),
                )));
            }
        }
    }
}

type Inspecting<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Input, &'static Location),
    (Without<Inspected>, Without<CliResult>),
>;

fn inspect(
    requests: Inspecting,
    directories: Query<(&Folder, &Directory)>,
    mut commands: Commands,
) {
    for (entity, input, location) in &requests {
        let (folder, directory) = directories.get(location.0).unwrap();
        if input.branch.is_none() {
            commands.entity(entity).insert(Inspected);
            continue;
        }
        let roots = match Repository(folder.0.clone()).worktrees() {
            Ok(records) => records,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
                continue;
            }
        };
        let mut seen = BTreeSet::new();
        for record in roots {
            let path = record.path;
            commands.spawn((
                Folder(path.clone()),
                Worktree {
                    branch: record.branch,
                    cloud: directory.cloud,
                },
                ChildOf(entity),
                Completed,
            ));
            if !directory.cloud {
                continue;
            }
            let client = path.join("client");
            if !client.join(".git").exists() {
                continue;
            }
            match Repository(client).worktrees() {
                Ok(records) => {
                    for record in records {
                        if !record.path.join(".git").exists() {
                            continue;
                        }
                        let path = match record.path.canonicalize() {
                            Ok(path) => path,
                            Err(error) => {
                                commands
                                    .entity(entity)
                                    .insert(CliResult(Err(error.to_string())));
                                break;
                            }
                        };
                        if seen.insert(path.clone()) {
                            commands.spawn((
                                Folder(path),
                                Worktree {
                                    branch: record.branch,
                                    cloud: false,
                                },
                                ChildOf(entity),
                                Completed,
                            ));
                        }
                    }
                }
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(CliResult(Err(error.to_string())));
                    break;
                }
            }
        }
        commands.entity(entity).insert(Inspected);
    }
}

type Choosing<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Input, &'static Location),
    (With<Inspected>, Without<Choice>, Without<CliResult>),
>;

fn choose(
    requests: Choosing,
    worktrees: Query<(Entity, &Folder, &Worktree, &ChildOf)>,
    mut commands: Commands,
) {
    for (request, input, location) in &requests {
        let Some(name) = &input.branch else {
            commands.entity(request).insert(Choice(location.0));
            continue;
        };
        let mut best = None;
        let mut ambiguous = false;
        for (entity, folder, worktree, parent) in &worktrees {
            if parent.parent() != request {
                continue;
            }
            let rank = if worktree.branch.as_deref() == Some(name.as_os_str()) {
                if worktree.cloud { 0 } else { 1 }
            } else if folder.0.file_name() == Some(name.as_os_str()) {
                if worktree.cloud { 2 } else { 3 }
            } else {
                continue;
            };
            match best {
                None => {
                    best = Some((entity, rank));
                    ambiguous = false;
                }
                Some((_, previous)) if rank < previous => {
                    best = Some((entity, rank));
                    ambiguous = false;
                }
                Some((_, previous)) if rank == previous => ambiguous = true,
                _ => {}
            }
        }
        if ambiguous {
            let error = if best.is_some_and(|(_, rank)| rank >= 2) {
                "Worktree alias is ambiguous; use the full branch name"
            } else {
                "Branch matches multiple worktrees"
            };
            commands
                .entity(request)
                .insert(CliResult(Err(error.into())));
        } else if let Some((entity, _)) = best {
            commands.entity(request).insert(Choice(entity));
        } else {
            commands.entity(request).insert(CliResult(Err(
                "Branch has no worktree; create its worktree explicitly first".into(),
            )));
        }
    }
}

type Finishing<'w, 's> =
    Query<'w, 's, (Entity, &'static Choice), (Without<Checkout>, Without<CliResult>)>;

fn finish(requests: Finishing, folders: Query<&Folder>, mut commands: Commands) {
    for (entity, choice) in &requests {
        let mut root = folders.get(choice.0).unwrap().0.clone();
        if !root.join("crates/app/vmux_cli/Cargo.toml").is_file() {
            root.push("client");
        }
        if !root.join("crates/app/vmux_cli/Cargo.toml").is_file() {
            commands.entity(entity).insert(CliResult(Err(
                "Selected checkout has no initialized Vmux client".into(),
            )));
            continue;
        }
        let root = match root.canonicalize() {
            Ok(root) => root,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
                continue;
            }
        };
        let mut checkout = Checkout {
            target: root.join("target"),
            root,
            build_target: None,
        };
        match checkout.configure(
            std::env::var_os("CARGO_TARGET_DIR").as_deref(),
            std::env::var_os("CARGO_BUILD_TARGET").as_deref(),
        ) {
            Ok(()) => {
                commands.entity(entity).insert(checkout);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
            }
        }
    }
}
