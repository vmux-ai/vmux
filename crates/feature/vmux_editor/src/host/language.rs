use crate::host::editor::{EditReadOnly, EditReadOnlyItem};
use bevy::prelude::*;
use bevy::prelude::{
    App, Commands, Component, Entity, EntityEvent, MessageWriter, On, Plugin, Query, With,
};
use bevy_cef::prelude::{UiEventPlugin, UiEventSystems, UiInput};
use vmux_ecs::event::*;

use crate::event::*;
use crate::host::editor::FileView;
use crate::host::markdown::ParsedNote;
use crate::lsp::manager::LspLine;
use crate::text::DisplayCells;

pub(super) struct LanguagePlugin;

impl Plugin for LanguagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            FileHoverRequest,
            FileHoverDismissRequest,
            FileDefinitionRequest,
            FileReferencesRequest,
            FileRenameRequest,
            FileRenameDraftRequest,
            FileRenameDismissRequest,
            FileCodeActionMoveRequest,
            FileCodeActionPick,
            FileCodeActionDismissRequest,
            FileCompletionRequest,
        )>::default())
            .add_plugins(UiEventPlugin::<FileEditorOperationRequests>::default())
            .add_observer(hover)
            .add_observer(definition)
            .add_observer(references)
            .add_observer(rename)
            .add_observer(completion)
            .add_observer(declaration)
            .add_observer(type_definition)
            .add_observer(implementation)
            .add_observer(format_document)
            .add_observer(format_selection)
            .add_observer(code_action)
            .add_observer(request_hover)
            .add_observer(dismiss_hover)
            .add_observer(request_definition)
            .add_observer(request_references)
            .add_observer(request_rename)
            .add_observer(edit_rename)
            .add_observer(dismiss_rename)
            .add_observer(open_command_palette)
            .add_observer(request_code_action)
            .add_observer(request_declaration)
            .add_observer(request_type_definition)
            .add_observer(request_implementation)
            .add_observer(request_format_document)
            .add_observer(request_format_selection)
            .add_observer(request_rename_prompt)
            .add_observer(copy_selection)
            .add_observer(cut_selection)
            .add_observer(paste)
            .add_observer(select_occurrences)
            .add_observer(request_completion)
            .add_observer(wiki_completion)
            .add_systems(
                Update,
                flush_lsp_changes
                    .before(vmux_lsp::Set::Documents)
                    .after(UiEventSystems::Flush),
            );
    }
}

#[derive(Component)]
#[require(LspChangeElapsed)]
pub(super) struct LspEditDirty;

#[derive(Component, Default)]
struct FileRenamePrompt {
    open: bool,
    line: u32,
    col: u32,
    current: String,
    draft: String,
}

impl FileRenamePrompt {
    fn snapshot(&self) -> FileRenameState {
        FileRenameState {
            open: self.open,
            line: self.line,
            col: self.col,
            current: self.current.clone(),
            draft: self.draft.clone(),
        }
    }
}

#[derive(Component, Default)]
struct LspChangeElapsed(std::time::Duration);

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

impl EditReadOnlyItem<'_, '_> {
    fn from_char_col(line: u32, line_text: String, char_col: usize) -> (u32, u32, usize, String) {
        let char_col = char_col.min(line_text.chars().count());
        let line_text = LspLine::new(line_text);
        let utf16_col = line_text.utf16_col(char_col as u32);
        (line, utf16_col, char_col, line_text.into_string())
    }

    fn word_start_col(line_text: &str, char_col: usize) -> u32 {
        let chars: Vec<char> = line_text.chars().collect();
        let mut start = char_col.min(chars.len());
        while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
            start -= 1;
        }
        start as u32
    }

    fn word_at(line_text: &str, char_col: usize) -> String {
        let chars: Vec<char> = line_text.chars().collect();
        let start = Self::word_start_col(line_text, char_col) as usize;
        let mut end = char_col.min(chars.len());
        while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
            end += 1;
        }
        chars[start..end].iter().collect()
    }

    fn caret_lsp_position(&self) -> (u32, u32, usize, String) {
        let head = self.core.primary().head;
        let (line, char_col) = self.core.buffer.char_to_coords(head);
        self.lsp_position_at_char(line as u32, char_col)
    }

    fn lsp_position_at_cell(&self, line: u32, cell: u32) -> (u32, u32, usize, String) {
        let line = line.min(self.core.buffer.len_lines().saturating_sub(1) as u32);
        let line_text = self.line_text(line);
        let char_col = DisplayCells::from(line_text.as_str()).char_at(cell);
        Self::from_char_col(line, line_text, char_col)
    }

    fn lsp_position_at_char(&self, line: u32, char_col: usize) -> (u32, u32, usize, String) {
        let line = line.min(self.core.buffer.len_lines().saturating_sub(1) as u32);
        Self::from_char_col(line, self.line_text(line), char_col)
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

fn hover(
    trigger: On<EditorHoverRequest>,
    views: Query<EditReadOnly>,
    manager: crate::lsp::manager::LspManager<'_, '_>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (line, utf16_col, char_col, _) = edit.caret_lsp_position();
    let request = manager.hover(
        entity,
        &edit.core.buffer.path,
        line,
        utf16_col,
        char_col as u32,
    );
    if let Some(request) = request {
        commands.spawn(request.bundle());
    }
}

fn definition(
    trigger: On<EditorDefinitionRequest>,
    views: Query<EditReadOnly>,
    manager: crate::lsp::manager::LspManager<'_, '_>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (line, utf16_col, _, _) = edit.caret_lsp_position();
    let request = manager.definition(entity, &edit.core.buffer.path, line, utf16_col);
    if let Some(request) = request {
        commands.spawn(request.bundle());
    }
}

fn references(
    trigger: On<EditorReferencesRequest>,
    views: Query<EditReadOnly>,
    manager: crate::lsp::manager::LspManager<'_, '_>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (line, utf16_col, _, _) = edit.caret_lsp_position();
    let request = manager.references(entity, &edit.core.buffer.path, line, utf16_col);
    if let Some(request) = request {
        commands.spawn(request.bundle());
    }
}

fn rename(
    trigger: On<EditorRenameRequest>,
    views: Query<EditReadOnly>,
    browsers: crate::host::delivery::Surface,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (line, _, char_col, line_text) = edit.caret_lsp_position();
    let current = EditReadOnlyItem::word_at(&line_text, char_col);
    if current.is_empty() || !browsers.can_emit_to(&entity) {
        return;
    }
    let prompt = FileRenamePrompt {
        open: true,
        line,
        col: char_col as u32,
        current: current.clone(),
        draft: current,
    };
    let state = prompt.snapshot();
    commands.entity(entity).insert(prompt);
    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(entity, &state));
    commands.trigger(super::feedback::ClearEditFailure(entity));
}

fn completion(
    trigger: On<EditorCompletionRequest>,
    views: Query<EditReadOnly>,
    manager: crate::lsp::manager::LspManager<'_, '_>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (line, utf16_col, char_col, line_text) = edit.caret_lsp_position();
    let request = manager.completion(
        entity,
        &edit.core.buffer.path,
        line,
        utf16_col,
        EditReadOnlyItem::word_start_col(&line_text, char_col),
    );
    if let Some(request) = request {
        commands.spawn(request.bundle());
    }
}

fn declaration(
    trigger: On<EditorDeclarationRequest>,
    views: Query<EditReadOnly>,
    manager: crate::lsp::manager::LspManager<'_, '_>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (line, utf16_col, _, _) = edit.caret_lsp_position();
    let request = manager.declaration(entity, &edit.core.buffer.path, line, utf16_col);
    if let Some(request) = request {
        commands.spawn(request.bundle());
    }
}

fn type_definition(
    trigger: On<EditorTypeDefinitionRequest>,
    views: Query<EditReadOnly>,
    manager: crate::lsp::manager::LspManager<'_, '_>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (line, utf16_col, _, _) = edit.caret_lsp_position();
    let request = manager.type_definition(entity, &edit.core.buffer.path, line, utf16_col);
    if let Some(request) = request {
        commands.spawn(request.bundle());
    }
}

fn implementation(
    trigger: On<EditorImplementationRequest>,
    views: Query<EditReadOnly>,
    manager: crate::lsp::manager::LspManager<'_, '_>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (line, utf16_col, _, _) = edit.caret_lsp_position();
    let request = manager.implementation(entity, &edit.core.buffer.path, line, utf16_col);
    if let Some(request) = request {
        commands.spawn(request.bundle());
    }
}

fn format_document(
    trigger: On<EditorFormatDocumentRequest>,
    views: Query<EditReadOnly>,
    manager: crate::lsp::manager::LspManager<'_, '_>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    if let Some(request) = manager.format_document(entity, &edit.core.buffer.path) {
        commands.spawn(request.bundle());
    }
}

fn format_selection(
    trigger: On<EditorFormatSelectionRequest>,
    views: Query<EditReadOnly>,
    manager: crate::lsp::manager::LspManager<'_, '_>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (from, to) = edit.core.selected_lines();
    if let Some(request) = manager.format_range(entity, &edit.core.buffer.path, from, to) {
        commands.spawn(request.bundle());
    }
}

fn code_action(
    trigger: On<EditorCodeActionRequest>,
    views: Query<EditReadOnly>,
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

fn wiki_completion(
    trigger: On<WikiCompletionRequest>,
    views: Query<EditReadOnly>,
    graph: vmux_knowledge::KnowledgeGraph,
    mut commands: Commands,
) {
    let Some((owner, index)) = graph.index() else {
        return;
    };
    let entity = trigger.event_target();
    let Ok(edit) = views.get(entity) else {
        return;
    };
    if !index.loaded()
        || !edit.core.buffer.path.starts_with(index.root())
        || !ParsedNote::supports(&edit.core.buffer.path)
    {
        return;
    }
    let Some((line, replace_from_col, prefix)) =
        edit.core.buffer.wiki_prefix(edit.core.primary().head)
    else {
        return;
    };
    let entries = graph.completions(owner, &prefix, 32);
    commands.trigger(crate::host::panel::CompletionResult::from_knowledge(
        entity,
        entries,
        replace_from_col,
        line,
    ));
}

fn request_hover(
    trigger: On<UiInput<FileHoverRequest>>,
    views: Query<EditReadOnly>,
    manager: crate::lsp::manager::LspManager<'_, '_>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = trigger.event().payload;
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (line, utf16_col, char_col, _) = edit.lsp_position_at_cell(request.line, request.col);
    let request = manager.hover(
        entity,
        &edit.core.buffer.path,
        line,
        utf16_col,
        char_col as u32,
    );
    if let Some(request) = request {
        commands.spawn(request.bundle());
    }
}

fn dismiss_hover(trigger: On<UiInput<FileHoverDismissRequest>>, mut commands: Commands) {
    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
        trigger.event().webview,
        &FileHoverState::default(),
    ));
}

fn request_definition(
    trigger: On<UiInput<FileDefinitionRequest>>,
    views: Query<EditReadOnly>,
    manager: crate::lsp::manager::LspManager<'_, '_>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = trigger.event().payload;
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (line, utf16_col, _, _) = edit.lsp_position_at_cell(request.line, request.col);
    let request = manager.definition(entity, &edit.core.buffer.path, line, utf16_col);
    if let Some(request) = request {
        commands.spawn(request.bundle());
    }
}

fn open_command_palette(
    trigger: On<UiInput<FileEditorCommandPaletteRequest>>,
    mut command_invocations: MessageWriter<vmux_command::CommandInvocation>,
) {
    command_invocations.write(vmux_command::CommandInvocation::new(
        trigger.event().webview,
        "command_bar_open",
    ));
}

fn request_code_action(trigger: On<UiInput<FileEditorCodeActionRequest>>, mut commands: Commands) {
    commands.trigger(EditorCodeActionRequest::from(trigger.event().webview));
}

fn request_declaration(
    trigger: On<UiInput<FileEditorGotoDeclarationRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditorDeclarationRequest::from(trigger.event().webview));
}

fn request_type_definition(
    trigger: On<UiInput<FileEditorGotoTypeDefinitionRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditorTypeDefinitionRequest::from(trigger.event().webview));
}

fn request_implementation(
    trigger: On<UiInput<FileEditorGotoImplementationRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditorImplementationRequest::from(trigger.event().webview));
}

fn request_format_document(
    trigger: On<UiInput<FileEditorFormatDocumentRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditorFormatDocumentRequest::from(trigger.event().webview));
}

fn request_format_selection(
    trigger: On<UiInput<FileEditorFormatSelectionRequest>>,
    mut commands: Commands,
) {
    commands.trigger(EditorFormatSelectionRequest::from(trigger.event().webview));
}

fn request_rename_prompt(trigger: On<UiInput<FileEditorRenameRequest>>, mut commands: Commands) {
    commands.trigger(EditorRenameRequest::from(trigger.event().webview));
}

fn copy_selection(
    trigger: On<UiInput<FileEditorCopyRequest>>,
    documents: Query<&crate::edit::document::Revision>,
    mut commands: Commands,
) {
    let Ok(revision) = documents.get(trigger.event().webview) else {
        return;
    };
    commands.trigger(crate::host::editing::document::OperatorRequest {
        entity: trigger.event().webview,
        document: Some(revision.identity),
        operator: crate::edit::command::Operator::Yank,
        target: crate::edit::command::Target::Selection,
        register: None,
    });
}

fn cut_selection(
    trigger: On<UiInput<FileEditorCutRequest>>,
    documents: Query<&crate::edit::document::Revision>,
    mut commands: Commands,
) {
    let Ok(revision) = documents.get(trigger.event().webview) else {
        return;
    };
    commands.trigger(crate::host::editing::document::OperatorRequest {
        entity: trigger.event().webview,
        document: Some(revision.identity),
        operator: crate::edit::command::Operator::Delete,
        target: crate::edit::command::Target::Selection,
        register: None,
    });
}

fn paste(
    trigger: On<UiInput<FileEditorPasteRequest>>,
    documents: Query<&crate::edit::document::Revision>,
    mut commands: Commands,
) {
    let Ok(revision) = documents.get(trigger.event().webview) else {
        return;
    };
    commands.trigger(crate::host::editing::effects::PasteRequest {
        entity: trigger.event().webview,
        document: Some(revision.identity),
    });
}

fn select_occurrences(
    trigger: On<UiInput<FileEditorChangeAllOccurrencesRequest>>,
    documents: Query<&crate::edit::document::Revision>,
    mut commands: Commands,
) {
    let Ok(revision) = documents.get(trigger.event().webview) else {
        return;
    };
    commands.trigger(crate::host::editing::document::SelectOccurrencesRequest {
        entity: trigger.event().webview,
        document: Some(revision.identity),
    });
}

fn request_rename(
    trigger: On<UiInput<FileRenameRequest>>,
    views: Query<EditReadOnly>,
    manager: crate::lsp::manager::LspManager<'_, '_>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    commands.entity(entity).remove::<FileRenamePrompt>();
    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
        entity,
        &FileRenameState::default(),
    ));
    let request = &trigger.event().payload;
    if request.new_name.trim().is_empty() {
        return;
    }
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (line, utf16_col, _, _) = edit.lsp_position_at_cell(request.line, request.col);
    let request = manager.rename(
        entity,
        &edit.core.buffer.path,
        line,
        utf16_col,
        &request.new_name,
    );
    if let Some(request) = request {
        commands.spawn(request.bundle());
    }
}

fn edit_rename(
    trigger: On<UiInput<FileRenameDraftRequest>>,
    mut prompts: Query<&mut FileRenamePrompt>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(mut prompt) = prompts.get_mut(entity) else {
        return;
    };
    if !prompt.open {
        return;
    }
    prompt.draft.clone_from(&trigger.event().payload.draft);
    let state = prompt.snapshot();
    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(entity, &state));
}

fn dismiss_rename(trigger: On<UiInput<FileRenameDismissRequest>>, mut commands: Commands) {
    let entity = trigger.event().webview;
    commands.entity(entity).remove::<FileRenamePrompt>();
    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
        entity,
        &FileRenameState::default(),
    ));
}

fn request_references(
    trigger: On<UiInput<FileReferencesRequest>>,
    views: Query<EditReadOnly>,
    manager: crate::lsp::manager::LspManager<'_, '_>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = trigger.event().payload;
    let Ok(edit) = views.get(entity) else {
        return;
    };
    let (line, utf16_col, _, _) = edit.lsp_position_at_cell(request.line, request.col);
    let request = manager.references(entity, &edit.core.buffer.path, line, utf16_col);
    if let Some(request) = request {
        commands.spawn(request.bundle());
    }
}

fn request_completion(
    trigger: On<UiInput<FileCompletionRequest>>,
    views: Query<EditReadOnly>,
    graph: vmux_knowledge::KnowledgeGraph,
    mut commands: Commands,
    manager: crate::lsp::manager::LspManager<'_, '_>,
) {
    let entity = trigger.event().webview;
    let request = trigger.event().payload;
    let Ok(edit) = views.get(entity) else {
        return;
    };
    if let Some((owner, index)) = graph.index()
        && index.loaded()
        && edit.core.buffer.path.starts_with(index.root())
        && ParsedNote::supports(&edit.core.buffer.path)
        && let Some((line, replace_from_col, prefix)) =
            edit.core.buffer.wiki_prefix(edit.core.primary().head)
    {
        let entries = graph.completions(owner, &prefix, 32);
        commands.trigger(crate::host::panel::CompletionResult::from_knowledge(
            entity,
            entries,
            replace_from_col,
            line,
        ));
        return;
    }
    let (line, utf16_col, char_col, line_text) =
        edit.lsp_position_at_cell(request.line, request.col);
    let request = manager.completion(
        entity,
        &edit.core.buffer.path,
        line,
        utf16_col,
        EditReadOnlyItem::word_start_col(&line_text, char_col),
    );
    if let Some(request) = request {
        commands.spawn(request.bundle());
    }
}

fn flush_lsp_changes(
    time: Res<Time>,
    mut views: Query<(Entity, &FileView, EditReadOnly, &mut LspChangeElapsed), With<LspEditDirty>>,
    mut lsp_changes: Option<MessageWriter<vmux_lsp::ChangeDocument>>,
    mut commands: Commands,
) {
    for (entity, view, edit, mut elapsed) in &mut views {
        elapsed.0 += time.delta();
        if elapsed.0 < std::time::Duration::from_millis(150) {
            continue;
        }
        if let Some(lsp_changes) = lsp_changes.as_mut() {
            lsp_changes.write(vmux_lsp::ChangeDocument {
                owner: entity,
                path: view.path.clone(),
                text: edit.core.buffer.text(),
                revision: Some(edit.core.content_version()),
            });
        }
        commands
            .entity(entity)
            .remove::<(LspEditDirty, LspChangeElapsed)>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_views_flush_on_their_own_interval_and_release_the_timer() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .add_message::<vmux_lsp::ChangeDocument>()
            .add_plugins(LanguagePlugin);
        let mut views = Vec::new();
        for name in ["first", "second"] {
            let path = std::path::PathBuf::from(format!("/workspace/{name}.rs"));
            let entity = app
                .world_mut()
                .spawn((
                    FileView { path: path.clone() },
                    crate::host::editor::EditorBundle::new(
                        crate::edit::Document::new(
                            path.clone(),
                            "Rust".into(),
                            name,
                            crate::edit::EditMode::Normal,
                        ),
                        crate::edit::highlight_cache::HighlightCache::plain(&path),
                        crate::fold::FoldState::default(),
                    ),
                ))
                .id();
            views.push(entity);
        }
        app.world_mut().entity_mut(views[0]).insert(LspEditDirty);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(100));
        app.update();
        assert!(
            app.world_mut()
                .resource_mut::<Messages<vmux_lsp::ChangeDocument>>()
                .drain()
                .next()
                .is_none()
        );
        app.world_mut().entity_mut(views[1]).insert(LspEditDirty);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(50));
        app.update();
        let changes: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_lsp::ChangeDocument>>()
            .drain()
            .collect();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].owner, views[0]);
        assert_eq!(changes[0].text, "first");
        assert!(app.world().get::<LspChangeElapsed>(views[0]).is_none());
        assert!(app.world().get::<LspEditDirty>(views[1]).is_some());
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(100));
        app.update();
        let changes: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_lsp::ChangeDocument>>()
            .drain()
            .collect();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].owner, views[1]);
        assert_eq!(changes[0].text, "second");
        assert!(app.world().get::<LspChangeElapsed>(views[1]).is_none());
    }

    #[test]
    fn rename_prefill_is_the_identifier_around_the_caret() {
        let text = "let some_name = 1;";

        assert_eq!(EditReadOnlyItem::word_at(text, 8), "some_name");
        assert_eq!(EditReadOnlyItem::word_at(text, 4), "some_name");
        assert_eq!(EditReadOnlyItem::word_at(text, 13), "some_name");
        assert_eq!(EditReadOnlyItem::word_at(text, 14), "");
    }
}
