use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};
use vmux_ecs::event::{
    FileBreadcrumbRequest, FileBreadcrumbState, FileDirEntry, FileDirectoryActivateRequest,
    FileDirectoryAscendRequest, FileDirectoryBackRequest, FileDirectoryDescendRequest,
    FileDirectoryNextRequest, FileDirectoryOpenRequest, FileDirectoryParentRequest,
    FileDirectoryPreviousRequest, FileDirectorySelectRequest, FileDirectoryState,
    FileDirectoryToggleHiddenRequest, FilePreviewRequest,
};
use vmux_ecs::{AsyncTask, TaskPlugin, TaskResult, TaskSet, TaskToken, TaskWaker};

use crate::host::editor::{FileNavigateRequest, FileView};
use crate::host::file_lifecycle::{EditorFileLoadedSet, FileDir};
use crate::host::media::FilePreviewLoad;
use crate::host::preview::PreviewBuilder;
use crate::host::status::FileInitialMetaSent;

pub(crate) struct DirectoryPlugin;

impl Plugin for DirectoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TaskPlugin::<FileBreadcrumbState>::default())
            .add_plugins(UiEventPlugin::<(
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
                FileBreadcrumbRequest,
            )>::default())
            .add_systems(
                Update,
                (
                    initialize.after(EditorFileLoadedSet),
                    publish.after(initialize),
                    cancel_breadcrumbs.before(TaskSet::of::<FileBreadcrumbState>()),
                    finish_breadcrumb.after(TaskSet::of::<FileBreadcrumbState>()),
                ),
            )
            .add_observer(select)
            .add_observer(next)
            .add_observer(previous)
            .add_observer(ascend)
            .add_observer(descend)
            .add_observer(activate)
            .add_observer(parent)
            .add_observer(open)
            .add_observer(back)
            .add_observer(toggle_hidden)
            .add_observer(breadcrumb);
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
            path: file.directory_title(),
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

#[derive(Component)]
struct BreadcrumbPath(String);

type ReadyDirectory = (
    Without<FileInitialMetaSent>,
    With<vmux_ecs::page::PageReady>,
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
    With<vmux_ecs::page::PageReady>,
>;

fn initialize(
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
        let (parent_path, parent_entries) = FileDir::parent(&file.path);
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

fn publish(directories: DirectoryProjection, browsers: NonSend<Browsers>, mut commands: Commands) {
    for (entity, file, directory, navigation) in &directories {
        if !directory.is_changed() && !navigation.is_changed() {
            continue;
        }
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let state = navigation.state(file, &directory);
        commands.trigger(vmux_ecs::FileUiStateWrite::from_event(entity, &state));
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
            if entry.is_dir || !PreviewBuilder::is_image(Path::new(&entry.path)) {
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

fn breadcrumb(
    trigger: On<UiInput<FileBreadcrumbRequest>>,
    views: Query<&FileView>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(view) = views.get(entity) else {
        return;
    };
    let path = PathBuf::from(&trigger.event().payload.path);
    if !view.path.starts_with(&path) || !path.is_dir() {
        return;
    }
    let path_text = path.to_string_lossy().into_owned();
    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
        entity,
        &FileBreadcrumbState {
            path: path_text.clone(),
            entries: Vec::new(),
            pending: true,
        },
    ));
    let request_path = path_text.clone();
    let task = IoTaskPool::get().spawn(async move {
        FileBreadcrumbState {
            path: path_text,
            entries: FileDir::read(&path),
            pending: false,
        }
    });
    commands.entity(entity).insert((
        AsyncTask::new(task),
        TaskWaker::new(vmux_ecs::wake::Wake::waker(proxy.as_deref())),
        BreadcrumbPath(request_path),
    ));
}

fn cancel_breadcrumbs(tasks: Query<(&FileView, &BreadcrumbPath, &TaskToken)>) {
    for (view, requested, token) in &tasks {
        if !view.path.starts_with(&requested.0) {
            token.cancel();
        }
    }
}

fn finish_breadcrumb(
    mut tasks: Query<(
        Entity,
        &mut TaskResult<FileBreadcrumbState>,
        &BreadcrumbPath,
    )>,
    mut commands: Commands,
) {
    for (entity, mut completion, requested) in &mut tasks {
        let Some(result) = completion.take() else {
            continue;
        };
        let state = match result {
            Ok(state) => state,
            Err(_) => FileBreadcrumbState {
                path: requested.0.clone(),
                entries: Vec::new(),
                pending: false,
            },
        };
        commands.queue(completion.guard(entity, BreadcrumbReady { entity, state }));
    }
}

struct BreadcrumbReady {
    entity: Entity,
    state: FileBreadcrumbState,
}

impl Command for BreadcrumbReady {
    type Out = ();

    fn apply(self, world: &mut World) {
        let Some(view) = world.get::<FileView>(self.entity) else {
            return;
        };
        if !view.path.starts_with(&self.state.path) {
            return;
        }
        if world.non_send::<Browsers>().can_emit_to(&self.entity) {
            world.trigger(vmux_ecs::FileUiStateWrite::from_event(
                self.entity,
                &self.state,
            ));
        }
    }
}

fn select(
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

fn next(
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

fn previous(
    trigger: On<UiInput<FileDirectoryPreviousRequest>>,
    mut navigation: Query<&mut FileDirectoryNavigation>,
) {
    let Ok(mut navigation) = navigation.get_mut(trigger.event().webview) else {
        return;
    };
    navigation.selected = navigation.selected.saturating_sub(1);
}

fn ascend(
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

fn descend(
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

fn activate(
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

fn parent(
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

fn open(
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

fn back(
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

fn toggle_hidden(
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

impl FileDir {
    pub(crate) fn read(path: &Path) -> Vec<FileDirEntry> {
        Self::try_read(path).unwrap_or_default()
    }

    pub(crate) fn try_read(path: &Path) -> Result<Vec<FileDirEntry>, String> {
        let read = std::fs::read_dir(path).map_err(|error| error.to_string())?;
        let mut entries: Vec<FileDirEntry> = read
            .flatten()
            .map(|entry| {
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
                FileDirEntry {
                    name: entry.file_name().to_string_lossy().to_string(),
                    path: path.to_string_lossy().to_string(),
                    is_dir,
                }
            })
            .collect();
        entries.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(entries)
    }

    pub(crate) fn parent(path: &Path) -> (String, Vec<FileDirEntry>) {
        match path.parent() {
            Some(parent) => (parent.to_string_lossy().to_string(), Self::read(parent)),
            None => (String::new(), Vec::new()),
        }
    }

    pub(crate) fn project_root(start: &Path) -> PathBuf {
        Self::project_root_with_knowledge(
            start,
            &vmux_knowledge::KnowledgeVault::user().into_root(),
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
        let mut directory = base;
        loop {
            if directory.join(".git").exists() {
                return directory.to_path_buf();
            }
            match directory.parent() {
                Some(parent) => directory = parent,
                None => break,
            }
        }
        base.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct BreadcrumbFixture {
        app: App,
        entity: Entity,
        root: tempfile::TempDir,
    }

    impl BreadcrumbFixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir(root.path().join("child")).unwrap();
            fs::write(root.path().join("child/file.rs"), "").unwrap();
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                DirectoryPlugin,
                vmux_ecs::UiStatePlugin::<vmux_ecs::event::FileUiState>::default(),
            ))
            .insert_non_send(Browsers::default())
            .add_observer(deliver);
            let entity = app
                .world_mut()
                .spawn((
                    vmux_ecs::PageMetadata {
                        url: "file:///fixture/child/file.rs".into(),
                        ..Default::default()
                    },
                    FileView {
                        path: root.path().join("child/file.rs"),
                    },
                    bevy_cef::prelude::BinIpcSurfaceGeneration(1),
                    bevy_cef::prelude::BinIpcNavigationRevision(1),
                    bevy_cef::prelude::BinIpcEpoch(1),
                ))
                .id();
            app.world_mut()
                .non_send_mut::<Browsers>()
                .set_externally_hosted(entity);
            app.world_mut().spawn(vmux_api::UiEventPermissions {
                url: "file://",
                owns_subtree: true,
                permissions: &["FileUiState"],
            });
            Self { app, entity, root }
        }

        fn request(&mut self, path: &Path) -> TaskToken {
            self.app.world_mut().trigger(UiInput {
                webview: self.entity,
                payload: FileBreadcrumbRequest {
                    path: path.to_string_lossy().into_owned(),
                },
            });
            self.app.world_mut().flush();
            self.app
                .world()
                .get::<TaskToken>(self.entity)
                .unwrap()
                .clone()
        }

        fn wait(&mut self, path: &Path) -> FileBreadcrumbState {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                self.app.update();
                if let Some(state) = self
                    .app
                    .world()
                    .get::<vmux_ecs::UiState<vmux_ecs::event::FileUiState>>(self.entity)
                    .and_then(vmux_ecs::UiState::current)
                {
                    let state = &state.document.breadcrumb;
                    if state.path == path.to_string_lossy() && !state.pending {
                        return state.clone();
                    }
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    fn deliver(event: On<bevy_cef::prelude::BinHostEmitEvent>) {
        event.mark_delivered();
    }

    #[test]
    fn successive_breadcrumb_requests_publish_only_the_latest_directory() {
        let mut fixture = BreadcrumbFixture::new();
        let first = fixture.root.path().to_path_buf();
        let latest = first.join("child");
        let previous = fixture.request(&first);
        fixture.request(&latest);
        let state = fixture.wait(&latest);
        assert!(previous.is_canceled());
        let names = state
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["file.rs"]);
        assert!(
            fixture
                .app
                .world()
                .get::<AsyncTask<FileBreadcrumbState>>(fixture.entity)
                .is_none()
        );
    }

    #[test]
    fn leaving_the_document_scope_cancels_its_breadcrumb_read() {
        let mut fixture = BreadcrumbFixture::new();
        let path = fixture.root.path().to_path_buf();
        let token = fixture.request(&path);
        fixture
            .app
            .world_mut()
            .get_mut::<FileView>(fixture.entity)
            .unwrap()
            .path = PathBuf::from("/different/file.rs");
        for _ in 0..3 {
            fixture.app.update();
        }
        assert!(token.is_canceled());
        assert!(
            fixture
                .app
                .world()
                .get::<AsyncTask<FileBreadcrumbState>>(fixture.entity)
                .is_none()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<TaskResult<FileBreadcrumbState>>(fixture.entity)
                .is_none()
        );
        let state = fixture
            .app
            .world()
            .get::<vmux_ecs::UiState<vmux_ecs::event::FileUiState>>(fixture.entity)
            .and_then(vmux_ecs::UiState::current)
            .unwrap();
        assert!(state.document.breadcrumb.entries.is_empty());
    }

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
        let entries = FileDir::read(tmp.path());
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
        assert_eq!(FileDir::project_root(&file), root);
        assert_eq!(FileDir::project_root(&sub), root);
    }

    #[test]
    fn project_root_falls_back_to_containing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("nogit");
        fs::create_dir(&sub).unwrap();
        let file = sub.join("a.txt");
        fs::write(&file, "x").unwrap();
        assert_eq!(FileDir::project_root(&file), sub);
    }

    #[test]
    fn project_root_uses_full_knowledge_vault() {
        let tmp = tempfile::tempdir().unwrap();
        let knowledge = tmp.path().join("knowledge");
        let projects = knowledge.join("projects");
        fs::create_dir_all(&projects).unwrap();
        let file = projects.join("note.md");
        fs::write(&file, "# Note").unwrap();
        assert_eq!(
            FileDir::project_root_with_knowledge(&file, &knowledge),
            knowledge
        );
    }

    #[test]
    fn parent_listing_of_nested_is_some_root_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let child = tmp.path().join("child");
        fs::create_dir(&child).unwrap();
        let (pp, pe) = FileDir::parent(&child);
        assert_eq!(pp, tmp.path().to_string_lossy());
        assert!(pe.iter().any(|e| e.name == "child"));

        let (rp, re) = FileDir::parent(Path::new("/"));
        assert!(rp.is_empty());
        assert!(re.is_empty());
    }
}
