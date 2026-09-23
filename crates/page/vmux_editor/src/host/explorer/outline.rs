use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::event::OutlineEvent;

use super::OutlineDirty;
use crate::host::editor::{Editor, FileView};

pub(super) struct OutlinePlugin;

impl Plugin for OutlinePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (emit_markdown_outline, clear_on_file_change));
    }
}

type DirtyOutline = (With<OutlineDirty>, With<vmux_core::page::PageReady>);

fn emit_markdown_outline(
    query: Query<(Entity, &Editor), DirtyOutline>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, edit) in &query {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let items = crate::explorer_model::markdown_outline(&edit.core.buffer.text());
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
            entity,
            &OutlineEvent { items },
        ));
        commands.entity(entity).remove::<OutlineDirty>();
    }
}

fn clear_on_file_change(
    query: Query<Entity, (With<FileView>, Changed<FileView>)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for entity in &query {
        if browsers.can_emit_to(&entity) {
            commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                entity,
                &OutlineEvent { items: Vec::new() },
            ));
        }
    }
}
