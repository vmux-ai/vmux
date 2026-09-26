use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_core::event::FileDirEntry;
use vmux_core::event::space::ProjectActivateRequest;

use crate::event::{
    GitDirectoryAscendRequest, GitDirectoryDescendRequest, GitDirectoryOpenRequest,
    GitDirectorySelectRequest, GitDirectoryToggleHiddenRequest, GitRepositoryRequest,
};
use crate::state::GitDirectoryState;

pub(super) struct DirectoryPlugin;

impl Plugin for DirectoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            GitDirectoryOpenRequest,
            GitDirectorySelectRequest,
            GitDirectoryAscendRequest,
            GitDirectoryDescendRequest,
            GitDirectoryToggleHiddenRequest,
        )>::default())
            .add_observer(on_directory_open_request)
            .add_observer(on_directory_select_request)
            .add_observer(on_directory_ascend_request)
            .add_observer(on_directory_descend_request)
            .add_observer(on_directory_toggle_hidden_request)
            .add_observer(load_directory);
    }
}

#[derive(Component)]
pub(super) struct GitDirectoryNavigation {
    current: Option<DirectoryListing>,
    preview: Option<DirectoryListing>,
    selected: usize,
    show_hidden: bool,
}

impl Default for GitDirectoryNavigation {
    fn default() -> Self {
        Self {
            current: None,
            preview: None,
            selected: 0,
            show_hidden: true,
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
        let repo_root = super::runner::repo_root(&path).ok();
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

fn on_directory_open_request(
    trigger: On<UiInput<GitDirectoryOpenRequest>>,
    mut commands: Commands,
) {
    commands.trigger(DirectoryLoad {
        webview: trigger.event().webview,
        path: trigger.event().payload.path.clone(),
        came_from: String::new(),
    });
}

fn on_directory_select_request(
    trigger: On<UiInput<GitDirectorySelectRequest>>,
    mut navigation: Query<&mut GitDirectoryNavigation>,
) {
    let Ok(mut navigation) = navigation.get_mut(trigger.event().webview) else {
        return;
    };
    let Ok(index) = usize::try_from(trigger.event().payload.index) else {
        return;
    };
    let Some(entry) = navigation
        .current
        .as_ref()
        .and_then(|current| current.visible_entry(navigation.show_hidden, index))
        .cloned()
    else {
        return;
    };
    navigation.selected = index;
    navigation.preview = entry
        .is_dir
        .then(|| DirectoryListing::read(Path::new(&entry.path)));
}

fn on_directory_ascend_request(
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

fn on_directory_descend_request(
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

fn on_directory_toggle_hidden_request(
    trigger: On<UiInput<GitDirectoryToggleHiddenRequest>>,
    mut navigation: Query<&mut GitDirectoryNavigation>,
) {
    let Ok(mut navigation) = navigation.get_mut(trigger.event().webview) else {
        return;
    };
    navigation.show_hidden = !navigation.show_hidden;
    let entries = navigation
        .current
        .as_ref()
        .map(|current| current.visible_entries(navigation.show_hidden))
        .unwrap_or_default();
    navigation.selected = navigation.selected.min(entries.len().saturating_sub(1));
    navigation.preview = entries
        .get(navigation.selected)
        .filter(|entry| entry.is_dir)
        .map(|entry| DirectoryListing::read(Path::new(&entry.path)));
}

fn load_directory(
    trigger: On<DirectoryLoad>,
    mut pages: Query<&mut vmux_core::PageMetadata>,
    mut views: Query<(&mut super::state::GitState, &mut GitDirectoryNavigation)>,
    mut commands: Commands,
) {
    let Ok((mut view, mut navigation)) = views.get_mut(trigger.event_target()) else {
        return;
    };
    let listing = DirectoryListing::read(Path::new(&trigger.event().path));
    view.start_directory(&listing.path);
    let path_changed = navigation
        .current
        .as_ref()
        .is_none_or(|current| current.path != listing.path);
    if path_changed {
        let entries = listing.visible_entries(navigation.show_hidden);
        navigation.selected = entries
            .iter()
            .position(|entry| entry.path == trigger.event().came_from)
            .unwrap_or(0);
    }
    navigation.preview = listing
        .visible_entry(navigation.show_hidden, navigation.selected)
        .filter(|entry| entry.is_dir)
        .map(|entry| DirectoryListing::read(Path::new(&entry.path)));
    if let Ok(mut page) = pages.get_mut(trigger.event_target()) {
        if let Some(url) = crate::GitUrl::from_path(&listing.path) {
            page.url = url;
        }
        let name = listing
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Git".to_string());
        page.title = format!("{name} · Git");
    }
    let repo_root = listing.repo_root.clone();
    navigation.current = Some(listing);
    let Some(repo_root) = repo_root else {
        view.finish_directory();
        return;
    };
    let repo_root = repo_root.to_string_lossy().into_owned();
    commands.trigger(UiInput {
        webview: trigger.event_target(),
        payload: ProjectActivateRequest {
            path: repo_root.clone(),
            branch: String::new(),
            checkout: String::new(),
            pane_id: None,
        },
    });
    commands.trigger(UiInput {
        webview: trigger.event_target(),
        payload: GitRepositoryRequest { path: repo_root },
    });
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
        };

        let state = navigation.state();

        assert_eq!(state.entries, vec![entry("visible", true)]);
        assert_eq!(state.parent_entries, vec![entry("sibling", true)]);
        assert_eq!(state.children, Some(vec![entry("file", false)]));
        assert_eq!(navigation.selected_entry(), Some(&entry("visible", true)));
    }
}
