use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_core::event::{ExplorerCloseEditor, OpenEditorItem, OpenEditorsEvent};

use super::ExplorerState;
use crate::host::editing::{EditState, FileView, ParkedEdits};
use crate::host::viewport::FileViewport;

pub(in crate::host) struct ExplorerTabsPlugin;

impl Plugin for ExplorerTabsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<vmux_layout::CloseStackRequest>()
            .add_systems(Update, (sync_open_editors, emit_open_editors))
            .add_observer(on_explorer_close_editor);
    }
}

#[derive(Component)]
pub(in crate::host) struct OpenEditorsDirty;

type OpenEditorsDirtyReady = (With<OpenEditorsDirty>, With<vmux_core::page::PageReady>);
type NavigableFileView = (
    &'static mut FileView,
    &'static mut FileViewport,
    &'static mut PageMetadata,
);
type OpenEditorsView = (
    Entity,
    &'static FileView,
    &'static ExplorerState,
    Option<&'static EditState>,
    Option<&'static ParkedEdits>,
);

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

fn emit_open_editors(
    query: Query<OpenEditorsView, OpenEditorsDirtyReady>,
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
        commands.trigger(BinHostEmitEvent::from_event(
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

#[derive(bevy::ecs::system::SystemParam)]
struct EditorPageClose<'w, 's> {
    child_of: Query<'w, 's, &'static ChildOf>,
    stacks: Query<'w, 's, (), With<vmux_layout::stack::Stack>>,
    closing: MessageWriter<'w, vmux_layout::CloseStackRequest>,
}

impl EditorPageClose<'_, '_> {
    fn holding(&mut self, webview: Entity) {
        let mut current = webview;
        for _ in 0..8 {
            if self.stacks.contains(current) {
                self.closing
                    .write(vmux_layout::CloseStackRequest::by_user(current));
                return;
            }
            let Ok(parent) = self.child_of.get(current) else {
                return;
            };
            current = parent.parent();
        }
    }
}

fn on_explorer_close_editor(
    trigger: On<BinReceive<ExplorerCloseEditor>>,
    mut states: Query<&mut ExplorerState>,
    mut views: Query<NavigableFileView>,
    mut page: EditorPageClose,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
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
        page.holding(entity);
        return;
    };
    let Ok((mut file_view, mut viewport, mut metadata)) = views.get_mut(entity) else {
        return;
    };
    if file_view.path != path {
        return;
    }
    file_view.navigate(
        entity,
        next,
        0,
        &mut viewport,
        &mut metadata,
        &mut manager,
        &mut commands,
    );
}
