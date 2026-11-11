use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::task::Wake;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::winit::EventLoopProxyWrapper;
use vmux_ecs::Cwd;
use vmux_ecs::profile::ProjectsRoot;
use vmux_project::filesystem::{FileWake, FilesystemPlugin};
use vmux_project::{ProjectPlugin, Root};

use crate::project::ProjectRoute;

pub(super) use vmux_project::{
    Mutation, ProjectSaved, Snapshot, StoreSet, WriteCompleted, WriteRequest,
};

pub(super) struct ProjectStorePlugin;

impl Plugin for ProjectStorePlugin {
    fn build(&self, app: &mut App) {
        assert!(
            app.is_plugin_added::<ProjectPlugin>(),
            "Editor requires ProjectPlugin in the application composition"
        );
        app.add_plugins(FilesystemPlugin)
            .add_systems(Update, bind.before(StoreSet::Load));
    }
}

fn bind(
    profiles: Query<(Entity, &ProjectsRoot, Option<&Root>)>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    wake: Option<Res<FileWake>>,
    mut commands: Commands,
) {
    for (entity, directory, current) in &profiles {
        if current.is_none_or(|root| root.0 != directory.0.path()) {
            commands
                .entity(entity)
                .insert(Root(directory.0.path().to_path_buf()));
        }
    }
    if wake.is_none()
        && let Some(proxy) = proxy
    {
        let wake = EventLoopWake(vmux_ecs::wake::Wake::beside(Some(&proxy)));
        commands.insert_resource(FileWake(Arc::new(wake).into()));
    }
}

struct EventLoopWake(vmux_ecs::wake::Wake);

impl Wake for EventLoopWake {
    fn wake(self: Arc<Self>) {
        self.0.now();
    }
}

pub(super) struct ErrorMessage(pub(super) vmux_project::Error);

impl ErrorMessage {
    pub(super) fn localized(self) -> String {
        let key = match self.0 {
            vmux_project::Error::Conflict => "project-edit-conflict",
            vmux_project::Error::Duplicate => "project-already-exists",
            vmux_project::Error::InvalidName => "project-name-invalid",
            vmux_project::Error::NotFound => "error-page-not-found",
            vmux_project::Error::Unavailable(error) => return error,
        };
        vmux_ui::i18n::translate(key)
    }
}

#[derive(SystemParam)]
pub(super) struct Registrations<'w, 's> {
    inner: vmux_project::Registrations<'w, 's>,
}

impl Registrations<'_, '_> {
    pub(super) fn ready(&self, root: &Path) -> Result<bool, String> {
        self.inner
            .ready(vmux_path::PathIdentity::resolve(root).as_path())
            .map_err(|error| ErrorMessage(error).localized())
    }

    pub(super) fn snapshot(&self, root: &Path) -> Snapshot {
        self.inner
            .snapshot(vmux_path::PathIdentity::resolve(root).as_path())
    }

    pub(super) fn get(
        &self,
        root: &Path,
        url: &str,
    ) -> Option<(&Name, &Cwd, &vmux_project::Repository, &vmux_ecs::Revision)> {
        self.inner
            .get(vmux_path::PathIdentity::resolve(root).as_path(), url)
    }

    pub(super) fn resolve(&self, url: &str, root: &Path) -> Option<Result<PathBuf, String>> {
        let root = vmux_path::PathIdentity::resolve(root).into_path_buf();
        let root = root.as_path();
        if !ProjectRoute::is_page(url) {
            return None;
        }
        if let Err(error) = ProjectRoute::route_path(url, root)? {
            return Some(Err(error));
        }
        let directory = match self.inner.directory(root, url) {
            Ok(directory) => directory,
            Err(error) => return Some(Err(ErrorMessage(error).localized())),
        };
        if let Some(directory) = directory {
            if !directory.is_dir() {
                return Some(Err(ErrorMessage(vmux_project::Error::NotFound).localized()));
            }
            return Some(Ok(directory));
        }
        ProjectRoute::resolve(url, root)
    }

    pub(super) fn url_for_directory(&self, root: &Path, directory: &Path) -> Option<String> {
        self.inner.url_for_directory(
            vmux_path::PathIdentity::resolve(root).as_path(),
            vmux_path::PathIdentity::resolve(directory).as_path(),
        )
    }
}
