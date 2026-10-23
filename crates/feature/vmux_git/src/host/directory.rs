use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::event::FileDirEntry;
use vmux_ecs::event::space::ProjectActivateRequest;

use crate::event::{
    GitDirectoryActivateRequest, GitDirectoryAscendRequest, GitDirectoryDescendRequest,
    GitDirectoryNextRequest, GitDirectoryOpenRequest, GitDirectoryParentRequest,
    GitDirectoryPreviousRequest, GitDirectorySelectRequest, GitDirectoryToggleHiddenRequest,
    GitRepositoryRequest,
};
use crate::state::GitDirectoryState;

use super::runtime::{GitRuntime, GitTask};

pub(super) struct DirectoryPlugin;

impl Plugin for DirectoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GitRuntime>()
            .add_plugins(UiEventPlugin::<(
                GitDirectoryOpenRequest,
                GitDirectorySelectRequest,
                GitDirectoryNextRequest,
                GitDirectoryPreviousRequest,
                GitDirectoryAscendRequest,
                GitDirectoryDescendRequest,
                GitDirectoryActivateRequest,
                GitDirectoryParentRequest,
                GitDirectoryToggleHiddenRequest,
            )>::default())
            .add_observer(open_request)
            .add_observer(select_request)
            .add_observer(next_request)
            .add_observer(previous_request)
            .add_observer(ascend_request)
            .add_observer(descend_request)
            .add_observer(activate_request)
            .add_observer(parent_request)
            .add_observer(toggle_hidden)
            .add_observer(load)
            .add_systems(
                Update,
                (
                    cleanup_inactive,
                    bevy::ecs::schedule::ApplyDeferred,
                    start_loads,
                    poll_loads,
                    start_previews,
                    poll_previews,
                )
                    .chain(),
            );
    }
}

type InactiveDirectoryTaskFilter = (
    Or<(
        With<PendingDirectoryLoad>,
        With<DirectoryLoadTask>,
        With<PendingDirectoryPreview>,
        With<DirectoryPreviewTask>,
    )>,
    Without<super::state::GitState>,
);

fn cleanup_inactive(inactive: Query<Entity, InactiveDirectoryTaskFilter>, mut commands: Commands) {
    for entity in &inactive {
        commands.entity(entity).remove::<(
            PendingDirectoryLoad,
            DirectoryLoadTask,
            PendingDirectoryPreview,
            DirectoryPreviewTask,
        )>();
    }
}

#[derive(Component)]
pub(super) struct GitDirectoryNavigation {
    current: Option<DirectoryListing>,
    preview: Option<DirectoryListing>,
    selected: usize,
    show_hidden: bool,
    load_revision: u64,
    preview_revision: u64,
}

impl Default for GitDirectoryNavigation {
    fn default() -> Self {
        Self {
            current: None,
            preview: None,
            selected: 0,
            show_hidden: true,
            load_revision: 0,
            preview_revision: 0,
        }
    }
}

impl GitDirectoryNavigation {
    pub(super) fn state(&self) -> GitDirectoryState {
        let Some(current) = self.current.as_ref() else {
            return GitDirectoryState::default();
        };
        GitDirectoryState {
            path: current.path.to_string_lossy().into_owned(),
            parent_entries: current.visible_parent_entries(self.show_hidden),
            entries: current.visible_entries(self.show_hidden),
            children: self
                .preview
                .as_ref()
                .map(|preview| preview.visible_entries(self.show_hidden)),
            selected: u32::try_from(self.selected).unwrap_or(u32::MAX),
            show_hidden: self.show_hidden,
        }
    }

    fn selected_entry(&self) -> Option<&FileDirEntry> {
        self.current
            .as_ref()?
            .visible_entry(self.show_hidden, self.selected)
    }

    fn select(&mut self, index: usize) -> Option<PendingDirectoryPreview> {
        let entries = self
            .current
            .as_ref()
            .map(|current| current.visible_entries(self.show_hidden))
            .unwrap_or_default();
        self.selected = index.min(entries.len().saturating_sub(1));
        self.preview = None;
        self.preview_revision = self.preview_revision.wrapping_add(1).max(1);
        entries
            .get(self.selected)
            .filter(|entry| entry.is_dir)
            .map(|entry| PendingDirectoryPreview {
                revision: self.preview_revision,
                path: PathBuf::from(&entry.path),
            })
    }

    fn next_load_revision(&mut self) -> u64 {
        self.load_revision = self.load_revision.wrapping_add(1).max(1);
        self.load_revision
    }
}

#[derive(Component, Clone)]
struct PendingDirectoryLoad {
    revision: u64,
    path: PathBuf,
    came_from: String,
    show_hidden: bool,
}

#[derive(Component)]
struct DirectoryLoadTask {
    revision: u64,
    task: GitTask<DirectoryLoadOutput>,
}

struct DirectoryLoadOutput {
    listing: DirectoryListing,
    selected: usize,
    preview: Option<DirectoryListing>,
}

impl DirectoryLoadOutput {
    fn read(path: &Path, came_from: &str, show_hidden: bool) -> Self {
        let listing = DirectoryListing::read(path);
        let entries = listing.visible_entries(show_hidden);
        let selected = entries
            .iter()
            .position(|entry| entry.path == came_from)
            .unwrap_or(0);
        let preview = listing
            .visible_entry(show_hidden, selected)
            .filter(|entry| entry.is_dir)
            .map(|entry| DirectoryListing::read(Path::new(&entry.path)));
        Self {
            listing,
            selected,
            preview,
        }
    }
}

#[derive(Component, Clone)]
struct PendingDirectoryPreview {
    revision: u64,
    path: PathBuf,
}

#[derive(Component)]
struct DirectoryPreviewTask {
    revision: u64,
    task: GitTask<DirectoryListing>,
}

#[derive(Clone)]
struct DirectoryListing {
    path: PathBuf,
    parent_path: Option<PathBuf>,
    entries: Vec<FileDirEntry>,
    parent_entries: Vec<FileDirEntry>,
    repo_root: Option<PathBuf>,
}

impl DirectoryListing {
    fn read(path: &Path) -> Self {
        let path = Self::initial_path(path);
        let parent_path = path.parent().map(Path::to_path_buf);
        let parent_entries = parent_path
            .as_deref()
            .map(Self::read_entries)
            .unwrap_or_default();
        let repo_root = super::repository::GitRepository::discover(&path)
            .ok()
            .map(|repository| repository.path().to_path_buf());
        Self {
            entries: Self::read_entries(&path),
            path,
            parent_path,
            parent_entries,
            repo_root,
        }
    }

    fn initial_path(path: &Path) -> PathBuf {
        let mut current = path.to_path_buf();
        while !current.is_dir() && current.pop() {}
        if current.is_dir() && !current.as_os_str().is_empty() {
            return current;
        }
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_dir())
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    fn read_entries(path: &Path) -> Vec<FileDirEntry> {
        let Ok(read) = std::fs::read_dir(path) else {
            return Vec::new();
        };
        let mut entries = Vec::new();
        for entry in read.flatten() {
            let path = entry.path();
            let is_dir = entry
                .file_type()
                .map(|kind| {
                    kind.is_dir()
                        || kind.is_symlink()
                            && std::fs::metadata(&path)
                                .map(|metadata| metadata.is_dir())
                                .unwrap_or(false)
                })
                .unwrap_or(false);
            entries.push(FileDirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: path.to_string_lossy().into_owned(),
                is_dir,
            });
        }
        entries.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        entries
    }

    fn visible_entries(&self, show_hidden: bool) -> Vec<FileDirEntry> {
        Self::visible(&self.entries, show_hidden)
    }

    fn visible_parent_entries(&self, show_hidden: bool) -> Vec<FileDirEntry> {
        Self::visible(&self.parent_entries, show_hidden)
    }

    fn visible_entry(&self, show_hidden: bool, index: usize) -> Option<&FileDirEntry> {
        if show_hidden {
            return self.entries.get(index);
        }
        self.entries
            .iter()
            .filter(|entry| !entry.name.starts_with('.'))
            .nth(index)
    }

    fn visible(entries: &[FileDirEntry], show_hidden: bool) -> Vec<FileDirEntry> {
        if show_hidden {
            return entries.to_vec();
        }
        entries
            .iter()
            .filter(|entry| !entry.name.starts_with('.'))
            .cloned()
            .collect()
    }
}

#[derive(EntityEvent)]
struct DirectoryLoad {
    #[event_target]
    webview: Entity,
    path: String,
    came_from: String,
}

fn open_request(trigger: On<UiInput<GitDirectoryOpenRequest>>, mut commands: Commands) {
    commands.trigger(DirectoryLoad {
        webview: trigger.event().webview,
        path: trigger.event().payload.path.clone(),
        came_from: String::new(),
    });
}

fn select_request(
    trigger: On<UiInput<GitDirectorySelectRequest>>,
    mut navigation: Query<&mut GitDirectoryNavigation>,
    mut commands: Commands,
) {
    let Ok(mut navigation) = navigation.get_mut(trigger.event().webview) else {
        return;
    };
    let Ok(index) = usize::try_from(trigger.event().payload.index) else {
        return;
    };
    let preview = navigation.select(index);
    let mut entity = commands.entity(trigger.event().webview);
    entity.remove::<(PendingDirectoryPreview, DirectoryPreviewTask)>();
    if let Some(preview) = preview {
        entity.insert(preview);
    }
}

fn next_request(
    trigger: On<UiInput<GitDirectoryNextRequest>>,
    mut navigation: Query<&mut GitDirectoryNavigation>,
    mut commands: Commands,
) {
    let Ok(mut navigation) = navigation.get_mut(trigger.event().webview) else {
        return;
    };
    let next = navigation.selected.saturating_add(1);
    let preview = navigation.select(next);
    let mut entity = commands.entity(trigger.event().webview);
    entity.remove::<(PendingDirectoryPreview, DirectoryPreviewTask)>();
    if let Some(preview) = preview {
        entity.insert(preview);
    }
}

fn previous_request(
    trigger: On<UiInput<GitDirectoryPreviousRequest>>,
    mut navigation: Query<&mut GitDirectoryNavigation>,
    mut commands: Commands,
) {
    let Ok(mut navigation) = navigation.get_mut(trigger.event().webview) else {
        return;
    };
    let previous = navigation.selected.saturating_sub(1);
    let preview = navigation.select(previous);
    let mut entity = commands.entity(trigger.event().webview);
    entity.remove::<(PendingDirectoryPreview, DirectoryPreviewTask)>();
    if let Some(preview) = preview {
        entity.insert(preview);
    }
}

fn ascend_request(
    trigger: On<UiInput<GitDirectoryAscendRequest>>,
    navigation: Query<&GitDirectoryNavigation>,
    mut commands: Commands,
) {
    let Ok(navigation) = navigation.get(trigger.event().webview) else {
        return;
    };
    let Some(path) = navigation
        .current
        .as_ref()
        .and_then(|current| current.parent_path.as_ref())
    else {
        return;
    };
    commands.trigger(DirectoryLoad {
        webview: trigger.event().webview,
        path: path.to_string_lossy().into_owned(),
        came_from: trigger.event().payload.target.clone(),
    });
}

fn descend_request(
    trigger: On<UiInput<GitDirectoryDescendRequest>>,
    navigation: Query<&GitDirectoryNavigation>,
    mut commands: Commands,
) {
    let Ok(navigation) = navigation.get(trigger.event().webview) else {
        return;
    };
    let Some(entry) = navigation.selected_entry().filter(|entry| entry.is_dir) else {
        return;
    };
    commands.trigger(DirectoryLoad {
        webview: trigger.event().webview,
        path: entry.path.clone(),
        came_from: trigger.event().payload.target.clone(),
    });
}

fn activate_request(
    trigger: On<UiInput<GitDirectoryActivateRequest>>,
    navigation: Query<&GitDirectoryNavigation>,
    mut commands: Commands,
) {
    let Ok(navigation) = navigation.get(trigger.event().webview) else {
        return;
    };
    let Some(entry) = navigation.selected_entry().filter(|entry| entry.is_dir) else {
        return;
    };
    commands.trigger(DirectoryLoad {
        webview: trigger.event().webview,
        path: entry.path.clone(),
        came_from: String::new(),
    });
}

fn parent_request(
    trigger: On<UiInput<GitDirectoryParentRequest>>,
    navigation: Query<&GitDirectoryNavigation>,
    mut commands: Commands,
) {
    let Ok(navigation) = navigation.get(trigger.event().webview) else {
        return;
    };
    let Some(current) = navigation.current.as_ref() else {
        return;
    };
    let Some(parent) = current.parent_path.as_ref() else {
        return;
    };
    commands.trigger(DirectoryLoad {
        webview: trigger.event().webview,
        path: parent.to_string_lossy().into_owned(),
        came_from: current.path.to_string_lossy().into_owned(),
    });
}

fn toggle_hidden(
    trigger: On<UiInput<GitDirectoryToggleHiddenRequest>>,
    mut navigation: Query<&mut GitDirectoryNavigation>,
    mut commands: Commands,
) {
    let Ok(mut navigation) = navigation.get_mut(trigger.event().webview) else {
        return;
    };
    navigation.show_hidden = !navigation.show_hidden;
    let selected = navigation.selected;
    let preview = navigation.select(selected);
    let mut entity = commands.entity(trigger.event().webview);
    entity.remove::<(PendingDirectoryPreview, DirectoryPreviewTask)>();
    if let Some(preview) = preview {
        entity.insert(preview);
    }
}

fn load(
    trigger: On<DirectoryLoad>,
    mut navigation: Query<&mut GitDirectoryNavigation>,
    mut states: super::state::GitStates,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    if !states.contains(entity) {
        return;
    }
    let Ok(mut navigation) = navigation.get_mut(entity) else {
        return;
    };
    let path = PathBuf::from(&trigger.event().path);
    states.start_directory(entity, &path);
    let revision = navigation.next_load_revision();
    let pending = PendingDirectoryLoad {
        revision,
        path,
        came_from: trigger.event().came_from.clone(),
        show_hidden: navigation.show_hidden,
    };
    commands
        .entity(entity)
        .remove::<(
            DirectoryLoadTask,
            PendingDirectoryPreview,
            DirectoryPreviewTask,
        )>()
        .insert(pending);
}

fn start_loads(
    pending: Query<(Entity, &PendingDirectoryLoad), Without<DirectoryLoadTask>>,
    runtime: Res<GitRuntime>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (entity, pending) in &pending {
        let request = pending.clone();
        let revision = request.revision;
        let Some(task) = runtime.spawn(wake.clone(), move || {
            DirectoryLoadOutput::read(&request.path, &request.came_from, request.show_hidden)
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<PendingDirectoryLoad>()
            .insert(DirectoryLoadTask { revision, task });
    }
}

fn poll_loads(
    mut tasks: Query<(Entity, &mut DirectoryLoadTask, &mut GitDirectoryNavigation)>,
    mut pages: Query<&mut vmux_ecs::PageMetadata>,
    mut states: super::state::GitStates,
    mut commands: Commands,
) {
    for (entity, mut task, mut navigation) in &mut tasks {
        let Some(output) = task.task.poll() else {
            continue;
        };
        commands.entity(entity).remove::<DirectoryLoadTask>();
        let Ok(output) = output else {
            states.finish_directory(entity);
            continue;
        };
        if navigation.load_revision != task.revision {
            continue;
        }
        let listing = output.listing;
        states.start_directory(entity, &listing.path);
        navigation.selected = output.selected;
        navigation.preview = output.preview;
        if let Ok(mut page) = pages.get_mut(entity)
            && crate::GitUrl::is_page(&page.url)
        {
            if let Some(url) = crate::GitUrl::from_path(&listing.path) {
                page.url = url;
            }
            let name = listing
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "Git".to_string());
            page.title = format!("Git · {name}");
        }
        let repo_root = listing.repo_root.clone();
        navigation.current = Some(listing);
        let Some(repo_root) = repo_root else {
            states.finish_directory(entity);
            continue;
        };
        let repo_root = repo_root.to_string_lossy().into_owned();
        commands.trigger(UiInput {
            webview: entity,
            payload: ProjectActivateRequest {
                path: repo_root.clone(),
                branch: String::new(),
                checkout: String::new(),
                pane_id: None,
            },
        });
        commands.trigger(UiInput {
            webview: entity,
            payload: GitRepositoryRequest { path: repo_root },
        });
    }
}

fn start_previews(
    pending: Query<(Entity, &PendingDirectoryPreview), Without<DirectoryPreviewTask>>,
    runtime: Res<GitRuntime>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (entity, pending) in &pending {
        let path = pending.path.clone();
        let revision = pending.revision;
        let Some(task) = runtime.spawn(wake.clone(), move || DirectoryListing::read(&path)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<PendingDirectoryPreview>()
            .insert(DirectoryPreviewTask { revision, task });
    }
}

fn poll_previews(
    mut tasks: Query<(
        Entity,
        &mut DirectoryPreviewTask,
        &mut GitDirectoryNavigation,
    )>,
    mut commands: Commands,
) {
    for (entity, mut task, mut navigation) in &mut tasks {
        let Some(output) = task.task.poll() else {
            continue;
        };
        commands.entity(entity).remove::<DirectoryPreviewTask>();
        let Ok(preview) = output else {
            continue;
        };
        if navigation.preview_revision == task.revision {
            navigation.preview = Some(preview);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, is_dir: bool) -> FileDirEntry {
        FileDirEntry {
            name: name.to_string(),
            path: format!("/tmp/{name}"),
            is_dir,
        }
    }

    #[test]
    fn projection_filters_hidden_entries_before_indexing() {
        let navigation = GitDirectoryNavigation {
            current: Some(DirectoryListing {
                path: "/tmp".into(),
                parent_path: Some("/".into()),
                entries: vec![entry(".hidden", true), entry("visible", true)],
                parent_entries: vec![entry(".parent", true), entry("sibling", true)],
                repo_root: None,
            }),
            preview: Some(DirectoryListing {
                path: "/tmp/visible".into(),
                parent_path: Some("/tmp".into()),
                entries: vec![entry(".child", false), entry("file", false)],
                parent_entries: Vec::new(),
                repo_root: None,
            }),
            selected: 0,
            show_hidden: false,
            load_revision: 0,
            preview_revision: 0,
        };

        let state = navigation.state();

        assert_eq!(state.entries, vec![entry("visible", true)]);
        assert_eq!(state.parent_entries, vec![entry("sibling", true)]);
        assert_eq!(state.children, Some(vec![entry("file", false)]));
        assert_eq!(navigation.selected_entry(), Some(&entry("visible", true)));
    }
}
