use std::io;
use std::path::{Path, PathBuf};

use bevy_app::{App, Plugin, PreStartup};
use bevy_ecs::prelude::{Commands, Query, *};
use bevy_ecs::schedule::ScheduleLabel;
use vmux_profile::{
    DirectoryChild, EntryObservation, NativeRelocation, Product, ProfilePaths, SessionEnvironment,
};

pub(super) struct MigrationPlugin;

impl Plugin for MigrationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreStartup,
            (seed_default_migration, drive_migration)
                .chain()
                .in_set(MigrationBarrierSet),
        )
        .add_systems(
            ProfileMigrationStep,
            (
                admit_core_roots,
                expand_legacy_profiles,
                select_next_relocation,
                observe_relocation,
                execute_relocation,
                settle_parent_relocations,
                admit_space_pruning,
                discover_space_prune,
                execute_prune,
                settle_migration,
            )
                .chain(),
        );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct MigrationBarrierSet;

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct ProfileMigrationStep;

#[derive(Component, Clone, Debug)]
pub struct ProfileMigrationSpec {
    pub home: PathBuf,
    pub data: PathBuf,
    pub managed_data: PathBuf,
    pub disabled: bool,
}

impl ProfileMigrationSpec {
    pub fn current() -> Self {
        let paths = ProfilePaths::current();
        Self {
            home: Self::native_home(),
            data: paths.shared_data(),
            managed_data: paths.application_data(),
            disabled: SessionEnvironment::is_test() || std::env::var_os("VMUX_DATA_DIR").is_some(),
        }
    }

    fn native_home() -> PathBuf {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    fn config(&self) -> PathBuf {
        self.home.join(Product::config_name())
    }

    fn spaces(&self) -> PathBuf {
        self.data.join("spaces")
    }

    fn recording(&self, profile: &str) -> PathBuf {
        self.data.join("profiles").join(profile).join("recording")
    }
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ProfileMigrationComplete;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct MigrationRefusal {
    pub path: PathBuf,
    pub operation: &'static str,
    pub message: String,
}

#[derive(Component)]
struct MigrationPending;

#[derive(Component)]
struct MigratingCoreRoots;

#[derive(Component)]
struct MigratingLegacyProfiles;

#[derive(Component)]
struct PruningLegacyProfiles;

#[derive(Component)]
struct PruningSpaces;

#[derive(Component)]
struct MigrationOwner(Entity);

#[derive(Component)]
struct RelocationTask {
    source: PathBuf,
    target: PathBuf,
}

impl RelocationTask {
    fn observe_source(&self) -> io::Result<EntryObservation> {
        NativeRelocation::observe(&self.source)
    }

    fn observe_target(&self) -> io::Result<EntryObservation> {
        NativeRelocation::observe(&self.target)
    }

    fn read_source_children(&self) -> io::Result<Vec<DirectoryChild>> {
        NativeRelocation::read_directory_entries(&self.source)
    }

    fn relocate(&self) -> io::Result<()> {
        NativeRelocation::ensure_parent(&self.target)?;
        NativeRelocation::relocate_no_replace(&self.source, &self.target)
    }

    fn prune_source(&self) -> io::Result<()> {
        NativeRelocation::remove_empty_directory(&self.source)
    }
}

#[derive(Component)]
struct RelocationParent(Entity);

#[derive(Component, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RelocationOrder(Vec<usize>);

#[derive(Component)]
struct RelocationPending;

#[derive(Component)]
struct RelocationSelected;

#[derive(Component)]
struct RelocationObserved {
    source: EntryObservation,
    target: EntryObservation,
}

#[derive(Component)]
struct RelocationMergingChildren;

#[derive(Component)]
struct RelocationSettled;

#[derive(Component)]
struct PruneNode(PathBuf);

impl PruneNode {
    fn observe(&self) -> io::Result<EntryObservation> {
        NativeRelocation::observe(&self.0)
    }

    fn read_children(&self) -> io::Result<Vec<DirectoryChild>> {
        NativeRelocation::read_directory_entries(&self.0)
    }

    fn remove_empty(&self) -> io::Result<()> {
        NativeRelocation::remove_empty_directory(&self.0)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

#[derive(Component)]
struct PruneParent(Entity);

#[derive(Component)]
struct PruneScanning;

#[derive(Component)]
struct PruneWaitingChildren;

#[derive(Component)]
struct PruneSettled;

fn seed_default_migration(
    existing: Query<Entity, With<ProfileMigrationSpec>>,
    mut commands: Commands,
) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        Name::new("Profile migration"),
        ProfileMigrationSpec::current(),
        MigrationPending,
    ));
}

fn drive_migration(world: &mut World) {
    let mut query = world.query_filtered::<Entity, (
        With<ProfileMigrationSpec>,
        Without<MigrationPending>,
        Without<ProfileMigrationComplete>,
    )>();
    let uninitialized: Vec<Entity> = query.iter(world).collect();
    for entity in uninitialized {
        world.entity_mut(entity).insert(MigrationPending);
    }
    for _ in 0..100_000 {
        let mut remaining = world.query_filtered::<Entity, (
            With<ProfileMigrationSpec>,
            Without<ProfileMigrationComplete>,
        )>();
        if remaining.iter(world).next().is_none() {
            break;
        }
        world.run_schedule(ProfileMigrationStep);
    }
}

fn admit_core_roots(
    roots: Query<(Entity, &ProfileMigrationSpec), With<MigrationPending>>,
    mut commands: Commands,
) {
    for (root, spec) in &roots {
        commands.entity(root).remove::<MigrationPending>();
        if spec.disabled {
            commands.entity(root).insert(ProfileMigrationComplete);
            continue;
        }
        let config = spec.config();
        let pairs = [
            (config.join("profiles/personal/spaces"), spec.spaces()),
            (config.join("spaces"), spec.spaces()),
            (config.join("recording"), spec.recording("personal")),
            (config.join("agents"), spec.managed_data.join("agents")),
            (
                config.join("extensions"),
                spec.managed_data.join("extensions"),
            ),
            (config.join("lsp"), spec.managed_data.join("lsp")),
        ];
        for (index, (source, target)) in pairs.into_iter().enumerate() {
            commands.spawn((
                MigrationOwner(root),
                RelocationTask { source, target },
                RelocationOrder(vec![index]),
                RelocationPending,
            ));
        }
        commands.entity(root).insert(MigratingCoreRoots);
    }
}

fn expand_legacy_profiles(
    roots: Query<(Entity, &ProfileMigrationSpec), With<MigratingCoreRoots>>,
    active: Query<&MigrationOwner, (With<RelocationTask>, Without<RelocationSettled>)>,
    mut commands: Commands,
) {
    for (root, spec) in &roots {
        if active.iter().any(|owner| owner.0 == root) {
            continue;
        }
        commands
            .entity(root)
            .remove::<MigratingCoreRoots>()
            .insert(MigratingLegacyProfiles);
        let profiles_dir = spec.config().join("profiles");
        let observed = match NativeRelocation::observe(&profiles_dir) {
            Ok(observation) => observation,
            Err(error) => {
                commands.spawn((
                    MigrationOwner(root),
                    MigrationRefusal {
                        path: profiles_dir,
                        operation: "observe_legacy_profiles",
                        message: error.to_string(),
                    },
                ));
                continue;
            }
        };
        if !observed.is_directory() {
            continue;
        }
        let children = match NativeRelocation::read_directory_entries(&profiles_dir) {
            Ok(children) => children,
            Err(error) => {
                commands.spawn((
                    MigrationOwner(root),
                    MigrationRefusal {
                        path: profiles_dir,
                        operation: "read_legacy_profiles",
                        message: error.to_string(),
                    },
                ));
                continue;
            }
        };
        for (index, child) in children.into_iter().enumerate() {
            if !child.is_directory() {
                continue;
            }
            commands.spawn((
                MigrationOwner(root),
                RelocationTask {
                    source: profiles_dir.join(&child.name),
                    target: spec.data.join("profiles").join(&child.name),
                },
                RelocationOrder(vec![100, index]),
                RelocationPending,
            ));
        }
    }
}

#[allow(clippy::type_complexity)]
fn select_next_relocation(
    selected: Query<Entity, With<RelocationSelected>>,
    pending: Query<(Entity, &RelocationOrder), (With<RelocationTask>, With<RelocationPending>)>,
    mut commands: Commands,
) {
    if !selected.is_empty() {
        return;
    }
    let mut next: Option<(Entity, &RelocationOrder)> = None;
    for (entity, order) in &pending {
        if next.is_none_or(|(_, best)| order < best) {
            next = Some((entity, order));
        }
    }
    if let Some((entity, _)) = next {
        commands
            .entity(entity)
            .remove::<RelocationPending>()
            .insert(RelocationSelected);
    }
}

#[allow(clippy::type_complexity)]
fn observe_relocation(
    tasks: Query<
        (Entity, &MigrationOwner, &RelocationTask),
        (With<RelocationSelected>, Without<RelocationObserved>),
    >,
    mut commands: Commands,
) {
    for (entity, owner, task) in &tasks {
        let source = match task.observe_source() {
            Ok(observation) => observation,
            Err(error) => {
                commands.spawn((
                    MigrationOwner(owner.0),
                    MigrationRefusal {
                        path: task.source.clone(),
                        operation: "observe_source",
                        message: error.to_string(),
                    },
                ));
                commands
                    .entity(entity)
                    .remove::<RelocationSelected>()
                    .insert(RelocationSettled);
                continue;
            }
        };
        if source.is_missing() {
            commands
                .entity(entity)
                .remove::<RelocationSelected>()
                .insert(RelocationSettled);
            continue;
        }
        let target = match task.observe_target() {
            Ok(observation) => observation,
            Err(error) => {
                commands.spawn((
                    MigrationOwner(owner.0),
                    MigrationRefusal {
                        path: task.target.clone(),
                        operation: "observe_target",
                        message: error.to_string(),
                    },
                ));
                commands
                    .entity(entity)
                    .remove::<RelocationSelected>()
                    .insert(RelocationSettled);
                continue;
            }
        };
        commands
            .entity(entity)
            .insert(RelocationObserved { source, target });
    }
}

fn execute_relocation(
    tasks: Query<
        (
            Entity,
            &MigrationOwner,
            &RelocationTask,
            &RelocationOrder,
            &RelocationObserved,
        ),
        With<RelocationSelected>,
    >,
    mut commands: Commands,
) {
    for (entity, owner, task, order, observed) in &tasks {
        commands
            .entity(entity)
            .remove::<(RelocationSelected, RelocationObserved)>();
        if observed.target.is_missing() {
            if let Err(error) = task.relocate() {
                commands.spawn((
                    MigrationOwner(owner.0),
                    MigrationRefusal {
                        path: task.source.clone(),
                        operation: "relocate_no_replace",
                        message: error.to_string(),
                    },
                ));
            }
            commands.entity(entity).insert(RelocationSettled);
            continue;
        }
        if observed.source.is_directory() && observed.target.is_directory() {
            let children = match task.read_source_children() {
                Ok(children) => children,
                Err(error) => {
                    commands.spawn((
                        MigrationOwner(owner.0),
                        MigrationRefusal {
                            path: task.source.clone(),
                            operation: "read_source_children",
                            message: error.to_string(),
                        },
                    ));
                    commands.entity(entity).insert(RelocationSettled);
                    continue;
                }
            };
            for (index, child) in children.into_iter().enumerate() {
                let mut child_order = order.0.clone();
                child_order.push(index);
                commands.spawn((
                    MigrationOwner(owner.0),
                    RelocationParent(entity),
                    RelocationTask {
                        source: task.source.join(&child.name),
                        target: task.target.join(&child.name),
                    },
                    RelocationOrder(child_order),
                    RelocationPending,
                ));
            }
            commands.entity(entity).insert(RelocationMergingChildren);
            continue;
        }
        commands.spawn((
            MigrationOwner(owner.0),
            MigrationRefusal {
                path: task.target.clone(),
                operation: "preserve_occupied_target",
                message: "target already exists and cannot be merged".to_string(),
            },
        ));
        commands.entity(entity).insert(RelocationSettled);
    }
}

fn settle_parent_relocations(
    parents: Query<(Entity, &MigrationOwner, &RelocationTask), With<RelocationMergingChildren>>,
    unsettled_children: Query<
        &RelocationParent,
        (With<RelocationTask>, Without<RelocationSettled>),
    >,
    mut commands: Commands,
) {
    for (parent, owner, task) in &parents {
        if unsettled_children.iter().any(|link| link.0 == parent) {
            continue;
        }
        if let Err(error) = task.prune_source() {
            commands.spawn((
                MigrationOwner(owner.0),
                MigrationRefusal {
                    path: task.source.clone(),
                    operation: "prune_merged_source",
                    message: error.to_string(),
                },
            ));
        }
        commands
            .entity(parent)
            .remove::<RelocationMergingChildren>()
            .insert(RelocationSettled);
    }
}

fn admit_space_pruning(
    legacy_roots: Query<(Entity, &ProfileMigrationSpec), With<MigratingLegacyProfiles>>,
    pruning_legacy: Query<(Entity, &ProfileMigrationSpec), With<PruningLegacyProfiles>>,
    active_relocations: Query<&MigrationOwner, (With<RelocationTask>, Without<RelocationSettled>)>,
    active_prunes: Query<&MigrationOwner, (With<PruneNode>, Without<PruneSettled>)>,
    mut commands: Commands,
) {
    for (root, spec) in &legacy_roots {
        if active_relocations.iter().any(|owner| owner.0 == root) {
            continue;
        }
        commands
            .entity(root)
            .remove::<MigratingLegacyProfiles>()
            .insert(PruningLegacyProfiles);
        let legacy_profiles = spec.config().join("profiles");
        if NativeRelocation::observe(&legacy_profiles).is_ok_and(|obs| obs.is_directory()) {
            commands.spawn((
                MigrationOwner(root),
                PruneNode(legacy_profiles),
                PruneWaitingChildren,
            ));
        }
    }
    for (root, spec) in &pruning_legacy {
        if active_prunes.iter().any(|owner| owner.0 == root) {
            continue;
        }
        commands
            .entity(root)
            .remove::<PruningLegacyProfiles>()
            .insert(PruningSpaces);
        let spaces = spec.spaces();
        if NativeRelocation::observe(&spaces).is_ok_and(|obs| obs.is_directory()) {
            commands.spawn((MigrationOwner(root), PruneNode(spaces), PruneScanning));
        }
    }
}

fn discover_space_prune(
    nodes: Query<(Entity, &MigrationOwner, &PruneNode), With<PruneScanning>>,
    mut commands: Commands,
) {
    for (entity, owner, node) in &nodes {
        commands.entity(entity).remove::<PruneScanning>();
        let observed = match node.observe() {
            Ok(observation) => observation,
            Err(error) => {
                commands.spawn((
                    MigrationOwner(owner.0),
                    MigrationRefusal {
                        path: node.path().to_path_buf(),
                        operation: "observe_prune_directory",
                        message: error.to_string(),
                    },
                ));
                commands.entity(entity).insert(PruneSettled);
                continue;
            }
        };
        if !observed.is_directory() {
            commands.entity(entity).insert(PruneSettled);
            continue;
        }
        let children = match node.read_children() {
            Ok(children) => children,
            Err(error) => {
                commands.spawn((
                    MigrationOwner(owner.0),
                    MigrationRefusal {
                        path: node.path().to_path_buf(),
                        operation: "read_prune_directory",
                        message: error.to_string(),
                    },
                ));
                commands.entity(entity).insert(PruneSettled);
                continue;
            }
        };
        for child in children {
            if !child.is_directory() {
                continue;
            }
            commands.spawn((
                MigrationOwner(owner.0),
                PruneParent(entity),
                PruneNode(node.path().join(child.name)),
                PruneScanning,
            ));
        }
        commands.entity(entity).insert(PruneWaitingChildren);
    }
}

fn execute_prune(
    nodes: Query<(Entity, &MigrationOwner, &PruneNode), With<PruneWaitingChildren>>,
    unsettled_children: Query<&PruneParent, (With<PruneNode>, Without<PruneSettled>)>,
    mut commands: Commands,
) {
    for (entity, owner, node) in &nodes {
        if unsettled_children.iter().any(|link| link.0 == entity) {
            continue;
        }
        commands
            .entity(entity)
            .remove::<PruneWaitingChildren>()
            .insert(PruneSettled);
        let observed = match node.observe() {
            Ok(observation) => observation,
            Err(error) => {
                commands.spawn((
                    MigrationOwner(owner.0),
                    MigrationRefusal {
                        path: node.path().to_path_buf(),
                        operation: "reobserve_prune_directory",
                        message: error.to_string(),
                    },
                ));
                continue;
            }
        };
        if !observed.is_directory() {
            continue;
        }
        let is_empty = match node.read_children() {
            Ok(children) => children.is_empty(),
            Err(error) => {
                commands.spawn((
                    MigrationOwner(owner.0),
                    MigrationRefusal {
                        path: node.path().to_path_buf(),
                        operation: "check_prune_empty",
                        message: error.to_string(),
                    },
                ));
                continue;
            }
        };
        if is_empty && let Err(error) = node.remove_empty() {
            commands.spawn((
                MigrationOwner(owner.0),
                MigrationRefusal {
                    path: node.path().to_path_buf(),
                    operation: "remove_empty_directory",
                    message: error.to_string(),
                },
            ));
        }
    }
}

fn settle_migration(
    roots: Query<Entity, With<PruningSpaces>>,
    active_prunes: Query<&MigrationOwner, (With<PruneNode>, Without<PruneSettled>)>,
    mut commands: Commands,
) {
    for root in &roots {
        if active_prunes.iter().any(|owner| owner.0 == root) {
            continue;
        }
        commands
            .entity(root)
            .remove::<PruningSpaces>()
            .insert(ProfileMigrationComplete);
    }
}
