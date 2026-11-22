use std::path::Path;

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_profile::mcp_credentials::{McpCredentialAccess, McpCredentialStorage};

pub use crate::{ProfileId, ProfileLabel};
pub use bootstrap::ProfileBootstrapSpec;
pub use migration::{MigrationRefusal, ProfileMigrationComplete, ProfileMigrationSpec};
pub use vmux_profile::*;

mod bootstrap;
mod migration;

pub struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((migration::MigrationPlugin, bootstrap::BootstrapPlugin));
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
            Entity,
            &'static ActiveProfile,
            &'static Name,
            &'static ProfileDirectories,
        ),
    >,
}

impl CurrentProfile<'_, '_> {
    pub fn entity(&self) -> Option<Entity> {
        self.active.single().ok().map(|(entity, _, _, _)| entity)
    }

    pub fn profile(&self) -> Option<&Profile> {
        self.active
            .single()
            .ok()
            .map(|(_, profile, _, _)| &profile.0)
    }

    pub fn label(&self) -> Option<&str> {
        self.active
            .single()
            .ok()
            .map(|(_, _, name, _)| name.as_str())
    }

    pub fn paths(&self) -> Option<&ProfilePaths> {
        self.active.single().ok().map(|(_, _, _, paths)| &paths.0)
    }
}

#[derive(Component, Clone, Debug)]
pub struct ProjectsRoot(pub ProjectsDirectory);

#[derive(Component, Clone, Debug)]
pub(super) struct ProjectsRootFailure(pub(super) String);

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
