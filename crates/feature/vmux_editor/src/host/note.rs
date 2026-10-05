use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_ecs::event::{
    FileNoteEditDismissRequest, FileNoteEditRequest, FileNoteEvent,
    FileNotePropertiesToggleRequest, FilePropertyDraftRequest, FilePropertyDraftState,
    FilePropertyEdit, FileViewMode, MdBlock, NoteBlock,
};

use crate::host::editor::{Editor, FileView};
use crate::host::markdown::ParsedNote;
use crate::host::status::{FileInitialMetaSent, SharedFileViewMode};

pub(crate) struct NotePlugin;

impl Plugin for NotePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            FileNoteEditRequest,
            FileNoteEditDismissRequest,
            FilePropertyDraftRequest,
            FileNotePropertiesToggleRequest,
        )>::default())
            .add_observer(start_edit)
            .add_observer(dismiss_edit)
            .add_observer(edit_property)
            .add_observer(toggle_properties)
            .add_observer(commit_property)
            .add_systems(
                Update,
                (
                    clear_editing,
                    mark_notes_on_knowledge_change,
                    send.after(clear_editing)
                        .after(mark_notes_on_knowledge_change),
                ),
            );
    }
}

#[derive(Component)]
pub(crate) struct NoteSent;

#[derive(Component, Clone, Copy)]
pub(crate) struct NoteRevealLine(pub(crate) u32);

#[derive(Component)]
pub(crate) struct NoteEditing;

#[derive(Component, Clone, Default)]
struct NotePresentation {
    properties_open: bool,
    drafts: BTreeMap<String, FilePropertyDraftState>,
}

type ReadyNote = (
    Without<NoteSent>,
    With<vmux_ecs::page::PageReady>,
    With<FileInitialMetaSent>,
);

type ReadyNotes<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static FileView,
        &'static Editor,
        Option<&'static NoteRevealLine>,
        Option<&'static NoteEditing>,
        Option<&'static NotePresentation>,
    ),
    ReadyNote,
>;

fn active_note_block(blocks: &[NoteBlock], line: u32) -> Option<u32> {
    blocks
        .iter()
        .position(|block| block.start_line <= line && line < block.end_line)
        .or_else(|| blocks.iter().rposition(|block| block.start_line <= line))
        .or_else(|| (!blocks.is_empty()).then_some(0))
        .map(|index| index as u32)
}

fn send(
    mode: Single<&SharedFileViewMode>,
    indexes: Query<&vmux_knowledge::KnowledgeIndex>,
    notes: ReadyNotes,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    if mode.0 != FileViewMode::Note {
        return;
    }
    let index = indexes.single().ok();
    for (entity, file, edit, reveal, editing, presentation) in &notes {
        if !ParsedNote::supports(&file.path) {
            commands.entity(entity).insert(NoteSent);
            continue;
        }
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let Some(mut note) = edit.parsed_note() else {
            commands.entity(entity).insert(NoteSent);
            continue;
        };
        let missing_presentation = presentation.is_none();
        let presentation = presentation.cloned().unwrap_or_else(|| NotePresentation {
            properties_open: !note.properties.is_empty(),
            drafts: BTreeMap::new(),
        });
        if missing_presentation {
            commands.entity(entity).insert(presentation.clone());
        }
        let references = index
            .filter(|index| index.loaded() && file.path.starts_with(index.root()))
            .map(|index| {
                index.resolve_blocks(&file.path, &mut note.blocks);
                let mut references = index.backlinks(&file.path);
                references.extend(index.unlinked_mentions(&file.path, 32));
                references
                    .into_iter()
                    .map(|reference| vmux_knowledge::KnowledgeReference {
                        title: reference.title,
                        path: reference.path.to_string_lossy().into_owned(),
                        line: reference.line,
                        preview: reference.preview,
                        unlinked: reference.unlinked,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let active = active_note_block(&note.blocks, edit.cursor_line());
        let edit_line = editing
            .is_some()
            .then_some(())
            .and(active)
            .and_then(|index| note.blocks.get(index as usize))
            .filter(|block| matches!(block.block, MdBlock::List { .. }))
            .map(|_| edit.cursor_line());
        commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
            entity,
            &FileNoteEvent {
                title: note.title,
                properties: note.properties,
                properties_open: presentation.properties_open,
                property_drafts: presentation.drafts.values().cloned().collect(),
                blocks: note.blocks,
                active,
                editing: editing.is_some(),
                edit_line,
                references,
                reveal_line: reveal.map(|line| line.0),
            },
        ));
        commands
            .entity(entity)
            .insert(NoteSent)
            .remove::<NoteRevealLine>();
    }
}

fn start_edit(
    trigger: On<UiInput<FileNoteEditRequest>>,
    files: Query<(), With<FileView>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if !files.contains(entity) {
        return;
    }
    commands
        .entity(entity)
        .insert(NoteEditing)
        .remove::<NoteSent>();
}

fn dismiss_edit(trigger: On<UiInput<FileNoteEditDismissRequest>>, mut commands: Commands) {
    commands
        .entity(trigger.event().webview)
        .remove::<NoteEditing>()
        .remove::<NoteSent>();
}

fn edit_property(
    trigger: On<UiInput<FilePropertyDraftRequest>>,
    mut presentations: Query<&mut NotePresentation>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(mut presentation) = presentations.get_mut(entity) else {
        return;
    };
    let draft = trigger.event().payload.draft.clone();
    presentation
        .drafts
        .insert(draft.original_key.clone(), draft);
    commands.entity(entity).remove::<NoteSent>();
}

fn toggle_properties(
    trigger: On<UiInput<FileNotePropertiesToggleRequest>>,
    mut presentations: Query<&mut NotePresentation>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(mut presentation) = presentations.get_mut(entity) else {
        return;
    };
    presentation.properties_open = !presentation.properties_open;
    commands.entity(entity).remove::<NoteSent>();
}

fn commit_property(
    trigger: On<UiInput<FilePropertyEdit>>,
    mut presentations: Query<&mut NotePresentation>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(mut presentation) = presentations.get_mut(entity) else {
        return;
    };
    presentation
        .drafts
        .remove(&trigger.event().payload.original_key);
    commands.entity(entity).remove::<NoteSent>();
}

fn clear_editing(files: Query<Entity, Changed<FileView>>, mut commands: Commands) {
    for entity in &files {
        commands.entity(entity).remove::<NoteEditing>();
    }
}

fn mark_notes_on_knowledge_change(
    indexes: Query<Ref<vmux_knowledge::KnowledgeIndex>>,
    files: Query<Entity, With<FileView>>,
    mut commands: Commands,
) {
    let Ok(index) = indexes.single() else {
        return;
    };
    if !index.is_changed() {
        return;
    }
    for entity in &files {
        commands.entity(entity).remove::<NoteSent>();
    }
}
