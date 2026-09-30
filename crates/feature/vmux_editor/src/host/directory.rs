use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};
use vmux_core::event::{
    FileDirEntry, FileDirectoryActivateRequest, FileDirectoryAscendRequest,
    FileDirectoryBackRequest, FileDirectoryDescendRequest, FileDirectoryNextRequest,
    FileDirectoryOpenRequest, FileDirectoryParentRequest, FileDirectoryPreviousRequest,
    FileDirectorySelectRequest, FileDirectoryState, FileDirectoryToggleHiddenRequest,
    FilePreviewRequest,
};

use crate::host::editor::{FileNavigateRequest, FileView};
use crate::host::file_lifecycle::{EditorFileLoadedSet, FileDir};
use crate::host::media::FilePreviewLoad;
use crate::host::status::FileInitialMetaSent;

pub(crate) struct DirectoryPlugin;

impl Plugin for DirectoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            FileDirectorySelectRequest,
            FileDirectoryNextRequest,
            FileDirectoryPreviousRequest,
            FileDirectoryAscendRequest,
            FileDirectoryDescendRequest,
            FileDirectoryActivateRequest,
            FileDirectoryParentRequest,
            FileDirectoryOpenRequest,
            FileDirectoryBackRequest,
            FileDirectoryToggleHiddenRequest,
        )>::default())
            .add_systems(
                Update,
                (
                    initialize_directory.after(EditorFileLoadedSet),
                    publish_directory.after(initialize_directory),
                ),
            )
            .add_observer(on_select)
            .add_observer(on_next)
            .add_observer(on_previous)
            .add_observer(on_ascend)
            .add_observer(on_descend)
            .add_observer(on_activate)
            .add_observer(on_parent)
            .add_observer(on_open)
            .add_observer(on_back)
            .add_observer(on_toggle_hidden);
    }
}

#[derive(Component)]
pub(crate) struct FileDirectoryNavigation {
    path: PathBuf,
    parent_path: PathBuf,
    parent_entries: Vec<FileDirEntry>,
    selected: usize,
    pub(crate) show_hidden: bool,
}

impl Default for FileDirectoryNavigation {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            parent_path: PathBuf::new(),
            parent_entries: Vec::new(),
            selected: 0,
            show_hidden: true,
        }
    }
}

impl FileDirectoryNavigation {
    fn state(&self, file: &FileView, directory: &FileDir) -> FileDirectoryState {
        FileDirectoryState {
            path: file.display_path(),
            abs_path: file.path.to_string_lossy().into_owned(),
            entries: Self::visible(&directory.entries, self.show_hidden),
            parent_entries: Self::visible(&self.parent_entries, self.show_hidden),
            selected: u32::try_from(self.selected).unwrap_or(u32::MAX),
            show_hidden: self.show_hidden,
        }
    }

    fn selected_entry<'a>(&self, directory: &'a FileDir) -> Option<&'a FileDirEntry> {
        if self.show_hidden {
            return directory.entries.get(self.selected);
        }
        directory
            .entries
            .iter()
            .filter(|entry| !entry.name.starts_with('.'))
            .nth(self.selected)
    }

    pub(crate) fn selects(&self, directory: &FileDir, path: &str) -> bool {
        self.selected_entry(directory)
            .is_some_and(|entry| entry.path == path)
    }

    pub(crate) fn visible(entries: &[FileDirEntry], show_hidden: bool) -> Vec<FileDirEntry> {
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

#[derive(Component)]
struct FileBackDirectory {
    path: PathBuf,
    select: String,
}

#[derive(Component)]
struct DirectorySelectionTarget(String);

type ReadyDirectory = (
    Without<FileInitialMetaSent>,
    With<vmux_core::page::PageReady>,
);

type DirectoryProjection<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static FileView,
        Ref<'static, FileDir>,
        Ref<'static, FileDirectoryNavigation>,
    ),
    With<vmux_core::page::PageReady>,
>;

fn initialize_directory(
    mut directories: Query<
        (
            Entity,
            &FileView,
            &FileDir,
            &mut FileDirectoryNavigation,
            Option<&DirectorySelectionTarget>,
        ),
        ReadyDirectory,
    >,
    mut commands: Commands,
) {
    for (entity, file, directory, mut navigation, target) in &mut directories {
        let path_changed = navigation.path != file.path;
        navigation.path.clone_from(&file.path);
        let (parent_path, parent_entries) = parent_listing(&file.path);
        navigation.parent_path = PathBuf::from(parent_path);
        navigation.parent_entries = parent_entries;
        let visible = FileDirectoryNavigation::visible(&directory.entries, navigation.show_hidden);
        if path_changed {
            navigation.selected = target
                .and_then(|target| visible.iter().position(|entry| entry.path == target.0))
                .unwrap_or(0);
        } else {
            navigation.selected = navigation.selected.min(visible.len().saturating_sub(1));
        }
        commands.entity(entity).remove::<DirectorySelectionTarget>();
    }
}

fn publish_directory(
    directories: DirectoryProjection,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, file, directory, navigation) in &directories {
        if !directory.is_changed() && !navigation.is_changed() {
            continue;
        }
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let state = navigation.state(file, &directory);
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
            entity, &state,
        ));
        commands.entity(entity).insert(FileInitialMetaSent);
        if let Some(entry) = state.entries.get(state.selected as usize) {
            commands.trigger(FilePreviewLoad {
                webview: entity,
                request: FilePreviewRequest {
                    path: entry.path.clone(),
                    thumb: false,
                },
                selected_only: true,
            });
        }
        for entry in &state.entries {
            if entry.is_dir || !super::preview::is_image_path(Path::new(&entry.path)) {
                continue;
            }
            commands.trigger(FilePreviewLoad {
                webview: entity,
                request: FilePreviewRequest {
                    path: entry.path.clone(),
                    thumb: true,
                },
                selected_only: false,
            });
        }
    }
}

fn on_select(
    trigger: On<UiInput<FileDirectorySelectRequest>>,
    directories: Query<&FileDir>,
    mut navigation: Query<&mut FileDirectoryNavigation>,
) {
    let entity = trigger.event().webview;
    let Ok(directory) = directories.get(entity) else {
        return;
    };
    let Ok(mut navigation) = navigation.get_mut(entity) else {
        return;
    };
    let Ok(index) = usize::try_from(trigger.event().payload.index) else {
        return;
    };
    let visible = FileDirectoryNavigation::visible(&directory.entries, navigation.show_hidden);
    navigation.selected = index.min(visible.len().saturating_sub(1));
}

fn on_next(
    trigger: On<UiInput<FileDirectoryNextRequest>>,
    directories: Query<&FileDir>,
    mut navigation: Query<&mut FileDirectoryNavigation>,
) {
    let entity = trigger.event().webview;
    let Ok(directory) = directories.get(entity) else {
        return;
    };
    let Ok(mut navigation) = navigation.get_mut(entity) else {
        return;
    };
    let len = FileDirectoryNavigation::visible(&directory.entries, navigation.show_hidden).len();
    navigation.selected = (navigation.selected + 1).min(len.saturating_sub(1));
}

fn on_previous(
    trigger: On<UiInput<FileDirectoryPreviousRequest>>,
    mut navigation: Query<&mut FileDirectoryNavigation>,
) {
    let Ok(mut navigation) = navigation.get_mut(trigger.event().webview) else {
        return;
    };
    navigation.selected = navigation.selected.saturating_sub(1);
}

fn on_ascend(
    trigger: On<UiInput<FileDirectoryAscendRequest>>,
    navigation: Query<&FileDirectoryNavigation>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(navigation) = navigation.get(entity) else {
        return;
    };
    if navigation.parent_path.as_os_str().is_empty() {
        return;
    }
    commands.entity(entity).insert(DirectorySelectionTarget(
        trigger.event().payload.target.clone(),
    ));
    commands.trigger(FileNavigateRequest::new(
        entity,
        navigation.parent_path.clone(),
        0,
    ));
}

fn on_descend(
    trigger: On<UiInput<FileDirectoryDescendRequest>>,
    directories: Query<&FileDir>,
    navigation: Query<&FileDirectoryNavigation>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(directory) = directories.get(entity) else {
        return;
    };
    let Ok(navigation) = navigation.get(entity) else {
        return;
    };
    let Some(entry) = navigation
        .selected_entry(directory)
        .filter(|entry| entry.is_dir)
    else {
        return;
    };
    let path = PathBuf::from(&entry.path);
    commands.entity(entity).insert(DirectorySelectionTarget(
        trigger.event().payload.target.clone(),
    ));
    commands.trigger(FileNavigateRequest::new(entity, path, 0));
}

fn on_activate(
    trigger: On<UiInput<FileDirectoryActivateRequest>>,
    directories: Query<&FileDir>,
    views: Query<&FileView>,
    navigation: Query<&FileDirectoryNavigation>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(directory) = directories.get(entity) else {
        return;
    };
    let Ok(view) = views.get(entity) else {
        return;
    };
    let Ok(navigation) = navigation.get(entity) else {
        return;
    };
    let Some(entry) = navigation.selected_entry(directory).cloned() else {
        return;
    };
    let path = PathBuf::from(&entry.path);
    if entry.is_dir {
        commands.entity(entity).remove::<DirectorySelectionTarget>();
    } else {
        commands.entity(entity).insert(FileBackDirectory {
            path: view.path.clone(),
            select: entry.path,
        });
    }
    commands.trigger(FileNavigateRequest::new(entity, path, 0));
}

fn on_parent(
    trigger: On<UiInput<FileDirectoryParentRequest>>,
    views: Query<&FileView>,
    navigation: Query<&FileDirectoryNavigation>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(view) = views.get(entity) else {
        return;
    };
    let Ok(navigation) = navigation.get(entity) else {
        return;
    };
    if navigation.parent_path.as_os_str().is_empty() {
        return;
    }
    commands.entity(entity).insert(DirectorySelectionTarget(
        view.path.to_string_lossy().into_owned(),
    ));
    commands.trigger(FileNavigateRequest::new(
        entity,
        navigation.parent_path.clone(),
        0,
    ));
}

fn on_open(
    trigger: On<UiInput<FileDirectoryOpenRequest>>,
    views: Query<&FileView>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(view) = views.get(entity) else {
        return;
    };
    let path = PathBuf::from(&trigger.event().payload.path);
    if path.is_dir() {
        commands.entity(entity).remove::<DirectorySelectionTarget>();
    } else {
        let back = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| view.path.clone());
        commands.entity(entity).insert(FileBackDirectory {
            path: back,
            select: path.to_string_lossy().into_owned(),
        });
    }
    commands.trigger(FileNavigateRequest::new(entity, path, 0));
}

fn on_back(
    trigger: On<UiInput<FileDirectoryBackRequest>>,
    back: Query<&FileBackDirectory>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(back) = back.get(entity) else {
        return;
    };
    commands
        .entity(entity)
        .insert(DirectorySelectionTarget(back.select.clone()));
    commands.trigger(FileNavigateRequest::new(entity, back.path.clone(), 0));
}

fn on_toggle_hidden(
    trigger: On<UiInput<FileDirectoryToggleHiddenRequest>>,
    directories: Query<&FileDir>,
    mut navigation: Query<&mut FileDirectoryNavigation>,
) {
    let entity = trigger.event().webview;
    let Ok(directory) = directories.get(entity) else {
        return;
    };
    let Ok(mut navigation) = navigation.get_mut(entity) else {
        return;
    };
    navigation.show_hidden = !navigation.show_hidden;
    let visible = FileDirectoryNavigation::visible(&directory.entries, navigation.show_hidden);
    navigation.selected = navigation.selected.min(visible.len().saturating_sub(1));
}

pub fn list_dir(path: &Path) -> Vec<FileDirEntry> {
    let Ok(read) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut entries: Vec<FileDirEntry> = read
        .flatten()
        .map(|e| {
            let path = e.path();
            let is_dir = e
                .file_type()
                .map(|kind| {
                    kind.is_dir()
                        || kind.is_symlink()
                            && std::fs::metadata(&path)
                                .map(|metadata| metadata.is_dir())
                                .unwrap_or(false)
                })
                .unwrap_or(false);
            FileDirEntry {
                name: e.file_name().to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                is_dir,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

pub fn parent_listing(path: &Path) -> (String, Vec<FileDirEntry>) {
    match path.parent() {
        Some(p) => (p.to_string_lossy().to_string(), list_dir(p)),
        None => (String::new(), Vec::new()),
    }
}

pub fn project_root(start: &Path) -> PathBuf {
    project_root_with_knowledge(
        start,
        &vmux_core::knowledge::KnowledgeVault::user().into_root(),
    )
}

fn project_root_with_knowledge(start: &Path, knowledge: &Path) -> PathBuf {
    let base = if start.is_dir() {
        start
    } else {
        start.parent().unwrap_or(start)
    };
    if base.starts_with(knowledge) {
        return knowledge.to_path_buf();
    }
    let mut dir = base;
    loop {
        if dir.join(".git").exists() {
            return dir.to_path_buf();
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }
    base.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn entry(name: &str, is_dir: bool) -> FileDirEntry {
        FileDirEntry {
            name: name.to_string(),
            path: format!("/tmp/{name}"),
            is_dir,
        }
    }

    #[test]
    fn projection_filters_hidden_entries_before_indexing() {
        let file = FileView {
            path: PathBuf::from("/tmp"),
        };
        let directory = FileDir {
            entries: vec![entry(".hidden", true), entry("visible", false)],
        };
        let navigation = FileDirectoryNavigation {
            path: file.path.clone(),
            parent_path: PathBuf::from("/"),
            parent_entries: vec![entry(".parent", true), entry("sibling", true)],
            selected: 0,
            show_hidden: false,
        };

        let state = navigation.state(&file, &directory);

        assert_eq!(state.entries, vec![entry("visible", false)]);
        assert_eq!(state.parent_entries, vec![entry("sibling", true)]);
        assert_eq!(
            navigation.selected_entry(&directory),
            Some(&entry("visible", false))
        );
    }

    #[test]
    fn lists_dir_includes_dotfiles_dirs_first() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("zdir")).unwrap();
        fs::write(tmp.path().join("a.txt"), "x").unwrap();
        fs::write(tmp.path().join(".hidden"), "x").unwrap();
        let entries = list_dir(tmp.path());
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["zdir", ".hidden", "a.txt"]);
    }

    #[test]
    fn project_root_walks_up_to_git_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join(".git")).unwrap();
        let sub = root.join("crates").join("x");
        fs::create_dir_all(&sub).unwrap();
        let file = sub.join("lib.rs");
        fs::write(&file, "x").unwrap();
        assert_eq!(project_root(&file), root);
        assert_eq!(project_root(&sub), root);
    }

    #[test]
    fn project_root_falls_back_to_containing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("nogit");
        fs::create_dir(&sub).unwrap();
        let file = sub.join("a.txt");
        fs::write(&file, "x").unwrap();
        assert_eq!(project_root(&file), sub);
    }

    #[test]
    fn project_root_uses_full_knowledge_vault() {
        let tmp = tempfile::tempdir().unwrap();
        let knowledge = tmp.path().join("knowledge");
        let projects = knowledge.join("projects");
        fs::create_dir_all(&projects).unwrap();
        let file = projects.join("note.md");
        fs::write(&file, "# Note").unwrap();
        assert_eq!(project_root_with_knowledge(&file, &knowledge), knowledge);
    }

    #[test]
    fn parent_listing_of_nested_is_some_root_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let child = tmp.path().join("child");
        fs::create_dir(&child).unwrap();
        let (pp, pe) = parent_listing(&child);
        assert_eq!(pp, tmp.path().to_string_lossy());
        assert!(pe.iter().any(|e| e.name == "child"));

        let (rp, re) = parent_listing(Path::new("/"));
        assert!(rp.is_empty());
        assert!(re.is_empty());
    }
}
