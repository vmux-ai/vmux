use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::{BinHostEmitEvent, BinReceive, UiEventPlugin};

use crate::event::{GitDirectoryEvent, GitDirectoryRequest};

pub(super) struct DirectoryPlugin;

impl Plugin for DirectoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(GitDirectoryRequest,)>::default())
            .add_observer(on_directory_request);
    }
}

struct GitDirectory;

impl GitDirectory {
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

    fn entries(path: &Path) -> Vec<vmux_core::event::FileDirEntry> {
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
            entries.push(vmux_core::event::FileDirEntry {
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

    fn event(path: &Path, preview: bool) -> GitDirectoryEvent {
        let path = Self::initial_path(path);
        let parent = path.parent().map(Path::to_path_buf);
        let parent_path = parent
            .as_ref()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default();
        let parent_entries = parent.as_deref().map(Self::entries).unwrap_or_default();
        let repo_root = super::runner::repo_root(&path)
            .map(|root| root.to_string_lossy().into_owned())
            .unwrap_or_default();
        GitDirectoryEvent {
            entries: Self::entries(&path),
            path: path.to_string_lossy().into_owned(),
            parent_path,
            parent_entries,
            repo_root,
            preview,
        }
    }
}

fn on_directory_request(
    trigger: On<BinReceive<GitDirectoryRequest>>,
    mut pages: Query<&mut vmux_core::PageMetadata>,
    mut views: Query<&mut super::state::GitState>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    if let Ok(mut view) = views.get_mut(trigger.event().webview) {
        view.start_directory(Path::new(&request.path), request.preview);
    }
    let event = GitDirectory::event(Path::new(&request.path), request.preview);
    if !request.preview
        && let Ok(mut page) = pages.get_mut(trigger.event().webview)
    {
        if let Some(url) = crate::GitUrl::from_path(Path::new(&event.path)) {
            page.url = url;
        }
        let name = Path::new(&event.path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Git".to_string());
        page.title = format!("{name} · Git");
    }
    if let Ok(mut view) = views.get_mut(trigger.event().webview) {
        view.set_directory(event);
    } else {
        commands.trigger(BinHostEmitEvent::from_event(
            trigger.event().webview,
            &event,
        ));
    }
}
