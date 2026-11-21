use crate::host::editor::EditReadOnly;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::ecs::system::Commands;
use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_ecs::event::{ExplorerCloseEditor, OpenEditorItem, OpenEditorsEvent};

use super::{ExplorerState, OpenEditorsDirty, TabsPlugin};
use crate::host::editor::{FileNavigateRequest, FileView};
use crate::host::parked_edits::{Buffers, Parked};
use vmux_ecs::store::StoreAppExt;

impl Plugin for TabsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<vmux_layout::CloseStackRequest>()
            .register_stored::<ArchivedEditorState>()
            .add_systems(Update, (sync_open_editors, emit_open_editors))
            .add_observer(close)
            .add_observer(close_item)
            .add_observer(reopen_item)
            .add_observer(capture)
            .add_observer(retain)
            .add_observer(transfer)
            .add_observer(restore)
            .add_observer(restore_buffers)
            .add_observer(resume);
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

#[derive(EntityEvent)]
pub(crate) struct RestoreEditors {
    pub(crate) stack: Entity,
    #[event_target]
    pub(crate) view: Entity,
}

impl Command for RestoreEditors {
    type Out = ();

    fn apply(self, world: &mut World) {
        world.trigger(self);
    }
}

#[derive(EntityEvent)]
struct ResumeEditors {
    #[event_target]
    view: Entity,
    path: PathBuf,
}

fn restore_buffers(
    request: On<RestoreEditors>,
    mut archives: Query<(&mut ArchivedEditorState, Option<&mut ClosedEditors>)>,
    mut views: Query<(&FileView, Option<&mut ExplorerState>)>,
    mut buffers: Buffers,
    mut commands: Commands,
) {
    let stack = request.stack;
    let view = request.view;
    let Ok((file, explorer)) = views.get_mut(view) else {
        return;
    };
    let Ok((mut state, closed)) = archives.get_mut(stack) else {
        return;
    };
    let path = file.path.clone();
    let state = std::mem::take(&mut *state);
    let closed = match closed {
        Some(mut closed) => std::mem::take(&mut *closed),
        None => ClosedEditors::default(),
    };
    commands
        .entity(stack)
        .remove::<(ArchivedEditorState, ClosedEditors)>();
    buffers.transfer(stack, view);
    if let Some(mut explorer) = explorer {
        explorer.restore_editors(state.paths.iter().map(PathBuf::from).collect(), &path);
    }
    if let Some((_, closed_at)) = closed.0.last() {
        commands
            .entity(view)
            .insert(vmux_ecs::page::PageItemHistory {
                closed_at: *closed_at,
            });
    }
    commands.entity(view).insert((closed, OpenEditorsDirty));
    if !state.preview_checkout.is_empty() {
        commands
            .entity(view)
            .insert(crate::host::navigation::CheckoutPreview(
                vmux_ecs::event::FileCheckoutState {
                    path: path.to_string_lossy().into_owned(),
                    repository: state.preview_repository,
                    checkout: state.preview_checkout,
                    read_only: true,
                },
            ));
    }
    commands.trigger(ResumeEditors { view, path });
}

fn resume(
    request: On<ResumeEditors>,
    views: Query<&FileView>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut buffers: Buffers,
) {
    let Ok(file) = views.get(request.view) else {
        return;
    };
    if file.path != request.path {
        return;
    }
    let keymap = crate::host::keymap::KeymapConfig::resolve(settings.as_deref()).keymap();
    buffers.resume(request.view, &request.path, keymap);
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

#[derive(EntityEvent)]
struct Captured {
    source: Entity,
    #[event_target]
    archive: Entity,
    state: ArchivedEditorState,
    closed: ClosedEditors,
    buffer: Option<Entity>,
}

#[derive(EntityEvent)]
struct Transfer {
    source: Entity,
    #[event_target]
    target: Entity,
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
        if world.get_entity(archive).is_err() {
            return;
        }
        let Ok(mut document) = world.get_entity_mut(source) else {
            return;
        };
        let editor = document.take::<crate::host::editor::EditorBundle>();
        let diff = if editor.is_some() {
            document.take::<vmux_git::GitDiffSource>()
        } else {
            None
        };
        let closed = document.take::<ClosedEditors>().unwrap_or_default();
        let buffer = match editor {
            Some(editor) => {
                let mut buffer = world.spawn((editor, ChildOf(source)));
                if let Some(diff) = diff {
                    buffer.insert(diff);
                }
                Some(buffer.id())
            }
            None => None,
        };
        world.trigger(Captured {
            source,
            archive,
            state,
            closed,
            buffer,
        });
    });
}

fn retain(
    mut request: On<Captured>,
    entities: Query<()>,
    mut buffers: Buffers,
    mut commands: Commands,
) {
    let request = request.event_mut();
    if !entities.contains(request.archive) || !entities.contains(request.source) {
        return;
    }
    if let Some(buffer) = request.buffer.take() {
        buffers.retain(request.source, buffer);
    }
    commands.entity(request.archive).insert((
        std::mem::take(&mut request.state),
        std::mem::take(&mut request.closed),
    ));
    commands.trigger(Transfer {
        source: request.source,
        target: request.archive,
    });
}

fn transfer(request: On<Transfer>, mut buffers: Buffers) {
    buffers.transfer(request.source, request.target);
}

fn restore(
    request: On<vmux_ecs::page::ArchiveRestoreRequest>,
    mut archives: Query<(&mut ArchivedEditorState, Option<&mut ClosedEditors>)>,
    targets: Query<()>,
    mut buffers: Buffers,
    mut commands: Commands,
) {
    let archive = request.archive;
    let stack = request.stack;
    if !targets.contains(stack) {
        return;
    }
    let Ok((mut state, closed)) = archives.get_mut(archive) else {
        return;
    };
    let state = std::mem::take(&mut *state);
    let closed = match closed {
        Some(mut closed) => std::mem::take(&mut *closed),
        None => ClosedEditors::default(),
    };
    commands
        .entity(archive)
        .remove::<(ArchivedEditorState, ClosedEditors)>();
    buffers.transfer(archive, stack);
    commands.entity(stack).insert((state, closed));
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

#[derive(bevy::ecs::system::SystemParam)]
struct Delivery<'w, 's> {
    views: Query<'w, 's, (), With<FileView>>,
    browsers: Option<NonSend<'w, Browsers>>,
}

impl Delivery<'_, '_> {
    fn available(&self, entity: Entity) -> bool {
        self.views.contains(entity)
            && self
                .browsers
                .as_ref()
                .is_some_and(|browsers| browsers.can_emit_to(&entity))
    }
}

#[allow(clippy::type_complexity)]
fn emit_open_editors(
    query: Query<(Entity, &FileView, &ExplorerState, Option<EditReadOnly>), OpenEditorsDirtyReady>,
    children: Query<&Children>,
    parked: Query<
        (
            &super::super::fold_preferences::FileId,
            EditReadOnly,
            &super::super::parked_edits::Modified,
        ),
        With<Parked>,
    >,
    delivery: Delivery,
    mut commands: Commands,
) {
    for (entity, file_view, state, edit) in &query {
        if !delivery.available(entity) {
            continue;
        }
        let active_dirty = edit.map(|edit| edit.core.revision.dirty).unwrap_or(false);
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
                false => children.get(entity).is_ok_and(|children| {
                    children.iter().any(|child| {
                        parked.get(child).is_ok_and(|(file, edit, _)| {
                            file.0 == *path && edit.core.revision.dirty
                        })
                    })
                }),
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
