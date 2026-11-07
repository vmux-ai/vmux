use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_ecs::event::{ExplorerCloseEditor, OpenEditorItem, OpenEditorsEvent};

use super::{ExplorerState, OpenEditorsDirty, TabsPlugin};
use crate::host::editor::{Editor, FileNavigateRequest, FileView, ParkedEdits};
use vmux_ecs::persistence::PersistenceAppExt;

impl Plugin for TabsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<vmux_layout::CloseStackRequest>()
            .register_persisted::<ArchivedEditorState>()
            .add_systems(Update, (sync_open_editors, emit_open_editors))
            .add_observer(close)
            .add_observer(close_item)
            .add_observer(reopen_item)
            .add_observer(capture)
            .add_observer(restore);
    }
}

#[derive(Component, Reflect, Clone, Default)]
#[reflect(Component)]
#[type_path = "vmux_editor"]
pub(crate) struct ArchivedEditorState {
    pub(crate) paths: Vec<String>,
    pub(crate) active: String,
    preview_repository: String,
    preview_checkout: String,
}

#[derive(Component, Default)]
struct ClosedEditors(Vec<(PathBuf, i64)>);

#[derive(Component, Default)]
pub(crate) struct ArchivedEditorRuntime {
    parked: ParkedEdits,
    closed: ClosedEditors,
}

pub(crate) struct RestoreEditors {
    pub(crate) stack: Entity,
    pub(crate) view: Entity,
}

impl Command for RestoreEditors {
    type Out = ();

    fn apply(self, world: &mut World) {
        let stack = self.stack;
        let view = self.view;
        let Ok(mut owner) = world.get_entity_mut(stack) else {
            return;
        };
        let Some(state) = owner.take::<ArchivedEditorState>() else {
            return;
        };
        let mut runtime = owner.take::<ArchivedEditorRuntime>().unwrap_or_default();
        let settings = world.get_resource::<vmux_setting::AppSettings>();
        let keymap = crate::host::keymap::KeymapConfig::resolve(settings).keymap();
        let Ok(mut document) = world.get_entity_mut(view) else {
            return;
        };
        let Some(file) = document.get::<FileView>() else {
            return;
        };
        let path = file.path.clone();
        let active = runtime.parked.resume(&path);
        if let Some(mut explorer) = document.get_mut::<ExplorerState>() {
            explorer.restore_editors(state.paths.iter().map(PathBuf::from).collect(), &path);
        }
        if let Some((_, closed_at)) = runtime.closed.0.last() {
            document.insert(vmux_ecs::page::PageItemHistory {
                closed_at: *closed_at,
            });
        }
        document.insert((runtime.parked, runtime.closed, OpenEditorsDirty));
        if let Some(active) = active {
            document.insert((active.edit, active.diff, keymap));
        }
        if !state.preview_checkout.is_empty() {
            document.insert(crate::host::navigation::CheckoutPreview(
                vmux_ecs::event::FileCheckoutState {
                    path: path.to_string_lossy().into_owned(),
                    repository: state.preview_repository,
                    checkout: state.preview_checkout,
                    read_only: true,
                },
            ));
        }
    }
}

fn close_item(
    request: On<vmux_ecs::page::CloseItemRequest>,
    editors: Query<(&FileView, &ExplorerState)>,
    mut commands: Commands,
) {
    let entity = request.entity;
    let Ok((file, state)) = editors.get(entity) else {
        return;
    };
    if state.open_editors.len() <= 1 {
        return;
    }
    request.handle();
    commands.trigger(UiInput {
        webview: entity,
        payload: ExplorerCloseEditor {
            path: file.path.to_string_lossy().into_owned(),
        },
    });
}

fn reopen_item(
    request: On<vmux_ecs::page::ReopenItemRequest>,
    mut closed: Query<(&mut ClosedEditors, &ExplorerState)>,
    mut commands: Commands,
) {
    let entity = request.entity;
    let Ok((mut closed, state)) = closed.get_mut(entity) else {
        return;
    };
    while let Some((path, closed_at)) = closed.0.last().cloned() {
        if closed_at < request.closed_after {
            return;
        }
        closed.0.pop();
        if state.open_editors.contains(&path) {
            continue;
        }
        request.handle();
        commands.trigger(FileNavigateRequest::new(entity, path, 0));
        if let Some((_, closed_at)) = closed.0.last() {
            commands
                .entity(entity)
                .insert(vmux_ecs::page::PageItemHistory {
                    closed_at: *closed_at,
                });
        } else {
            commands
                .entity(entity)
                .remove::<vmux_ecs::page::PageItemHistory>();
        }
        return;
    }
    commands
        .entity(entity)
        .remove::<vmux_ecs::page::PageItemHistory>();
}

fn capture(
    request: On<vmux_ecs::page::ArchiveCaptureRequest>,
    documents: Query<(
        &FileView,
        &ExplorerState,
        Option<&crate::host::navigation::CheckoutPreview>,
        &vmux_ecs::PageMetadata,
    )>,
    mut commands: Commands,
) {
    let source = request.source;
    let archive = request.archive;
    let Ok((file, explorer, preview, metadata)) = documents.get(source) else {
        return;
    };
    if !metadata.url.starts_with("file:") {
        return;
    }
    let state = ArchivedEditorState {
        paths: explorer
            .open_editors
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        active: file.path.to_string_lossy().into_owned(),
        preview_repository: preview
            .map(|preview| preview.0.repository.clone())
            .unwrap_or_default(),
        preview_checkout: preview
            .map(|preview| preview.0.checkout.clone())
            .unwrap_or_default(),
    };
    commands.queue(move |world: &mut World| {
        let Ok(mut document) = world.get_entity_mut(source) else {
            return;
        };
        let mut parked = document.take::<ParkedEdits>().unwrap_or_default();
        if let Some(edit) = document.take::<Editor>() {
            let path = edit.core.buffer.path.clone();
            let diff = document
                .take::<vmux_git::GitDiffSource>()
                .unwrap_or_else(|| vmux_git::GitDiffSource {
                    content: edit.core.buffer.text(),
                    dirty: edit.core.dirty,
                });
            let modified = ParkedEdits::modified_at(&path);
            parked.insert(
                path,
                crate::host::editor::ParkedEdit {
                    edit,
                    diff,
                    modified,
                },
            );
        }
        let closed = document.take::<ClosedEditors>().unwrap_or_default();
        if let Ok(mut entry) = world.get_entity_mut(archive) {
            entry.insert((state, ArchivedEditorRuntime { parked, closed }));
        }
    });
}

fn restore(request: On<vmux_ecs::page::ArchiveRestoreRequest>, mut commands: Commands) {
    let archive = request.archive;
    let stack = request.stack;
    commands.queue(move |world: &mut World| {
        let Ok(mut entry) = world.get_entity_mut(archive) else {
            return;
        };
        let Some(state) = entry.take::<ArchivedEditorState>() else {
            return;
        };
        let runtime = entry.take::<ArchivedEditorRuntime>().unwrap_or_default();
        if let Ok(mut stack) = world.get_entity_mut(stack) {
            stack.insert((state, runtime));
        }
    });
}

type OpenEditorsDirtyReady = (With<OpenEditorsDirty>, With<vmux_ecs::page::PageReady>);

fn sync_open_editors(
    mut query: Query<(Entity, &FileView, &mut ExplorerState), Changed<FileView>>,
    mut commands: Commands,
) {
    for (entity, file_view, mut state) in &mut query {
        if state.active_editor_is_dir
            && let Some(previous) = state.active_editor.clone()
        {
            state.open_editors.retain(|open| open != &previous);
        }
        state.note_open(&file_view.path);
        state.active_editor = Some(file_view.path.clone());
        state.active_editor_is_dir = file_view.path.is_dir();
        commands.entity(entity).insert(OpenEditorsDirty);
    }
}

#[allow(clippy::type_complexity)]
fn emit_open_editors(
    query: Query<
        (
            Entity,
            &FileView,
            &ExplorerState,
            Option<&Editor>,
            Option<&ParkedEdits>,
        ),
        OpenEditorsDirtyReady,
    >,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let Some(browsers) = browsers else {
        return;
    };
    for (entity, file_view, state, edit, parked) in &query {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let active_dirty = edit.map(|edit| edit.core.dirty).unwrap_or(false);
        let mut name_counts = HashMap::new();
        for path in &state.open_editors {
            let name = OpenEditorPath::name(path);
            *name_counts.entry(name).or_insert(0usize) += 1;
        }
        let mut items = Vec::with_capacity(state.open_editors.len());
        for path in &state.open_editors {
            let active = *path == file_view.path;
            let dirty = match active {
                true => active_dirty,
                false => parked.is_some_and(|parked| parked.is_dirty(path)),
            };
            let name = OpenEditorPath::name(path);
            let context = match name_counts.get(&name).copied().unwrap_or_default() > 1 {
                true => OpenEditorPath::parent(path),
                false => String::new(),
            };
            items.push(OpenEditorItem {
                name,
                context,
                path: path.to_string_lossy().into_owned(),
                active,
                dirty,
                is_dir: path.is_dir(),
            });
        }
        commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
            entity,
            &OpenEditorsEvent { items },
        ));
        commands.entity(entity).remove::<OpenEditorsDirty>();
    }
}

struct OpenEditorPath;

impl OpenEditorPath {
    fn name(path: &Path) -> String {
        path.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string())
    }

    fn parent(path: &Path) -> String {
        let Some(parent) = path.parent() else {
            return String::new();
        };
        match parent.file_name() {
            Some(name) => name.to_string_lossy().into_owned(),
            None if parent.has_root() => "/".to_string(),
            None => String::new(),
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct EditorStacks<'w, 's> {
    views: Query<'w, 's, &'static FileView>,
    child_of: Query<'w, 's, &'static ChildOf>,
    stacks: Query<'w, 's, (), With<vmux_layout::stack::Stack>>,
}

fn close(
    trigger: On<UiInput<ExplorerCloseEditor>>,
    owner: EditorStacks,
    mut states: Query<&mut ExplorerState>,
    mut closed: Query<&mut ClosedEditors>,
    mut closing: MessageWriter<vmux_layout::CloseStackRequest>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(mut state) = states.get_mut(entity) else {
        return;
    };
    if state.open_editors.len() > 1 && state.open_editors.contains(&path) {
        let closed_at = vmux_ecs::UnixMillis::now().0;
        if let Ok(mut history) = closed.get_mut(entity) {
            history.0.push((path.clone(), closed_at));
        } else {
            commands
                .entity(entity)
                .insert(ClosedEditors(vec![(path.clone(), closed_at)]));
        }
        commands
            .entity(entity)
            .insert(vmux_ecs::page::PageItemHistory { closed_at });
    }
    let next = state.close_editor(&path);
    commands.entity(entity).insert(OpenEditorsDirty);
    let Some(next) = next else {
        let mut current = entity;
        for _ in 0..8 {
            if owner.stacks.contains(current) {
                closing.write(vmux_layout::CloseStackRequest::by_user(current));
                break;
            }
            let Ok(parent) = owner.child_of.get(current) else {
                break;
            };
            current = parent.parent();
        }
        return;
    };
    let Ok(file_view) = owner.views.get(entity) else {
        return;
    };
    if file_view.path != path {
        return;
    }
    commands.trigger(FileNavigateRequest::new(entity, next, 0));
}
