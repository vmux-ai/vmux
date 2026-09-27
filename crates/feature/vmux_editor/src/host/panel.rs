use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_core::event::{
    CompletionItem, FilePanelContent, FilePanelFocus, FilePanelFocusTarget, FilePanelPick,
    FilePanelState, RefItem,
};

use crate::edit::{EditCommand, Selection};
use crate::host::editing::EditRequest;
use crate::host::editor::{Editor, FileView};

pub(super) struct PanelPlugin;

impl Plugin for PanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(FilePanelPick,)>::default())
            .add_observer(receive_completion)
            .add_observer(receive_references)
            .add_observer(apply_panel_request)
            .add_observer(pick_panel_item)
            .add_systems(PostUpdate, (clear_navigated_panels, refresh_changed_panels));
    }
}

#[derive(Clone)]
struct CompletionSource {
    items: Vec<CompletionItem>,
    replace_from_col: u32,
    line: u32,
}

impl CompletionSource {
    fn content(&self, edit: &Editor) -> Option<FilePanelContent> {
        let head = edit.core.primary().head;
        let (line, caret) = edit.core.buffer.char_to_coords(head);
        if line as u32 != self.line {
            return None;
        }
        let line_text = edit
            .core
            .buffer
            .rope
            .line(line)
            .chars()
            .filter(|character| *character != '\n' && *character != '\r')
            .collect::<Vec<_>>();
        let from = self.replace_from_col as usize;
        if from > caret || from > line_text.len() {
            return None;
        }
        let prefix = line_text[from..caret.min(line_text.len())]
            .iter()
            .collect::<String>()
            .to_lowercase();
        let mut items = Vec::new();
        for item in &self.items {
            if item.label.to_lowercase().starts_with(&prefix) {
                items.push(item.clone());
            }
        }
        if items.is_empty() {
            return None;
        }
        Some(FilePanelContent::Completion {
            items,
            replace_from_col: self.replace_from_col,
            line: self.line,
        })
    }
}

#[derive(Default)]
enum FilePanelSource {
    #[default]
    None,
    References,
    Completion(CompletionSource),
}

#[derive(Component, Default)]
pub(super) struct FilePanel {
    source: FilePanelSource,
    state: FilePanelState,
}

impl FilePanel {
    fn show_references(&mut self, items: Vec<RefItem>) {
        let returning_focus =
            matches!(&self.source, FilePanelSource::References) && self.state.content.is_some();
        self.source = FilePanelSource::References;
        self.state.content = (!items.is_empty()).then_some(FilePanelContent::References { items });
        self.state.selected = 0;
        if self.state.content.is_some() {
            self.focus(FilePanelFocusTarget::References);
        } else if returning_focus {
            self.focus(FilePanelFocusTarget::Editor);
        }
    }

    fn show_completion(
        &mut self,
        items: Vec<CompletionItem>,
        replace_from_col: u32,
        line: u32,
        edit: &Editor,
    ) {
        let returning_focus =
            matches!(&self.source, FilePanelSource::References) && self.state.content.is_some();
        self.source = FilePanelSource::Completion(CompletionSource {
            items,
            replace_from_col,
            line,
        });
        self.state.selected = 0;
        self.refresh(edit);
        if returning_focus {
            self.focus(FilePanelFocusTarget::Editor);
        }
    }

    fn refresh(&mut self, edit: &Editor) -> bool {
        let FilePanelSource::Completion(source) = &self.source else {
            return false;
        };
        let content = source.content(edit);
        let last_index = match content.as_ref() {
            Some(FilePanelContent::References { items }) => items.len().saturating_sub(1) as u32,
            Some(FilePanelContent::Completion { items, .. }) => {
                items.len().saturating_sub(1) as u32
            }
            None => 0,
        };
        let selected = self.state.selected.min(last_index);
        if self.state.content == content && self.state.selected == selected {
            return false;
        }
        self.state.content = content;
        self.state.selected = selected;
        true
    }

    fn move_selection(&mut self, movement: FilePanelMovement) -> bool {
        let Some(content) = self.state.content.as_ref() else {
            return false;
        };
        let len = match content {
            FilePanelContent::References { items } => items.len(),
            FilePanelContent::Completion { items, .. } => items.len(),
        };
        if len == 0 {
            return false;
        }
        let selected = match movement {
            FilePanelMovement::Next => (self.state.selected as usize + 1) % len,
            FilePanelMovement::Previous => (self.state.selected as usize + len - 1) % len,
        } as u32;
        if selected == self.state.selected {
            return false;
        }
        self.state.selected = selected;
        true
    }

    fn choice(&mut self, index: Option<u32>) -> Option<FilePanelChoice> {
        let content = self.state.content.as_ref()?;
        let selected = index.unwrap_or(self.state.selected) as usize;
        let choice = match content {
            FilePanelContent::References { items } => {
                FilePanelChoice::Reference(items.get(selected)?.clone())
            }
            FilePanelContent::Completion {
                items,
                replace_from_col,
                line,
            } => FilePanelChoice::Completion {
                item: items.get(selected)?.clone(),
                replace_from_col: *replace_from_col,
                line: *line,
            },
        };
        self.dismiss();
        Some(choice)
    }

    fn dismiss(&mut self) -> bool {
        if self.state.content.is_none() && matches!(&self.source, FilePanelSource::None) {
            return false;
        }
        let focused_references = matches!(self.source, FilePanelSource::References);
        self.source = FilePanelSource::None;
        self.state.content = None;
        self.state.selected = 0;
        if focused_references {
            self.focus(FilePanelFocusTarget::Editor);
        }
        true
    }

    fn reset(&mut self) -> bool {
        if self.state.content.is_none() && matches!(&self.source, FilePanelSource::None) {
            return false;
        }
        self.source = FilePanelSource::None;
        self.state.content = None;
        self.state.selected = 0;
        true
    }

    fn focus(&mut self, target: FilePanelFocusTarget) {
        self.state.focus = FilePanelFocus {
            revision: self.state.focus.revision.wrapping_add(1).max(1),
            target,
        };
    }

    fn state(&self) -> FilePanelState {
        self.state.clone()
    }
}

enum FilePanelChoice {
    Reference(RefItem),
    Completion {
        item: CompletionItem,
        replace_from_col: u32,
        line: u32,
    },
}

#[derive(Clone, Copy)]
pub(super) enum FilePanelMovement {
    Next,
    Previous,
}

#[derive(Clone, Copy)]
pub(super) enum FilePanelOperation {
    Move(FilePanelMovement),
    Choose,
    ChooseAt(u32),
    Dismiss,
}

#[derive(EntityEvent)]
pub(super) struct FilePanelRequest {
    #[event_target]
    entity: Entity,
    operation: FilePanelOperation,
}

impl FilePanelRequest {
    pub(super) fn new(entity: Entity, operation: FilePanelOperation) -> Self {
        Self { entity, operation }
    }
}

#[derive(EntityEvent)]
pub(super) struct CompletionResult {
    #[event_target]
    entity: Entity,
    items: Vec<CompletionItem>,
    replace_from_col: u32,
    line: u32,
}

impl CompletionResult {
    pub(super) fn new(
        entity: Entity,
        items: Vec<CompletionItem>,
        replace_from_col: u32,
        line: u32,
    ) -> Self {
        Self {
            entity,
            items,
            replace_from_col,
            line,
        }
    }
}

#[derive(EntityEvent)]
pub(super) struct ReferencesResult {
    #[event_target]
    entity: Entity,
    items: Vec<RefItem>,
}

impl ReferencesResult {
    pub(super) fn new(entity: Entity, items: Vec<RefItem>) -> Self {
        Self { entity, items }
    }
}

fn receive_completion(
    trigger: On<CompletionResult>,
    mut panels: Query<(&Editor, &mut FilePanel)>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok((edit, mut panel)) = panels.get_mut(trigger.event_target()) else {
        return;
    };
    panel.show_completion(
        event.items.clone(),
        event.replace_from_col,
        event.line,
        edit,
    );
    commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
        trigger.event_target(),
        &panel.state(),
    ));
}

fn receive_references(
    trigger: On<ReferencesResult>,
    mut panels: Query<&mut FilePanel>,
    mut commands: Commands,
) {
    let Ok(mut panel) = panels.get_mut(trigger.event_target()) else {
        return;
    };
    panel.show_references(trigger.event().items.clone());
    commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
        trigger.event_target(),
        &panel.state(),
    ));
}

fn refresh_changed_panels(
    mut panels: Query<(Entity, &Editor, &mut FilePanel), Changed<Editor>>,
    mut commands: Commands,
) {
    for (entity, edit, mut panel) in &mut panels {
        if !panel.refresh(edit) {
            continue;
        }
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
            entity,
            &panel.state(),
        ));
    }
}

fn clear_navigated_panels(
    mut panels: Query<(Entity, &mut FilePanel), Changed<FileView>>,
    mut commands: Commands,
) {
    for (entity, mut panel) in &mut panels {
        if !panel.reset() {
            continue;
        }
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
            entity,
            &panel.state(),
        ));
    }
}

fn pick_panel_item(trigger: On<UiInput<FilePanelPick>>, mut commands: Commands) {
    commands.trigger(FilePanelRequest::new(
        trigger.event().webview,
        FilePanelOperation::ChooseAt(trigger.event().payload.index),
    ));
}

fn apply_panel_request(
    trigger: On<FilePanelRequest>,
    mut panels: Query<(&mut FilePanel, &mut Editor)>,
    mut goto: MessageWriter<crate::lsp::manager::LspGoto>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok((mut panel, mut edit)) = panels.get_mut(entity) else {
        return;
    };
    let mut changed = false;
    let choice = match trigger.event().operation {
        FilePanelOperation::Move(movement) => {
            changed = panel.move_selection(movement);
            None
        }
        FilePanelOperation::Choose => panel.choice(None),
        FilePanelOperation::ChooseAt(index) => panel.choice(Some(index)),
        FilePanelOperation::Dismiss => {
            changed = panel.dismiss();
            None
        }
    };
    if choice.is_some() {
        changed = true;
    }
    let state = changed.then(|| panel.state());
    match choice {
        Some(FilePanelChoice::Reference(item)) => {
            let path = PathBuf::from(item.path);
            let line_text = crate::lsp::manager::disk_line(&path, item.line);
            goto.write(crate::lsp::manager::LspGoto {
                entity,
                path,
                line: item.line,
                utf16_col: crate::lsp::manager::char_to_utf16_col(&line_text, item.col),
            });
        }
        Some(FilePanelChoice::Completion {
            item,
            replace_from_col,
            line,
        }) => {
            let start = edit
                .core
                .buffer
                .coords_to_char(line as usize, replace_from_col as usize);
            let head = edit.core.primary().head;
            edit.core.selections = vec![Selection {
                anchor: start.min(head),
                head: start.max(head),
            }];
            commands.trigger(EditRequest::new(
                entity,
                vec![EditCommand::InsertText(item.insert_text)],
            ));
        }
        None => {}
    }
    if let Some(state) = state {
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
            entity, &state,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{EditCore, EditMode};
    use crate::host::edit::highlight_cache::HighlightCache;

    struct CompletionFixture {
        edit: Editor,
        panel: FilePanel,
    }

    impl CompletionFixture {
        fn new(text: &str, caret: usize) -> Self {
            let path = PathBuf::from("completion.rs");
            let mut core = EditCore::new(path.clone(), "Rust".into(), text, EditMode::Insert);
            core.set_caret(caret);
            Self {
                edit: Editor::new(
                    core,
                    HighlightCache::new(&path),
                    crate::fold::FoldState::default(),
                ),
                panel: FilePanel::default(),
            }
        }

        fn show(&mut self) {
            self.panel.show_completion(
                vec![
                    CompletionItem {
                        label: "print".into(),
                        insert_text: "print".into(),
                        detail: String::new(),
                        kind: String::new(),
                    },
                    CompletionItem {
                        label: "println".into(),
                        insert_text: "println".into(),
                        detail: String::new(),
                        kind: String::new(),
                    },
                    CompletionItem {
                        label: "panic".into(),
                        insert_text: "panic".into(),
                        detail: String::new(),
                        kind: String::new(),
                    },
                ],
                0,
                0,
                &self.edit,
            );
        }
    }

    #[test]
    fn completion_projection_follows_the_host_buffer_and_caret() {
        let mut fixture = CompletionFixture::new("pri", 3);
        fixture.show();

        let Some(FilePanelContent::Completion { items, .. }) = &fixture.panel.state.content else {
            panic!("completion panel is closed");
        };
        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            ["print", "println"]
        );

        fixture.edit.core.set_caret(1);
        assert!(fixture.panel.refresh(&fixture.edit));
        let Some(FilePanelContent::Completion { items, .. }) = &fixture.panel.state.content else {
            panic!("completion panel is closed");
        };
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn panel_selection_wraps_and_choice_closes_the_panel() {
        let mut fixture = CompletionFixture::new("p", 1);
        fixture.show();

        assert!(fixture.panel.move_selection(FilePanelMovement::Previous));
        assert_eq!(fixture.panel.state.selected, 2);
        assert!(matches!(
            fixture.panel.choice(None),
            Some(FilePanelChoice::Completion { item, .. }) if item.label == "panic"
        ));
        assert!(fixture.panel.state.content.is_none());
    }
}
