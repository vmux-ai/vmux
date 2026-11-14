use bevy_app::{App, Plugin};
use bevy_ecs::prelude::{Commands, Query};
use std::path::Path;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

pub use crate::{ProfileId, ProfileLabel};
use vmux_profile::mcp_credentials::{McpCredentialAccess, McpCredentialStorage};
pub use vmux_profile::*;

pub struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, (queue_migration, migrate, spawn).chain());
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

fn spawn(mut commands: bevy::prelude::Commands) {
    let profile = Profile::current();
    let paths = ProfilePaths::current();
    let label = ProfileStore::current().active_label();
    let credentials = McpCredentials {
        access: McpCredentialAccess::default(),
        storage: McpCredentialStorage::at(profile.clone(), paths.clone()),
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
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ActiveProfile(pub Profile);

#[derive(Component, Clone, Debug)]
pub struct ProfileDirectories(pub ProfilePaths);

#[derive(Component, Clone, Debug)]
pub struct McpCredentials {
    pub access: McpCredentialAccess,
    pub storage: McpCredentialStorage,
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
