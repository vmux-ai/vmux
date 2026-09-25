use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::event::*;

use crate::edit::EditCommand;
use crate::event::*;
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
            FileCodeActionPick,
            FileCompletionRequest,
        )>::default())
            .add_plugins(UiEventPlugin::<FileEditorOperationRequests>::default())
            .add_observer(on_editor_hover)
            .add_observer(on_editor_definition)
            .add_observer(on_editor_references)
            .add_observer(on_editor_rename)
            .add_observer(on_editor_completion)
            .add_observer(on_editor_declaration)
            .add_observer(on_editor_type_definition)
            .add_observer(on_editor_implementation)
            .add_observer(on_editor_format_document)
            .add_observer(on_editor_format_selection)
            .add_observer(on_editor_code_action)
            .add_observer(on_file_hover_request)
            .add_observer(on_file_definition_request)
            .add_observer(on_file_references_request)
            .add_observer(on_file_rename_request)
            .add_observer(on_file_editor_command_palette_request)
            .add_observer(on_file_editor_code_action_request)
            .add_observer(on_file_editor_goto_declaration_request)
            .add_observer(on_file_editor_goto_type_definition_request)
            .add_observer(on_file_editor_goto_implementation_request)
            .add_observer(on_file_editor_format_document_request)
            .add_observer(on_file_editor_format_selection_request)
            .add_observer(on_file_editor_rename_request)
            .add_observer(on_file_editor_copy_request)
            .add_observer(on_file_editor_cut_request)
            .add_observer(on_file_editor_paste_request)
            .add_observer(on_file_editor_change_all_occurrences_request)
            .add_observer(on_file_code_action_pick)
            .add_observer(on_file_completion_request)
            .add_observer(on_wiki_completion_request)
            .add_systems(Update, flush_lsp_changes);
    }
}

#[derive(Component)]
pub(super) struct LspEditDirty;

#[derive(EntityEvent)]
pub(super) struct WikiCompletionRequest(#[event_target] Entity);

impl From<Entity> for WikiCompletionRequest {
    fn from(entity: Entity) -> Self {
        Self(entity)
    }
}

#[derive(EntityEvent)]
pub(super) struct EditorHoverRequest(#[event_target] Entity);

impl From<Entity> for EditorHoverRequest {
    fn from(entity: Entity) -> Self {
        Self(entity)
    }
}

#[derive(EntityEvent)]
pub(super) struct EditorDefinitionRequest(#[event_target] Entity);

impl From<Entity> for EditorDefinitionRequest {
    fn from(entity: Entity) -> Self {
        Self(entity)
    }
}

#[derive(EntityEvent)]
pub(super) struct EditorReferencesRequest(#[event_target] Entity);

impl From<Entity> for EditorReferencesRequest {
    fn from(entity: Entity) -> Self {
        Self(entity)
    }
}

#[derive(EntityEvent)]
pub(super) struct EditorRenameRequest(#[event_target] Entity);

impl From<Entity> for EditorRenameRequest {
    fn from(entity: Entity) -> Self {
        Self(entity)
    }
}

#[derive(EntityEvent)]
pub(super) struct EditorCompletionRequest(#[event_target] Entity);

impl From<Entity> for EditorCompletionRequest {
    fn from(entity: Entity) -> Self {
        Self(entity)
    }
}

#[derive(EntityEvent)]
struct EditorDeclarationRequest(#[event_target] Entity);

impl From<Entity> for EditorDeclarationRequest {
    fn from(entity: Entity) -> Self {
        Self(entity)
    }
}

#[derive(EntityEvent)]
struct EditorTypeDefinitionRequest(#[event_target] Entity);

impl From<Entity> for EditorTypeDefinitionRequest {
    fn from(entity: Entity) -> Self {
        Self(entity)
    }
}

#[derive(EntityEvent)]
struct EditorImplementationRequest(#[event_target] Entity);

impl From<Entity> for EditorImplementationRequest {
    fn from(entity: Entity) -> Self {
        Self(entity)
    }
}

#[derive(EntityEvent)]
struct EditorFormatDocumentRequest(#[event_target] Entity);

impl From<Entity> for EditorFormatDocumentRequest {
    fn from(entity: Entity) -> Self {
        Self(entity)
    }
}

#[derive(EntityEvent)]
struct EditorFormatSelectionRequest(#[event_target] Entity);

impl From<Entity> for EditorFormatSelectionRequest {
    fn from(entity: Entity) -> Self {
        Self(entity)
    }
}

#[derive(EntityEvent)]
struct EditorCodeActionRequest(#[event_target] Entity);

impl From<Entity> for EditorCodeActionRequest {
    fn from(entity: Entity) -> Self {
        Self(entity)
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

    fn result(
        self,
        entity: Entity,
        index: &vmux_core::knowledge::KnowledgeIndex,
    ) -> crate::host::panel::CompletionResult {
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
        crate::host::panel::CompletionResult::new(entity, items, self.replace_from_col, self.line)
    }
}

fn on_editor_hover(
    trigger: On<EditorHoverRequest>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let position = edit.caret_lsp_position();
    manager.hover(
        entity,
        &edit.core.buffer.path,
        position.line,
        position.utf16_col,
        position.char_col as u32,
    );
}

fn on_editor_definition(
    trigger: On<EditorDefinitionRequest>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let position = edit.caret_lsp_position();
    manager.definition(
        entity,
        &edit.core.buffer.path,
        position.line,
        position.utf16_col,
    );
}

fn on_editor_references(
    trigger: On<EditorReferencesRequest>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let position = edit.caret_lsp_position();
    manager.references(
        entity,
        &edit.core.buffer.path,
        position.line,
        position.utf16_col,
    );
}

fn on_editor_rename(
    trigger: On<EditorRenameRequest>,
    views: Query<&Editor>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let position = edit.caret_lsp_position();
    let current = position.word();
    if current.is_empty() || !browsers.can_emit_to(&entity) {
        return;
    }
    commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
        entity,
        &FileRenamePrompt {
            line: position.line,
            col: position.char_col as u32,
            current,
        },
    ));
}

fn on_editor_completion(
    trigger: On<EditorCompletionRequest>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let position = edit.caret_lsp_position();
    manager.completion(
        entity,
        &edit.core.buffer.path,
        position.line,
        position.utf16_col,
        position.word_start_col(),
    );
}

fn on_editor_declaration(
    trigger: On<EditorDeclarationRequest>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let position = edit.caret_lsp_position();
    manager.declaration(
        entity,
        &edit.core.buffer.path,
        position.line,
        position.utf16_col,
    );
}

fn on_editor_type_definition(
    trigger: On<EditorTypeDefinitionRequest>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let position = edit.caret_lsp_position();
    manager.type_definition(
        entity,
        &edit.core.buffer.path,
        position.line,
        position.utf16_col,
    );
}

fn on_editor_implementation(
    trigger: On<EditorImplementationRequest>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let position = edit.caret_lsp_position();
    manager.implementation(
        entity,
        &edit.core.buffer.path,
        position.line,
        position.utf16_col,
    );
}

fn on_editor_format_document(
    trigger: On<EditorFormatDocumentRequest>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    manager.format_document(entity, &edit.core.buffer.path);
}

fn on_editor_format_selection(
    trigger: On<EditorFormatSelectionRequest>,
    views: Query<&Editor>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (from, to) = edit.core.selected_lines();
    manager.format_range(entity, &edit.core.buffer.path, from, to);
}

fn on_editor_code_action(
    trigger: On<EditorCodeActionRequest>,
    views: Query<&Editor>,
    mut requests: MessageWriter<crate::lsp::manager::LspCodeActionRequest>,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (from_line, to_line) = edit.core.selected_lines();
    requests.write(crate::lsp::manager::LspCodeActionRequest {
        entity,
        path: edit.core.buffer.path.clone(),
        from_line,
        to_line,
    });
}

fn on_wiki_completion_request(
    trigger: On<WikiCompletionRequest>,
    views: Query<&Editor>,
    index: Option<Res<vmux_core::knowledge::KnowledgeIndex>>,
    mut commands: Commands,
) {
    let Some(index) = index.as_deref() else {
        return;
    };
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let Some(completion) = WikiCompletion::for_edit(edit, index) else {
        return;
    };
    commands.trigger(completion.result(entity, index));
}

fn on_file_hover_request(
    trigger: On<UiInput<FileHoverRequest>>,
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
    trigger: On<UiInput<FileDefinitionRequest>>,
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

fn on_file_editor_command_palette_request(
    trigger: On<UiInput<FileEditorCommandPaletteRequest>>,
    mut command_invocations: MessageWriter<vmux_command::CommandInvocation>,
) {
    command_invocations.write(vmux_command::CommandInvocation::new(
        trigger.event().webview,
        "browser_open_command_bar",
    ));
}

fn on_file_editor_code_action_request(
    trigger: On<UiInput<FileEditorCodeActionRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditorCodeActionRequest::from(trigger.event().webview));
}

fn on_file_editor_goto_declaration_request(
    trigger: On<UiInput<FileEditorGotoDeclarationRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditorDeclarationRequest::from(trigger.event().webview));
}

fn on_file_editor_goto_type_definition_request(
    trigger: On<UiInput<FileEditorGotoTypeDefinitionRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditorTypeDefinitionRequest::from(trigger.event().webview));
}

fn on_file_editor_goto_implementation_request(
    trigger: On<UiInput<FileEditorGotoImplementationRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditorImplementationRequest::from(trigger.event().webview));
}

fn on_file_editor_format_document_request(
    trigger: On<UiInput<FileEditorFormatDocumentRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditorFormatDocumentRequest::from(trigger.event().webview));
}

fn on_file_editor_format_selection_request(
    trigger: On<UiInput<FileEditorFormatSelectionRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditorFormatSelectionRequest::from(trigger.event().webview));
}

fn on_file_editor_rename_request(
    trigger: On<UiInput<FileEditorRenameRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditorRenameRequest::from(trigger.event().webview));
}

fn on_file_editor_copy_request(
    trigger: On<UiInput<FileEditorCopyRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditRequest::new(
        trigger.event().webview,
        vec![EditCommand::Op {
            operator: crate::edit::command::Operator::Yank,
            target: crate::edit::command::Target::Selection,
            register: None,
        }],
    ));
}

fn on_file_editor_cut_request(trigger: On<UiInput<FileEditorCutRequest>>, mut commands: Commands) {
    commands.trigger(EditRequest::new(
        trigger.event().webview,
        vec![EditCommand::Op {
            operator: crate::edit::command::Operator::Delete,
            target: crate::edit::command::Target::Selection,
            register: None,
        }],
    ));
}

fn on_file_editor_paste_request(
    trigger: On<UiInput<FileEditorPasteRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditRequest::new(
        trigger.event().webview,
        vec![EditCommand::Paste],
    ));
}

fn on_file_editor_change_all_occurrences_request(
    trigger: On<UiInput<FileEditorChangeAllOccurrencesRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditRequest::new(
        trigger.event().webview,
        vec![EditCommand::SelectAllOccurrences],
    ));
}

fn on_file_code_action_pick(
    trigger: On<UiInput<FileCodeActionPick>>,
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
    trigger: On<UiInput<FileRenameRequest>>,
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
    trigger: On<UiInput<FileReferencesRequest>>,
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
    trigger: On<UiInput<FileCompletionRequest>>,
    views: Query<&Editor>,
    index: Option<Res<vmux_core::knowledge::KnowledgeIndex>>,
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
        commands.trigger(completion.result(entity, index));
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
