use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::event::{FileNoteEvent, FileViewMode, NoteBlock};

use crate::host::editor::{Editor, FileView};
use crate::host::status::{FileInitialMetaSent, SharedFileViewMode};

pub(crate) struct NotePlugin;

impl Plugin for NotePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                mark_notes_on_knowledge_change,
                send_note.after(mark_notes_on_knowledge_change),
            ),
        );
    }
}

#[derive(Component)]
pub(crate) struct NoteSent;

#[derive(Component, Clone, Copy)]
pub(crate) struct NoteRevealLine(pub(crate) u32);

type ReadyNote = (
    Without<NoteSent>,
    With<vmux_core::page::PageReady>,
    With<FileInitialMetaSent>,
);

fn active_note_block(blocks: &[NoteBlock], line: u32) -> Option<u32> {
    blocks
        .iter()
        .position(|block| block.start_line <= line && line < block.end_line)
        .or_else(|| blocks.iter().rposition(|block| block.start_line <= line))
        .or_else(|| (!blocks.is_empty()).then_some(0))
        .map(|index| index as u32)
}

fn send_note(
    mode: Res<SharedFileViewMode>,
    index: Option<Res<vmux_core::knowledge::KnowledgeIndex>>,
    notes: Query<(Entity, &FileView, &Editor, Option<&NoteRevealLine>), ReadyNote>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    if mode.0 != FileViewMode::Note {
        return;
    }
    for (entity, file, edit, reveal) in &notes {
        if !crate::markdown::is_markdown_path(&file.path) {
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
        let references = index
            .as_deref()
            .filter(|index| index.loaded() && file.path.starts_with(index.root()))
            .map(|index| {
                index.resolve_blocks(&file.path, &mut note.blocks);
                let mut references = index.backlinks(&file.path);
                references.extend(index.unlinked_mentions(&file.path, 32));
                references
                    .into_iter()
                    .map(|reference| vmux_core::knowledge::KnowledgeReference {
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
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
            entity,
            &FileNoteEvent {
                title: note.title,
                properties: note.properties,
                blocks: note.blocks,
                active,
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

fn mark_notes_on_knowledge_change(
    index: Option<Res<vmux_core::knowledge::KnowledgeIndex>>,
    files: Query<Entity, With<FileView>>,
    mut commands: Commands,
) {
    if index.is_none_or(|index| !index.is_changed()) {
        return;
    }
    for entity in &files {
        commands.entity(entity).remove::<NoteSent>();
    }
}
