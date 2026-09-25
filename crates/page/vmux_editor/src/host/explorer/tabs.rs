use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::event::{ExplorerCloseEditor, OpenEditorItem, OpenEditorsEvent};

use super::{ExplorerState, OpenEditorsDirty, TabsPlugin};
use crate::host::editor::{Editor, FileNavigateRequest, FileView, ParkedEdits};

impl Plugin for TabsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<vmux_layout::CloseStackRequest>()
            .add_systems(Update, (sync_open_editors, emit_open_editors))
            .add_observer(on_explorer_close_editor);
    }
}

type OpenEditorsDirtyReady = (With<OpenEditorsDirty>, With<vmux_core::page::PageReady>);

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
        crate::explorer_model::note_open(&mut state.open_editors, &file_view.path);
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
        let mut items = Vec::with_capacity(state.open_editors.len());
        for path in &state.open_editors {
            let active = *path == file_view.path;
            let dirty = match active {
                true => active_dirty,
                false => parked.is_some_and(|parked| parked.is_dirty(path)),
            };
            items.push(OpenEditorItem {
                name: OpenEditorPath::name(path),
                path: path.to_string_lossy().into_owned(),
                active,
                dirty,
                is_dir: path.is_dir(),
            });
        }
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
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
}

struct EditorPageClose(Entity);

impl EditorPageClose {
    fn holding(
        webview: Entity,
        child_of: &Query<&ChildOf>,
        stacks: &Query<(), With<vmux_layout::stack::Stack>>,
    ) -> Option<Self> {
        let mut current = webview;
        for _ in 0..8 {
            if stacks.contains(current) {
                return Some(Self(current));
            }
            let Ok(parent) = child_of.get(current) else {
                return None;
            };
            current = parent.parent();
        }
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn on_explorer_close_editor(
    trigger: On<UiInput<ExplorerCloseEditor>>,
    mut states: Query<&mut ExplorerState>,
    views: Query<&FileView>,
    child_of: Query<&ChildOf>,
    stacks: Query<(), With<vmux_layout::stack::Stack>>,
    mut closing: MessageWriter<vmux_layout::CloseStackRequest>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(mut state) = states.get_mut(entity) else {
        return;
    };
    let next = state.close_editor(&path);
    commands.entity(entity).insert(OpenEditorsDirty);
    let Some(next) = next else {
        if let Some(close) = EditorPageClose::holding(entity, &child_of, &stacks) {
            closing.write(vmux_layout::CloseStackRequest::by_user(close.0));
        }
        return;
    };
    let Ok(file_view) = views.get(entity) else {
        return;
    };
    if file_view.path != path {
        return;
    }
    commands.trigger(FileNavigateRequest::new(entity, next, 0));
}
