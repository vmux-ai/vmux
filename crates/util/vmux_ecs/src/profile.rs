use std::path::Path;

use crate::Active;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};

pub use vmux_profile::*;

pub struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, (queue_migration, migrate, spawn).chain())
            .add_systems(Update, finish);
    }
}

fn queue_migration(mut commands: Commands) {
    commands.spawn((
        Name::new("Profile migration"),
        ProfileMigrationJob(ProfileMigration::current()),
    ));
}

fn migrate(jobs: Query<(Entity, &ProfileMigrationJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        job.0.run();
        commands
            .entity(entity)
            .remove::<ProfileMigrationJob>()
            .insert(ProfileMigrationComplete);
    }
}

fn spawn(mut commands: Commands) {
    let profile = Profile::current();
    let paths = ProfilePaths::current();
    let catalog = ProfileCatalog(ProfileStore::current());
    let label = catalog.0.active_label();
    let credentials = McpCredentials {
        access: mcp_credentials::McpCredentialAccess::default(),
        storage: mcp_credentials::McpCredentialStorage::at(profile.clone(), paths.clone()),
    };
    let mut active = commands.spawn((
        Name::new(label),
        ActiveProfile(profile.clone()),
        ProfileDirectories(paths),
        credentials,
    ));
    match ProjectsDirectory::ensure() {
        Ok(directory) => {
            active.insert(ProjectsRoot(directory));
        }
        Err(error) => {
            active.insert(ProjectsRootFailure(error));
        }
    }

    let loader = catalog.0.clone();
    commands.spawn((
        Name::new("Profiles"),
        catalog,
        ProfileListTask(IoTaskPool::get().spawn(async move { loader.profiles() })),
    ));
}

fn finish(mut tasks: Query<(Entity, &mut ProfileListTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        let Some(records) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<ProfileListTask>();
        for record in records {
            let mut profile = commands.spawn((
                ProfileLabel,
                ProfileId(record.profile.into_id()),
                Name::new(record.name),
            ));
            if record.active {
                profile.insert(Active);
            }
        }
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ActiveProfile(pub Profile);

#[derive(Component, Clone, Debug)]
pub struct ProfileDirectories(pub ProfilePaths);

#[derive(Component, Clone, Debug)]
pub struct ProfileCatalog(pub ProfileStore);

#[derive(Component, Clone, Debug)]
pub struct McpCredentials {
    pub access: mcp_credentials::McpCredentialAccess,
    pub storage: mcp_credentials::McpCredentialStorage,
}

#[derive(SystemParam)]
pub struct CurrentProfile<'w, 's> {
    active: Query<
        'w,
        's,
        (
            &'static ActiveProfile,
            &'static Name,
            &'static ProfileDirectories,
        ),
    >,
}

impl CurrentProfile<'_, '_> {
    pub fn profile(&self) -> Option<&Profile> {
        self.active.single().ok().map(|(profile, _, _)| &profile.0)
    }

    pub fn label(&self) -> Option<&str> {
        self.active.single().ok().map(|(_, name, _)| name.as_str())
    }

    pub fn paths(&self) -> Option<&ProfilePaths> {
        self.active.single().ok().map(|(_, _, paths)| &paths.0)
    }
}

#[derive(Component)]
struct ProfileMigrationJob(ProfileMigration);

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ProfileMigrationComplete;

#[derive(Component, Clone, Debug)]
pub struct ProjectsRoot(pub ProjectsDirectory);

#[derive(Component, Clone, Debug)]
struct ProjectsRootFailure(String);

#[derive(SystemParam)]
pub struct Projects<'w, 's> {
    roots: Query<'w, 's, &'static ProjectsRoot>,
    failures: Query<'w, 's, &'static ProjectsRootFailure>,
}

impl Projects<'_, '_> {
    pub fn path(&self) -> Result<&Path, String> {
        if let Ok(root) = self.roots.single() {
            return Ok(root.0.path());
        }
        if let Ok(failure) = self.failures.single() {
            return Err(failure.0.clone());
        }
        Err("projects directory is not initialized".to_string())
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.path().is_ok_and(|root| path.starts_with(root))
    }
}

#[derive(Component)]
struct ProfileListTask(Task<Vec<ProfileRecord>>);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ProfileId(pub String);

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ProfileLabel;
