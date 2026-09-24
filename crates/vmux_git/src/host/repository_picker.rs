use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::event::GitRepositoryPickerRequest;
use crate::state::{GitRepositoryPicked, GitUiState};

type GitUiStateUpdates = vmux_core::host::UiState<GitUiState>;

pub(super) struct RepositoryPickerPlugin;

impl Plugin for RepositoryPickerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(GitRepositoryPickerRequest,)>::default())
            .add_observer(on_repository_picker_request)
            .add_systems(Update, poll_repository_pickers);
    }
}

#[derive(Component)]
struct PendingGitRepositoryPicker {
    webview: Entity,
    task: Task<Option<PathBuf>>,
}

struct GitRepositoryPicker;

impl GitRepositoryPicker {
    fn initial_directory(path: &Path) -> PathBuf {
        let mut current = path.to_path_buf();
        while !current.is_dir() && current.pop() {}
        if current.is_dir() {
            return current;
        }
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_dir())
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    fn task(
        path: PathBuf,
        proxy: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) -> Task<Option<PathBuf>> {
        let initial = Self::initial_directory(&path);
        IoTaskPool::get().spawn(async move {
            let selected = rfd::AsyncFileDialog::new()
                .set_title("Choose Git repository")
                .set_directory(initial)
                .pick_folder()
                .await
                .map(|folder| folder.path().to_path_buf());
            if let Some(proxy) = proxy {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }
            selected
        })
    }
}

fn on_repository_picker_request(
    trigger: On<UiInput<GitRepositoryPickerRequest>>,
    pending: Query<&PendingGitRepositoryPicker>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if pending.iter().any(|picker| picker.webview == webview) {
        return;
    }
    let path = PathBuf::from(&trigger.event().payload.path);
    let proxy = proxy.as_deref().map(|proxy| (**proxy).clone());
    commands.spawn(PendingGitRepositoryPicker {
        webview,
        task: GitRepositoryPicker::task(path, proxy),
    });
}

fn poll_repository_pickers(
    mut pending: Query<(Entity, &mut PendingGitRepositoryPicker)>,
    mut commands: Commands,
) {
    for (entity, mut picker) in &mut pending {
        let Some(selected) = future::block_on(future::poll_once(&mut picker.task)) else {
            continue;
        };
        if let Some(path) = selected {
            GitUiStateUpdates::write(
                &mut commands,
                picker.webview,
                &GitRepositoryPicked {
                    path: path.to_string_lossy().into_owned(),
                },
            );
        }
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_picker_starts_at_the_nearest_existing_directory() {
        let root = tempfile::tempdir().unwrap();
        let existing = root.path().join("projects");
        std::fs::create_dir(&existing).unwrap();
        let missing = existing.join("github.com/vmux-ai/vmux");

        assert_eq!(GitRepositoryPicker::initial_directory(&missing), existing);
    }
}
