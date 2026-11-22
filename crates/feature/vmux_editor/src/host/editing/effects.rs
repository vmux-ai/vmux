use super::document::Edited;
use super::*;
use bevy::prelude::{App, Commands, Entity, EntityEvent, On, Plugin, Query};

pub(super) struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(save)
            .add_observer(paste)
            .add_observer(scroll)
            .add_observer(scroll_cursor)
            .add_observer(toggle)
            .add_observer(open)
            .add_observer(close)
            .add_observer(toggle_recursive)
            .add_observer(fold_all)
            .add_observer(unfold_all);
    }
}

#[derive(EntityEvent)]
pub(crate) struct SaveRequest {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) document: Option<uuid::Uuid>,
}

#[derive(EntityEvent)]
pub(crate) struct PasteRequest {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) document: Option<uuid::Uuid>,
}

#[derive(EntityEvent)]
pub(super) struct ScrollRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) lines: i32,
}

#[derive(EntityEvent)]
pub(super) struct ScrollCursorRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) placement: crate::edit::command::ScrollPlacement,
}

#[derive(EntityEvent)]
pub(super) struct FoldToggleRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

#[derive(EntityEvent)]
pub(super) struct FoldOpenRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

#[derive(EntityEvent)]
pub(super) struct FoldCloseRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

#[derive(EntityEvent)]
pub(super) struct FoldToggleRecursiveRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

#[derive(EntityEvent)]
pub(super) struct FoldAllRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

#[derive(EntityEvent)]
pub(super) struct UnfoldAllRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

fn save(
    trigger: On<SaveRequest>,
    mut views: Query<Edit>,
    preview: PreviewAccess,
    mut self_writes: NonSendMut<SelfWrites>,
    browsers: crate::host::delivery::Surface,
    mut commands: Commands,
) {
    let event = trigger.event();
    let entity = event.entity;
    let Ok(mut edit) = views.get_mut(entity) else {
        return;
    };
    if event
        .document
        .is_some_and(|id| id != edit.core.revision.identity)
        || preview.previews.contains(entity)
        || preview
            .checkout
            .preview(entity, &edit.core.buffer.path)
            .is_some()
    {
        return;
    }
    let path = edit.core.buffer.path.clone();
    let body = edit.core.buffer.text();
    let encoding = edit.core.buffer.encoding;
    let bytes = match (crate::encoding::Reencode { encoding }).applied(&body) {
        Ok(bytes) => bytes,
        Err(unmappable) => {
            tracing::warn!(path = %path.display(), "editor save refused: {unmappable}");
            if browsers.can_emit_to(&entity) {
                commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
                    entity,
                    &FileErrorEvent {
                        message: format!("save failed: {unmappable}"),
                        undecodable: false,
                    },
                ));
            }
            return;
        }
    };
    match vmux_path::AtomicFile::write(&path, &bytes) {
        Ok(()) => {
            self_writes.0.insert(
                vmux_path::PathIdentity::resolve(&path).into_path_buf(),
                std::time::Instant::now(),
            );
            let was_dirty = edit.core.revision.dirty;
            edit.core.mark_saved();
            if was_dirty {
                commands.trigger(Edited {
                    cursor_changed: false,
                    entity,
                    document: edit.core.revision.identity,
                    outcome: crate::edit::EditOutcome {
                        dirty_changed: true,
                        ..Default::default()
                    },
                });
            }
            commands
                .entity(entity)
                .insert(LspEditDirty)
                .remove::<crate::lsp::manager::LintRan>();
        }
        Err(e) => {
            tracing::warn!(path = %path.display(), "editor save failed: {e}");
            if browsers.can_emit_to(&entity) {
                commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
                    entity,
                    &FileErrorEvent {
                        message: format!("save failed: {e}"),
                        undecodable: false,
                    },
                ));
            }
        }
    }
}

fn paste(
    trigger: On<PasteRequest>,
    documents: Query<&crate::edit::document::Revision>,
    mut clipboard: crate::host::delivery::Clipboard,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok(revision) = documents.get(event.entity) else {
        return;
    };
    if event.document.is_some_and(|id| id != revision.identity) {
        return;
    }
    let Some(text) = clipboard.read(event.entity) else {
        return;
    };
    commands.trigger(super::document::PasteTextRequest {
        entity: event.entity,
        document: Some(revision.identity),
        text,
    });
}

fn scroll(
    trigger: On<ScrollRequest>,
    mut views: Query<(Edit, &mut FileViewport)>,
    browsers: crate::host::delivery::Surface,
    mut commands: Commands,
) {
    let event = trigger.event();
    let entity = event.entity;
    let Ok((mut edit, mut vp)) = views.get_mut(entity) else {
        return;
    };
    if event
        .document
        .is_some_and(|id| id != edit.core.revision.identity)
    {
        return;
    }
    let visible = vp.visible_rows(&mut edit);
    let target = (vp.top_row as i64 + event.lines as i64).clamp(0, u32::MAX as i64) as u32;
    let target = ScrollWindow::new(visible, target, vp.rows).top();
    if let Some(scroll) = vp.set_top(target)
        && browsers.can_emit_to(&entity)
    {
        commands.trigger(vmux_ecs::FileUiStateWrite::from_event(entity, &scroll));
    }
    edit.core.viewport.top_row = vp.top_row;
    if vp.follow_scrolled_cursor(&mut edit) {
        commands.trigger(CursorRenderRequest::new(entity));
    }
    commands.trigger(ViewportRenderRequest::new(entity));
}

fn scroll_cursor(
    trigger: On<ScrollCursorRequest>,
    mut views: Query<(Edit, &mut FileViewport)>,
    browsers: crate::host::delivery::Surface,
    mut commands: Commands,
) {
    let event = trigger.event();
    let entity = event.entity;
    let Ok((mut edit, mut vp)) = views.get_mut(entity) else {
        return;
    };
    if event
        .document
        .is_some_and(|id| id != edit.core.revision.identity)
    {
        return;
    }
    let row = edit
        .folds
        .view(edit.core.buffer.len_lines() as u32)
        .buffer_to_row(edit.core.cursor_pos().line);
    let rows = vp.rows.max(1) as u32;
    let target = match event.placement {
        crate::edit::command::ScrollPlacement::Top => row,
        crate::edit::command::ScrollPlacement::Center => row.saturating_sub(rows / 2),
        crate::edit::command::ScrollPlacement::Bottom => row.saturating_sub(rows - 1),
    };
    if let Some(scroll) = vp.set_top(target)
        && browsers.can_emit_to(&entity)
    {
        commands.trigger(vmux_ecs::FileUiStateWrite::from_event(entity, &scroll));
    }
    edit.core.viewport.top_row = vp.top_row;
    commands.trigger(ViewportRenderRequest::new(entity));
}

fn toggle(trigger: On<FoldToggleRequest>, mut views: Query<Edit>, mut commands: Commands) {
    let event = trigger.event();
    let entity = event.entity;
    let Ok(mut edit) = views.get_mut(entity) else {
        return;
    };
    if event
        .document
        .is_some_and(|id| id != edit.core.revision.identity)
    {
        return;
    }
    let line = edit.core.cursor_pos().line;
    edit.folds.toggle(line);
    edit.sync_fold_view();
    if let Some(header) = edit.folds.hiding_header(line) {
        let at = edit.core.buffer.line_to_char(header as usize);
        edit.core.set_caret(at);
    }
    commands.entity(entity).insert(FoldsDirty);
    commands.trigger(ViewportRenderRequest::new(entity));
    commands.trigger(CursorRenderRequest::new(entity));
}

fn open(trigger: On<FoldOpenRequest>, mut views: Query<Edit>, mut commands: Commands) {
    let event = trigger.event();
    let entity = event.entity;
    let Ok(mut edit) = views.get_mut(entity) else {
        return;
    };
    if event
        .document
        .is_some_and(|id| id != edit.core.revision.identity)
    {
        return;
    }
    let line = edit.core.cursor_pos().line;
    edit.folds.open(line);
    edit.sync_fold_view();
    if let Some(header) = edit.folds.hiding_header(line) {
        let at = edit.core.buffer.line_to_char(header as usize);
        edit.core.set_caret(at);
    }
    commands.entity(entity).insert(FoldsDirty);
    commands.trigger(ViewportRenderRequest::new(entity));
    commands.trigger(CursorRenderRequest::new(entity));
}

fn close(trigger: On<FoldCloseRequest>, mut views: Query<Edit>, mut commands: Commands) {
    let event = trigger.event();
    let entity = event.entity;
    let Ok(mut edit) = views.get_mut(entity) else {
        return;
    };
    if event
        .document
        .is_some_and(|id| id != edit.core.revision.identity)
    {
        return;
    }
    let line = edit.core.cursor_pos().line;
    edit.folds.close(line);
    edit.sync_fold_view();
    if let Some(header) = edit.folds.hiding_header(line) {
        let at = edit.core.buffer.line_to_char(header as usize);
        edit.core.set_caret(at);
    }
    commands.entity(entity).insert(FoldsDirty);
    commands.trigger(ViewportRenderRequest::new(entity));
    commands.trigger(CursorRenderRequest::new(entity));
}

fn toggle_recursive(
    trigger: On<FoldToggleRecursiveRequest>,
    mut views: Query<Edit>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let entity = event.entity;
    let Ok(mut edit) = views.get_mut(entity) else {
        return;
    };
    if event
        .document
        .is_some_and(|id| id != edit.core.revision.identity)
    {
        return;
    }
    let line = edit.core.cursor_pos().line;
    edit.folds.toggle_recursive(line);
    edit.sync_fold_view();
    if let Some(header) = edit.folds.hiding_header(line) {
        let at = edit.core.buffer.line_to_char(header as usize);
        edit.core.set_caret(at);
    }
    commands.entity(entity).insert(FoldsDirty);
    commands.trigger(ViewportRenderRequest::new(entity));
    commands.trigger(CursorRenderRequest::new(entity));
}

fn fold_all(trigger: On<FoldAllRequest>, mut views: Query<Edit>, mut commands: Commands) {
    let event = trigger.event();
    let entity = event.entity;
    let Ok(mut edit) = views.get_mut(entity) else {
        return;
    };
    if event
        .document
        .is_some_and(|id| id != edit.core.revision.identity)
    {
        return;
    }
    let line = edit.core.cursor_pos().line;
    edit.folds.fold_all();
    edit.sync_fold_view();
    if let Some(header) = edit.folds.hiding_header(line) {
        let at = edit.core.buffer.line_to_char(header as usize);
        edit.core.set_caret(at);
    }
    commands.entity(entity).insert(FoldsDirty);
    commands.trigger(ViewportRenderRequest::new(entity));
    commands.trigger(CursorRenderRequest::new(entity));
}

fn unfold_all(trigger: On<UnfoldAllRequest>, mut views: Query<Edit>, mut commands: Commands) {
    let event = trigger.event();
    let entity = event.entity;
    let Ok(mut edit) = views.get_mut(entity) else {
        return;
    };
    if event
        .document
        .is_some_and(|id| id != edit.core.revision.identity)
    {
        return;
    }
    let line = edit.core.cursor_pos().line;
    edit.folds.unfold_all();
    edit.sync_fold_view();
    if let Some(header) = edit.folds.hiding_header(line) {
        let at = edit.core.buffer.line_to_char(header as usize);
        edit.core.set_caret(at);
    }
    commands.entity(entity).insert(FoldsDirty);
    commands.trigger(ViewportRenderRequest::new(entity));
    commands.trigger(CursorRenderRequest::new(entity));
}
