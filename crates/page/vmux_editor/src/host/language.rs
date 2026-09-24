use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::event::*;

use crate::edit::{EditCommand, Selection};
use crate::host::editing::EditRequest;
use crate::host::editor::{Editor, FileView};
use crate::page_model::DisplayCells;

pub(super) struct LanguagePlugin;

impl Plugin for LanguagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            FileHoverRequest,
            FileDefinitionRequest,
            FileReferencesRequest,
            FileRenameRequest,
            FileEditorAction,
            FileCodeActionPick,
            FileCompletionRequest,
            FileGotoRequest,
            FileCompletionCommit,
        )>::default())
            .add_observer(on_editor_language_request)
            .add_observer(on_file_hover_request)
            .add_observer(on_file_definition_request)
            .add_observer(on_file_references_request)
            .add_observer(on_file_rename_request)
            .add_observer(on_file_editor_action)
            .add_observer(on_file_code_action_pick)
            .add_observer(on_file_completion_request)
            .add_observer(on_file_goto_request)
            .add_observer(on_file_completion_commit)
            .add_observer(on_wiki_completion_request)
            .add_systems(Update, flush_lsp_changes);
    }
}

#[derive(Component)]
pub(super) struct LspEditDirty;

#[derive(EntityEvent)]
pub(super) struct WikiCompletionRequest {
    #[event_target]
    entity: Entity,
}

impl WikiCompletionRequest {
    pub(super) fn new(entity: Entity) -> Self {
        Self { entity }
    }
}

#[derive(Clone, Copy)]
enum LanguageOperation {
    Hover,
    Definition,
    References,
    BeginRename,
    Completion,
    Declaration,
    TypeDefinition,
    Implementation,
    FormatDocument,
    FormatSelection,
    CodeAction,
}

#[derive(EntityEvent)]
pub(super) struct EditorLanguageRequest {
    #[event_target]
    entity: Entity,
    operation: LanguageOperation,
}

impl EditorLanguageRequest {
    pub(super) fn hover(entity: Entity) -> Self {
        Self::new(entity, LanguageOperation::Hover)
    }

    pub(super) fn definition(entity: Entity) -> Self {
        Self::new(entity, LanguageOperation::Definition)
    }

    pub(super) fn references(entity: Entity) -> Self {
        Self::new(entity, LanguageOperation::References)
    }

    pub(super) fn begin_rename(entity: Entity) -> Self {
        Self::new(entity, LanguageOperation::BeginRename)
    }

    pub(super) fn completion(entity: Entity) -> Self {
        Self::new(entity, LanguageOperation::Completion)
    }

    fn declaration(entity: Entity) -> Self {
        Self::new(entity, LanguageOperation::Declaration)
    }

    fn type_definition(entity: Entity) -> Self {
        Self::new(entity, LanguageOperation::TypeDefinition)
    }

    fn implementation(entity: Entity) -> Self {
        Self::new(entity, LanguageOperation::Implementation)
    }

    fn format_document(entity: Entity) -> Self {
        Self::new(entity, LanguageOperation::FormatDocument)
    }

    fn format_selection(entity: Entity) -> Self {
        Self::new(entity, LanguageOperation::FormatSelection)
    }

    fn code_action(entity: Entity) -> Self {
        Self::new(entity, LanguageOperation::CodeAction)
    }

    fn new(entity: Entity, operation: LanguageOperation) -> Self {
        Self { entity, operation }
    }
}

struct LspPosition {
    line: u32,
    utf16_col: u32,
    char_col: usize,
    line_text: String,
}

impl LspPosition {
    fn from_char_col(line: u32, line_text: String, char_col: usize) -> Self {
        let char_col = char_col.min(line_text.chars().count());
        let utf16_col = crate::lsp::manager::char_to_utf16_col(&line_text, char_col as u32);
        Self {
            line,
            utf16_col,
            char_col,
            line_text,
        }
    }

    fn word_start_col(&self) -> u32 {
        let chars: Vec<char> = self.line_text.chars().collect();
        let mut start = self.char_col;
        while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
            start -= 1;
        }
        start as u32
    }

    fn word(&self) -> String {
        let chars: Vec<char> = self.line_text.chars().collect();
        let start = self.word_start_col() as usize;
        let mut end = self.char_col;
        while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
            end += 1;
        }
        chars[start..end].iter().collect()
    }
}

impl Editor {
    fn caret_lsp_position(&self) -> LspPosition {
        let head = self.core.primary().head;
        let (line, char_col) = self.core.buffer.char_to_coords(head);
        self.lsp_position_at_char(line as u32, char_col)
    }

    fn lsp_position_at_cell(&self, line: u32, cell: u32) -> LspPosition {
        let line = line.min(self.core.buffer.len_lines().saturating_sub(1) as u32);
        let line_text = self.line_text(line);
        let char_col = DisplayCells::from(line_text.as_str()).char_at(cell);
        LspPosition::from_char_col(line, line_text, char_col)
    }

    fn lsp_position_at_char(&self, line: u32, char_col: usize) -> LspPosition {
        let line = line.min(self.core.buffer.len_lines().saturating_sub(1) as u32);
        LspPosition::from_char_col(line, self.line_text(line), char_col)
    }

    fn line_text(&self, line: u32) -> String {
        self.core
            .buffer
            .rope
            .line(line as usize)
            .chars()
            .filter(|character| *character != '\n' && *character != '\r')
            .collect()
    }
}

struct WikiCompletion {
    line: u32,
    replace_from_col: u32,
    prefix: String,
}

impl WikiCompletion {
    fn for_edit(edit: &Editor, index: &vmux_core::knowledge::KnowledgeIndex) -> Option<Self> {
        if !index.loaded()
            || !edit.core.buffer.path.starts_with(index.root())
            || !crate::markdown::is_markdown_path(&edit.core.buffer.path)
        {
            return None;
        }
        let position = edit.caret_lsp_position();
        let chars = position.line_text.chars().collect::<Vec<_>>();
        let open = (0..position.char_col.saturating_sub(1))
            .rev()
            .find(|offset| chars[*offset] == '[' && chars[*offset + 1] == '[')?;
        let prefix = chars[open + 2..position.char_col]
            .iter()
            .collect::<String>();
        if prefix.contains("]]") || prefix.contains('|') || prefix.contains('#') {
            return None;
        }
        Some(Self {
            line: position.line,
            replace_from_col: open as u32 + 2,
            prefix,
        })
    }

    fn emit(
        self,
        entity: Entity,
        index: &vmux_core::knowledge::KnowledgeIndex,
        browsers: &Browsers,
        commands: &mut Commands,
    ) {
        if !browsers.can_emit_to(&entity) {
            return;
        }
        let items = index
            .completions(&self.prefix, 32)
            .into_iter()
            .map(|(title, relative)| CompletionItem {
                label: title.clone(),
                insert_text: format!("{title}]]"),
                detail: relative,
                kind: "knowledge".to_string(),
            })
            .collect();
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
            entity,
            &FileCompletionEvent {
                items,
                replace_from_col: self.replace_from_col,
                line: self.line,
            },
        ));
    }
}

fn on_editor_language_request(
    trigger: On<EditorLanguageRequest>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    mut code_actions: MessageWriter<crate::lsp::manager::LspCodeActionRequest>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let path = edit.core.buffer.path.clone();
    match trigger.operation {
        LanguageOperation::Hover => {
            let position = edit.caret_lsp_position();
            manager.hover(
                entity,
                &path,
                position.line,
                position.utf16_col,
                position.char_col as u32,
            );
        }
        LanguageOperation::Definition => {
            let position = edit.caret_lsp_position();
            manager.definition(entity, &path, position.line, position.utf16_col);
        }
        LanguageOperation::References => {
            let position = edit.caret_lsp_position();
            manager.references(entity, &path, position.line, position.utf16_col);
        }
        LanguageOperation::BeginRename => {
            let position = edit.caret_lsp_position();
            let current = position.word();
            if current.is_empty() || !browsers.can_emit_to(&entity) {
                return;
            }
            commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                entity,
                &FileRenameBeginEvent {
                    line: position.line,
                    col: position.char_col as u32,
                    current,
                },
            ));
        }
        LanguageOperation::Completion => {
            let position = edit.caret_lsp_position();
            manager.completion(
                entity,
                &path,
                position.line,
                position.utf16_col,
                position.word_start_col(),
            );
        }
        LanguageOperation::Declaration => {
            let position = edit.caret_lsp_position();
            manager.declaration(entity, &path, position.line, position.utf16_col);
        }
        LanguageOperation::TypeDefinition => {
            let position = edit.caret_lsp_position();
            manager.type_definition(entity, &path, position.line, position.utf16_col);
        }
        LanguageOperation::Implementation => {
            let position = edit.caret_lsp_position();
            manager.implementation(entity, &path, position.line, position.utf16_col);
        }
        LanguageOperation::FormatDocument => manager.format_document(entity, &path),
        LanguageOperation::FormatSelection => {
            let (from, to) = edit.core.selected_lines();
            manager.format_range(entity, &path, from, to);
        }
        LanguageOperation::CodeAction => {
            let (from_line, to_line) = edit.core.selected_lines();
            code_actions.write(crate::lsp::manager::LspCodeActionRequest {
                entity,
                path,
                from_line,
                to_line,
            });
        }
    }
}

fn on_wiki_completion_request(
    trigger: On<WikiCompletionRequest>,
    views: Query<&Editor>,
    index: Option<Res<vmux_core::knowledge::KnowledgeIndex>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let Some(index) = index.as_deref() else {
        return;
    };
    let entity = trigger.entity;
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let Some(completion) = WikiCompletion::for_edit(edit, index) else {
        return;
    };
    completion.emit(entity, index, &browsers, &mut commands);
}

fn on_file_hover_request(
    trigger: On<BinReceive<FileHoverRequest>>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event().webview;
    let request = trigger.event().payload;
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let position = edit.lsp_position_at_cell(request.line, request.col);
    manager.hover(
        entity,
        &edit.core.buffer.path,
        position.line,
        position.utf16_col,
        position.char_col as u32,
    );
}

fn on_file_definition_request(
    trigger: On<BinReceive<FileDefinitionRequest>>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event().webview;
    let request = trigger.event().payload;
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let position = edit.lsp_position_at_cell(request.line, request.col);
    manager.definition(
        entity,
        &edit.core.buffer.path,
        position.line,
        position.utf16_col,
    );
}

fn on_file_editor_action(
    trigger: On<BinReceive<FileEditorAction>>,
    mut command_invocations: MessageWriter<vmux_command::CommandInvocation>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = match trigger.event().payload.action {
        EditorAction::CommandPalette => {
            command_invocations.write(vmux_command::CommandInvocation::new(
                entity,
                "browser_open_command_bar",
            ));
            return;
        }
        EditorAction::CodeAction => EditorLanguageRequest::code_action(entity),
        EditorAction::GotoDeclaration => EditorLanguageRequest::declaration(entity),
        EditorAction::GotoTypeDefinition => EditorLanguageRequest::type_definition(entity),
        EditorAction::GotoImplementation => EditorLanguageRequest::implementation(entity),
        EditorAction::FormatDocument => EditorLanguageRequest::format_document(entity),
        EditorAction::FormatSelection => EditorLanguageRequest::format_selection(entity),
        EditorAction::Rename => EditorLanguageRequest::begin_rename(entity),
        EditorAction::Copy => {
            commands.trigger(EditRequest::new(
                entity,
                vec![EditCommand::Op {
                    operator: crate::edit::command::Operator::Yank,
                    target: crate::edit::command::Target::Selection,
                    register: None,
                }],
            ));
            return;
        }
        EditorAction::Cut => {
            commands.trigger(EditRequest::new(
                entity,
                vec![EditCommand::Op {
                    operator: crate::edit::command::Operator::Delete,
                    target: crate::edit::command::Target::Selection,
                    register: None,
                }],
            ));
            return;
        }
        EditorAction::Paste => {
            commands.trigger(EditRequest::new(entity, vec![EditCommand::Paste]));
            return;
        }
        EditorAction::ChangeAllOccurrences => {
            commands.trigger(EditRequest::new(
                entity,
                vec![EditCommand::SelectAllOccurrences],
            ));
            return;
        }
    };
    commands.trigger(request);
}

fn on_file_code_action_pick(
    trigger: On<BinReceive<FileCodeActionPick>>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    mut edits: MessageWriter<crate::lsp::manager::LspRequestedEdit>,
) {
    let entity = trigger.event().webview;
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let path = edit.core.buffer.path.clone();
    let Some((root, workspace_edit)) =
        manager.run_code_action(entity, trigger.event().payload.index as usize, &path)
    else {
        return;
    };
    edits.write(crate::lsp::manager::LspRequestedEdit {
        entity,
        root,
        result: Ok(workspace_edit),
    });
}

fn on_file_rename_request(
    trigger: On<BinReceive<FileRenameRequest>>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    if request.new_name.trim().is_empty() {
        return;
    }
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let position = edit.lsp_position_at_cell(request.line, request.col);
    manager.rename(
        entity,
        &edit.core.buffer.path,
        position.line,
        position.utf16_col,
        &request.new_name,
    );
}

fn on_file_references_request(
    trigger: On<BinReceive<FileReferencesRequest>>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event().webview;
    let request = trigger.event().payload;
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let position = edit.lsp_position_at_cell(request.line, request.col);
    manager.references(
        entity,
        &edit.core.buffer.path,
        position.line,
        position.utf16_col,
    );
}

fn on_file_completion_request(
    trigger: On<BinReceive<FileCompletionRequest>>,
    views: Query<&Editor>,
    index: Option<Res<vmux_core::knowledge::KnowledgeIndex>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event().webview;
    let request = trigger.event().payload;
    let Ok(edit) = views.get(entity) else {
        return;
    };
    if let Some(index) = index.as_deref()
        && let Some(completion) = WikiCompletion::for_edit(edit, index)
    {
        completion.emit(entity, index, &browsers, &mut commands);
        return;
    }
    let position = edit.lsp_position_at_cell(request.line, request.col);
    manager.completion(
        entity,
        &edit.core.buffer.path,
        position.line,
        position.utf16_col,
        position.word_start_col(),
    );
}

fn on_file_goto_request(
    trigger: On<BinReceive<FileGotoRequest>>,
    mut goto: MessageWriter<crate::lsp::manager::LspGoto>,
) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    let path = PathBuf::from(&request.path);
    let line_text = crate::lsp::manager::disk_line(&path, request.line);
    let utf16_col = crate::lsp::manager::char_to_utf16_col(&line_text, request.col);
    goto.write(crate::lsp::manager::LspGoto {
        entity,
        path,
        line: request.line,
        utf16_col,
    });
}

fn on_file_completion_commit(
    trigger: On<BinReceive<FileCompletionCommit>>,
    mut views: Query<&mut Editor>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = trigger.event().payload.clone();
    let Ok(mut edit) = views.get_mut(entity) else {
        return;
    };
    let start = edit
        .core
        .buffer
        .coords_to_char(request.line as usize, request.replace_from_col as usize);
    let head = edit.core.primary().head;
    edit.core.selections = vec![Selection {
        anchor: start.min(head),
        head: start.max(head),
    }];
    commands.trigger(EditRequest::new(
        entity,
        vec![EditCommand::InsertText(request.text)],
    ));
}

fn flush_lsp_changes(
    time: Res<Time>,
    mut elapsed: Local<f32>,
    views: Query<(Entity, &FileView, &Editor), With<LspEditDirty>>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
    if views.is_empty() {
        return;
    }
    *elapsed += time.delta_secs();
    if *elapsed < 0.15 {
        return;
    }
    *elapsed = 0.0;
    for (entity, view, edit) in &views {
        manager.change_with_text(&view.path, &edit.core.buffer.text());
        manager.folding_range(entity, &view.path);
        manager.semantic_tokens(entity, &view.path);
        if !crate::explorer_model::is_markdown(&view.path) {
            manager.document_symbol(entity, &view.path);
        }
        commands.entity(entity).remove::<LspEditDirty>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_prefill_is_the_identifier_around_the_caret() {
        let text = "let some_name = 1;";

        assert_eq!(
            LspPosition::from_char_col(0, text.to_string(), 8).word(),
            "some_name"
        );
        assert_eq!(
            LspPosition::from_char_col(0, text.to_string(), 4).word(),
            "some_name"
        );
        assert_eq!(
            LspPosition::from_char_col(0, text.to_string(), 13).word(),
            "some_name"
        );
        assert_eq!(
            LspPosition::from_char_col(0, text.to_string(), 14).word(),
            ""
        );
    }
}
