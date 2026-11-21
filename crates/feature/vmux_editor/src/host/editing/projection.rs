use super::document::Edited;
use super::*;
use bevy::prelude::{App, Commands, NonSend, NonSendMut, On, Plugin, Query};

pub(super) struct ProjectionPlugin;

impl Plugin for ProjectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(project);
    }
}

fn project(
    trigger: On<Edited>,
    mut views: Query<(Edit, &mut FileViewport, &mut vmux_git::GitDiffSource)>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let entity = event.entity;
    let Ok((mut edit, mut vp, mut diff_source)) = views.get_mut(entity) else {
        return;
    };
    if event.document != edit.core.revision.identity {
        return;
    }
    let text_changed = event.outcome.text_changed;
    let cursor_stale = event.cursor_changed;
    let dirty_changed = event.outcome.dirty_changed;
    let mut fold_changed = false;
    let top_before = vp.top_row;
    if text_changed {
        let line = edit.core.buffer.char_to_coords(edit.core.primary().head).0;
        edit.hl.invalidate_from(line.saturating_sub(1));
    }
    if let Some(value) = event.outcome.yank.as_ref()
        && let Some(cb) = clipboard.0.as_mut()
        && cb.set_text(value.text.clone()).is_ok()
    {
        edit.core.registers.clipboard_shadow.clone_from(&value.text);
    }
    if text_changed {
        let regions = FoldState::indent_regions(&edit.core.buffer.rope);
        edit.folds.set_regions(regions);
        edit.sync_fold_view();
    }
    {
        let total = edit.core.buffer.len_lines() as u32;
        let caret_line = edit.core.cursor_pos().line;
        if edit.folds.view(total).is_hidden(caret_line) {
            edit.folds.reveal(caret_line);
            edit.sync_fold_view();
            fold_changed = true;
        }
    }
    if let Some(top) = vp.autoscroll(&mut edit) {
        if let Some(scroll) = vp.set_top(top)
            && browsers.can_emit_to(&entity)
        {
            commands.trigger(vmux_ecs::FileUiStateWrite::from_event(entity, &scroll));
        }
        edit.core.viewport.top_row = vp.top_row;
    }
    let vpc = *vp;
    if text_changed || fold_changed || vpc.left_render_band(top_before) {
        commands.trigger(ViewportRenderRequest::new(entity));
    }
    if text_changed || cursor_stale || fold_changed {
        commands.trigger(CursorRenderRequest::new(entity));
        if ParsedNote::supports(&edit.core.buffer.path) {
            commands.entity(entity).remove::<NoteSent>();
        }
    }
    if fold_changed {
        commands.entity(entity).insert(FoldsDirty);
    }
    if dirty_changed {
        commands.entity(entity).insert(OpenEditorsDirty);
    }
    if text_changed || dirty_changed {
        diff_source.content = edit.core.buffer.text();
        diff_source.dirty = edit.core.revision.dirty;
        commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
            entity,
            &FileDirtyEvent {
                dirty: edit.core.revision.dirty,
            },
        ));
    }
    if text_changed {
        edit.refresh_parsed_note();
        let markdown = edit.is_note();
        let mut entity_commands = commands.entity(entity);
        entity_commands
            .insert(LspEditDirty)
            .remove::<crate::lsp::manager::LintRan>();
        if markdown {
            entity_commands.remove::<NoteSent>().insert(OutlineDirty);
        }
    }
}
