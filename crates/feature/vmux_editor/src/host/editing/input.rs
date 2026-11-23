use super::document::*;
use super::effects::*;
use super::{FileFindOpenRequest, PreviewAccess};
use crate::edit::{EditCommand, EditCore, EditMode, Motion};
use crate::host::editor::Editor;
use crate::host::keymap::EditorKeymap;
use crate::host::language::{
    EditorCompletionRequest, EditorDefinitionRequest, EditorHoverRequest, EditorReferencesRequest,
    EditorRenameRequest,
};
use crate::host::markdown::ParsedNote;
use crate::host::status::SharedFileViewMode;
use crate::keymap::{KeyInput, Keymap as _};
use bevy::prelude::*;
use bevy::prelude::{App, Commands, Entity, EntityEvent, On, Plugin, Query};
use vmux_api::editor::KeymapKind;
use vmux_command::CommandInvocation;
use vmux_ecs::event::{FileViewMode, NoteBlock};

pub(crate) struct InputPlugin;

#[cfg(not(test))]
impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(decode::<KeyInputRequest>)
            .add_observer(decode::<ExInputRequest>);
    }
}

#[cfg(test)]
impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(decode::<KeyInputRequest>)
            .add_observer(decode::<ExInputRequest>)
            .add_observer(decode_fixture);
    }
}

#[cfg(test)]
fn decode_fixture(
    trigger: On<ParsedFixtureRequest>,
    documents: Query<(
        EditCore,
        Option<&Editor>,
        Option<&EditorKeymap>,
        &mut crate::keymap::vim::VimKeymap,
    )>,
    modes: Query<&SharedFileViewMode>,
    preview: PreviewAccess,
    commands: Commands,
) {
    decode(trigger, documents, modes, preview, commands);
}

#[derive(EntityEvent)]
pub(super) struct KeyInputRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: uuid::Uuid,
    pub(super) input: KeyInput,
}

impl SyntaxInput for KeyInputRequest {
    const KEY: bool = true;

    fn document(&self) -> uuid::Uuid {
        self.document
    }

    fn parsed(
        &self,
        mode: EditMode,
        kind: KeymapKind,
        parser: &mut crate::keymap::vim::VimKeymap,
    ) -> (Vec<EditCommand>, EditMode) {
        match kind {
            KeymapKind::Vscode => (
                crate::keymap::vscode::VscodeKeymap.handle(&self.input),
                mode,
            ),
            KeymapKind::Vim => parser.decode(mode, &self.input),
        }
    }
    fn accelerated(&self, decoded: Vec<EditCommand>) -> Vec<EditCommand> {
        if !self.input.repeat {
            return decoded;
        }
        let mut commands = Vec::with_capacity(decoded.len() * 2);
        for command in decoded {
            if let EditCommand::ScrollViewport(lines) = command {
                commands.push(EditCommand::ScrollViewport(lines.saturating_mul(2)));
                continue;
            }
            let accelerate = matches!(
                &command,
                EditCommand::Move(
                    Motion::Left
                        | Motion::Right
                        | Motion::LeftBounded
                        | Motion::RightBounded
                        | Motion::Up
                        | Motion::Down,
                ) | EditCommand::Select(
                    Motion::Left
                        | Motion::Right
                        | Motion::LeftBounded
                        | Motion::RightBounded
                        | Motion::Up
                        | Motion::Down,
                )
            );
            if accelerate {
                commands.push(command.clone());
            }
            commands.push(command);
        }
        commands
    }

    fn remapped(
        &self,
        decoded: Vec<EditCommand>,
        blocks: &[NoteBlock],
        start_line: u32,
    ) -> Vec<EditCommand> {
        let mut line = start_line;
        let mut commands = Vec::new();
        for command in decoded {
            let (direction, select) = match &command {
                EditCommand::Move(Motion::Down) => (1, false),
                EditCommand::Move(Motion::Up) => (-1, false),
                EditCommand::Select(Motion::Down) => (1, true),
                EditCommand::Select(Motion::Up) => (-1, true),
                _ => {
                    commands.push(command);
                    continue;
                }
            };
            let Some(target) = ParsedNote::vertical_target(blocks, line, direction) else {
                line = if direction > 0 {
                    line.saturating_add(1)
                } else {
                    line.saturating_sub(1)
                };
                commands.push(command);
                continue;
            };
            if target == line {
                continue;
            }
            let steps = target.abs_diff(line) as usize;
            line = target;
            let motion = if direction > 0 {
                Motion::Down
            } else {
                Motion::Up
            };
            let command = if select {
                EditCommand::Select(motion)
            } else {
                EditCommand::Move(motion)
            };
            commands.extend(std::iter::repeat_n(command, steps));
        }
        commands
    }
}

#[derive(EntityEvent)]
pub(super) struct ExInputRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: uuid::Uuid,
    pub(super) line: String,
}

impl SyntaxInput for ExInputRequest {
    const KEY: bool = false;

    fn document(&self) -> uuid::Uuid {
        self.document
    }

    fn parsed(
        &self,
        mode: EditMode,
        _: KeymapKind,
        _: &mut crate::keymap::vim::VimKeymap,
    ) -> (Vec<EditCommand>, EditMode) {
        (
            crate::edit::ex::ExCommand::edits_from_line(&self.line),
            mode,
        )
    }
}

pub(super) trait SyntaxInput: EntityEvent {
    const KEY: bool;
    fn document(&self) -> uuid::Uuid;
    fn accelerated(&self, commands: Vec<EditCommand>) -> Vec<EditCommand> {
        commands
    }
    fn remapped(&self, commands: Vec<EditCommand>, _: &[NoteBlock], _: u32) -> Vec<EditCommand> {
        commands
    }
    fn parsed(
        &self,
        mode: EditMode,
        kind: KeymapKind,
        parser: &mut crate::keymap::vim::VimKeymap,
    ) -> (Vec<EditCommand>, EditMode);
}

fn decode<T: SyntaxInput>(
    trigger: On<T>,
    mut documents: Query<(
        EditCore,
        Option<&Editor>,
        Option<&EditorKeymap>,
        &mut crate::keymap::vim::VimKeymap,
    )>,
    modes: Query<&SharedFileViewMode>,
    preview: PreviewAccess,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let input = trigger.event();
    let Ok((core, cache, keymap, mut parser)) = documents.get_mut(entity) else {
        return;
    };
    if core.revision.identity != input.document() {
        return;
    }
    let kind = keymap.map_or(KeymapKind::Vim, |keymap| keymap.0);
    let read_only = preview.read_only(entity, &core.buffer.path);
    let mut candidate = if T::KEY && read_only && kind == KeymapKind::Vim {
        Some(parser.clone())
    } else {
        None
    };
    let (decoded, mode) = match candidate.as_mut() {
        Some(candidate) => input.parsed(core.mode.value, kind, candidate),
        None => input.parsed(core.mode.value, kind, &mut parser),
    };
    let mut decoded = input.accelerated(decoded);
    if T::KEY
        && modes
            .single()
            .is_ok_and(|mode| mode.0 == FileViewMode::Note)
        && let Some(blocks) = cache.and_then(Editor::note_blocks)
    {
        decoded = input.remapped(decoded, blocks, core.cursor_pos().line);
    }
    if T::KEY && read_only && decoded.iter().any(|command| !command.allowed_in_preview()) {
        parser.reject(core.mode.value);
        return;
    }
    if let Some(candidate) = candidate {
        *parser = candidate;
    }
    if mode != core.mode.value
        && !decoded
            .iter()
            .any(|command| matches!(command, EditCommand::SetMode(value) if *value == mode))
    {
        decoded.push(EditCommand::SetMode(mode));
    }
    for command in decoded {
        if let Some(effect) =
            emit_document(command, entity, Some(core.revision.identity), &mut commands)
        {
            emit_effect(effect, entity, Some(core.revision.identity), &mut commands);
        }
    }
}

fn emit_document(
    command: EditCommand,
    entity: Entity,
    document: Option<uuid::Uuid>,
    commands: &mut Commands,
) -> Option<EditCommand> {
    let command = encode_text(command, entity, document, commands)?;
    let command = encode_selection(command, entity, document, commands)?;
    let command = encode_history(command, entity, document, commands)?;
    encode_operator(command, entity, document, commands)
}

fn encode_text(
    command: EditCommand,
    entity: Entity,
    document: Option<uuid::Uuid>,
    commands: &mut Commands,
) -> Option<EditCommand> {
    match command {
        EditCommand::InsertText(t) => {
            commands.trigger(InsertTextRequest {
                entity,
                document,
                text: t,
            });
        }
        EditCommand::OvertypeText(t) => {
            commands.trigger(OvertypeTextRequest {
                entity,
                document,
                text: t,
            });
        }
        EditCommand::ReplaceText(t) => {
            commands.trigger(ReplaceTextRequest {
                entity,
                document,
                text: t,
            });
        }
        EditCommand::InsertNewline => {
            commands.trigger(InsertNewlineRequest { entity, document });
        }
        EditCommand::InsertTab => {
            commands.trigger(InsertTabRequest { entity, document });
        }
        EditCommand::DeleteBack => {
            commands.trigger(DeleteBackRequest { entity, document });
        }
        EditCommand::DeleteForward => {
            commands.trigger(DeleteForwardRequest { entity, document });
        }
        EditCommand::DeleteWordBack => {
            commands.trigger(DeleteWordBackRequest { entity, document });
        }
        EditCommand::ReplaceChar { ch, count } => {
            commands.trigger(ReplaceCharRequest {
                entity,
                document,
                ch,
                count,
            });
        }
        effect => return Some(effect),
    }
    None
}

fn encode_selection(
    command: EditCommand,
    entity: Entity,
    document: Option<uuid::Uuid>,
    commands: &mut Commands,
) -> Option<EditCommand> {
    match command {
        EditCommand::Move(m) => {
            commands.trigger(MoveRequest {
                entity,
                document,
                motion: m,
            });
        }
        EditCommand::Select(m) => {
            commands.trigger(SelectRequest {
                entity,
                document,
                motion: m,
            });
        }
        EditCommand::SetMode(m) => {
            commands.trigger(ModeRequest {
                entity,
                document,
                mode: m,
            });
        }
        EditCommand::BeginBlockInsert { after } => {
            commands.trigger(BeginBlockInsertRequest {
                entity,
                document,
                after,
            });
        }
        EditCommand::FinishBlockInsert { text } => {
            commands.trigger(FinishBlockInsertRequest {
                entity,
                document,
                text,
            });
        }
        EditCommand::SwapSelectionEnds => {
            commands.trigger(SwapSelectionRequest { entity, document });
        }
        EditCommand::SelectTextObject(obj) => {
            commands.trigger(SelectObjectRequest {
                entity,
                document,
                object: obj,
            });
        }
        EditCommand::SelectAllOccurrences => {
            commands.trigger(SelectOccurrencesRequest { entity, document });
        }
        EditCommand::CollapseCarets => {
            commands.trigger(CollapseCaretsRequest { entity, document });
        }
        EditCommand::AddCaretVertically(direction) => {
            commands.trigger(AddCaretRequest {
                entity,
                document,
                direction,
            });
        }
        effect => return Some(effect),
    }
    None
}

fn encode_history(
    command: EditCommand,
    entity: Entity,
    document: Option<uuid::Uuid>,
    commands: &mut Commands,
) -> Option<EditCommand> {
    match command {
        EditCommand::Undo => {
            commands.trigger(UndoRequest { entity, document });
        }
        EditCommand::Redo => {
            commands.trigger(RedoRequest { entity, document });
        }
        EditCommand::UndoTime { forward, count } => {
            commands.trigger(UndoTimeRequest {
                entity,
                document,
                forward,
                count,
            });
        }
        EditCommand::SetSearch { pattern, forward } => {
            commands.trigger(SearchRequest {
                entity,
                document,
                pattern,
                forward,
            });
        }
        EditCommand::SearchWord { forward } => {
            commands.trigger(SearchWordRequest {
                entity,
                document,
                forward,
            });
        }
        EditCommand::ClearSearchHighlight => {
            commands.trigger(ClearSearchHighlightRequest { entity, document });
        }
        EditCommand::SetMark(name) => {
            commands.trigger(MarkRequest {
                entity,
                document,
                name,
            });
        }
        EditCommand::GotoMark { name, linewise } => {
            commands.trigger(GotoMarkRequest {
                entity,
                document,
                name,
                linewise,
            });
        }
        EditCommand::JumpList { back, count } => {
            commands.trigger(JumpRequest {
                entity,
                document,
                back,
                count,
            });
        }
        EditCommand::ChangeList { back, count } => {
            commands.trigger(ChangeRequest {
                entity,
                document,
                back,
                count,
            });
        }
        effect => return Some(effect),
    }
    None
}

fn encode_operator(
    command: EditCommand,
    entity: Entity,
    document: Option<uuid::Uuid>,
    commands: &mut Commands,
) -> Option<EditCommand> {
    match command {
        EditCommand::Op {
            operator,
            target,
            register,
        } => {
            commands.trigger(OperatorRequest {
                entity,
                document,
                operator,
                target,
                register,
            });
        }
        EditCommand::Put {
            before,
            count,
            register,
        } => {
            commands.trigger(PutRequest {
                entity,
                document,
                before,
                count,
                register,
            });
        }
        EditCommand::JoinLines { count, spaces } => {
            commands.trigger(JoinLinesRequest {
                entity,
                document,
                count,
                spaces,
            });
        }
        EditCommand::OpenLine { above } => {
            commands.trigger(OpenLineRequest {
                entity,
                document,
                above,
            });
        }
        EditCommand::Substitute {
            range,
            pattern,
            replacement,
            all,
        } => {
            commands.trigger(SubstituteRequest {
                entity,
                document,
                range,
                pattern,
                replacement,
                all,
            });
        }
        EditCommand::ExDelete(range) => {
            commands.trigger(DeleteLinesRequest {
                entity,
                document,
                range,
            });
        }
        EditCommand::ExYank(range) => {
            commands.trigger(YankLinesRequest {
                entity,
                document,
                range,
            });
        }
        EditCommand::Reshape(shape) => {
            commands.trigger(ReshapeRequest {
                entity,
                document,
                shape,
            });
        }
        EditCommand::Increment(delta) => {
            commands.trigger(IncrementRequest {
                entity,
                document,
                delta,
            });
        }
        effect => return Some(effect),
    }
    None
}

fn emit_effect(
    command: EditCommand,
    entity: Entity,
    document: Option<uuid::Uuid>,
    commands: &mut Commands,
) {
    match command {
        EditCommand::Save => {
            commands.trigger(SaveRequest { entity, document });
        }
        EditCommand::Paste => {
            commands.trigger(PasteRequest { entity, document });
        }
        EditCommand::ScrollViewport(lines) => {
            commands.trigger(ScrollRequest {
                entity,
                document,
                lines,
            });
        }
        EditCommand::ScrollCursorTo(placement) => {
            commands.trigger(ScrollCursorRequest {
                entity,
                document,
                placement,
            });
        }
        EditCommand::FoldToggle => {
            commands.trigger(FoldToggleRequest { entity, document });
        }
        EditCommand::FoldOpen => {
            commands.trigger(FoldOpenRequest { entity, document });
        }
        EditCommand::FoldClose => {
            commands.trigger(FoldCloseRequest { entity, document });
        }
        EditCommand::FoldToggleRecursive => {
            commands.trigger(FoldToggleRecursiveRequest { entity, document });
        }
        EditCommand::FoldAll => {
            commands.trigger(FoldAllRequest { entity, document });
        }
        EditCommand::UnfoldAll => {
            commands.trigger(UnfoldAllRequest { entity, document });
        }
        EditCommand::Hover => {
            commands.trigger(EditorHoverRequest::from(entity));
        }
        EditCommand::GotoDefinition => {
            commands.trigger(EditorDefinitionRequest::from(entity));
        }
        EditCommand::FindReferences => {
            commands.trigger(EditorReferencesRequest::from(entity));
        }
        EditCommand::BeginRename => {
            commands.trigger(EditorRenameRequest::from(entity));
        }
        EditCommand::TriggerCompletion => {
            commands.trigger(EditorCompletionRequest::from(entity));
        }
        EditCommand::OpenFind { forward } => {
            commands.trigger(FileFindOpenRequest::new(entity, forward));
        }
        EditCommand::OpenCommandLine => {
            commands.write_message(CommandInvocation::new(entity, "browser_open_ex_bar"));
        }
        _ => {}
    }
}

#[cfg(test)]
#[derive(EntityEvent)]
pub(super) struct ParsedFixtureRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: uuid::Uuid,
    pub(super) commands: Vec<EditCommand>,
}

#[cfg(test)]
impl SyntaxInput for ParsedFixtureRequest {
    const KEY: bool = false;

    fn document(&self) -> uuid::Uuid {
        self.document
    }

    fn parsed(
        &self,
        mode: EditMode,
        _: KeymapKind,
        _: &mut crate::keymap::vim::VimKeymap,
    ) -> (Vec<EditCommand>, EditMode) {
        (self.commands.clone(), mode)
    }
}
