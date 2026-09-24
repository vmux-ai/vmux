use crate::explorer::{SEARCH_INPUT_ID, SidebarView};
use crate::ui::{ExplorerPane, Mode, focus_file_input};
use dioxus::prelude::*;
use vmux_core::event::{
    CompletionItem, FileCompletionCommit, FileGotoRequest, FileKey, FileLine, RefItem,
};
use vmux_core::input::{PageKeyContext, Unclaimed};
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::{KeyClaim, MenuDirection, move_selection, send, use_key_claim};
use vmux_ui::platform::sleep_ms;

pub(crate) fn use_file_keys(page: FilePage) -> FileKeys {
    let actions = FileKeyActions(page);
    let events = crate::state::use_file_ui::<FileKey>();
    use_effect(move || events.for_each(|key| actions.apply(key)));
    let keys = FileKeys {
        claim: use_key_claim(Unclaimed::Types, move || page.key_context()),
    };
    use_drop(move || {
        let _ = send(&PageKeyContext { keys: Vec::new() });
    });
    keys
}

#[derive(Clone, Copy)]
pub struct FileKeys {
    claim: KeyClaim,
}

impl FileKeys {
    pub fn offer(&self, event: &Event<KeyboardData>) -> bool {
        self.claim.on_keydown(event, |_| false);
        !event.default_action_enabled()
    }
}

#[derive(Clone, Copy)]
struct FileKeyActions(FilePage);

impl FileKeyActions {
    fn apply(&self, key: FileKey) {
        match key {
            FileKey::ToggleExplorer => self.0.toggle_explorer(),
            FileKey::RevealInExplorer => self.0.reveal_in_explorer(),
            FileKey::PanelNext => self.move_panel(MenuDirection::Next),
            FileKey::PanelPrevious => self.move_panel(MenuDirection::Previous),
            FileKey::PanelChoose => self.choose(),
            FileKey::PanelDismiss => self.dismiss(),
            FileKey::Find { forward } => self.0.open_find(forward),
            FileKey::FindClose => self.0.close_find(),
            FileKey::FindInFiles => self.0.open_find_in_files(),
        }
    }

    fn move_panel(&self, direction: MenuDirection) {
        let Some(panel) = FilePanel::current(self.0) else {
            return;
        };
        panel.move_by(self.0, direction);
    }

    fn choose(&self) {
        let Some(panel) = FilePanel::current(self.0) else {
            return;
        };
        panel.choose(self.0);
    }

    fn dismiss(&self) {
        let Some(panel) = FilePanel::current(self.0) else {
            return;
        };
        panel.dismiss(self.0);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FilePanel {
    References,
    Completion,
}

impl FilePanel {
    fn current(page: FilePage) -> Option<Self> {
        if (page.references_open)() {
            return Some(Self::References);
        }
        if !(page.completions)().is_empty() {
            return Some(Self::Completion);
        }
        None
    }

    fn len(self, page: FilePage) -> usize {
        match self {
            Self::References => page.references.read().len(),
            Self::Completion => page.completions.read().len(),
        }
    }

    fn selection(self, page: FilePage) -> Signal<usize> {
        match self {
            Self::References => page.reference_selection,
            Self::Completion => page.completion_selection,
        }
    }

    fn move_by(self, page: FilePage, direction: MenuDirection) {
        let mut selection = self.selection(page);
        let landed = move_selection(*selection.peek(), self.len(page), direction);
        selection.set(landed);
    }

    fn choose(self, page: FilePage) {
        match self {
            Self::References => {
                let Some(item) = page.reference(page.clamped_selection(self)) else {
                    return;
                };
                let _ = send(&FileGotoRequest {
                    path: item.path,
                    line: item.line,
                    col: item.col,
                });
                self.dismiss(page);
            }
            Self::Completion => {
                let index = page.clamped_selection(self);
                if let Some(item) = page.completions.peek().get(index) {
                    let (line, replace_from_col) = (page.completion_anchor)();
                    let _ = send(&FileCompletionCommit {
                        line,
                        replace_from_col,
                        text: item.insert_text.clone(),
                    });
                }
                let mut open = page.completion_open;
                open.set(false);
            }
        }
    }

    fn dismiss(self, page: FilePage) {
        match self {
            Self::References => {
                let mut open = page.references_open;
                open.set(false);
                focus_file_input();
            }
            Self::Completion => {
                let mut open = page.completion_open;
                open.set(false);
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FilePage {
    pub mode: Signal<Mode>,
    pub explorer: ExplorerPane,
    pub completion_open: Signal<bool>,
    pub completion_selection: Signal<usize>,
    pub completion_anchor: Signal<(u32, u32)>,
    pub completions: Memo<Vec<CompletionItem>>,
    pub references_open: Signal<bool>,
    pub reference_selection: Signal<usize>,
    pub references: Signal<Vec<RefItem>>,
    pub find_open: Signal<bool>,
    pub find_forward: Signal<bool>,
    pub sidebar_view: Signal<SidebarView>,
}

impl FilePage {
    fn key_context(&self) -> Vec<String> {
        let mut keys = vec!["files".to_string()];
        if FilePanel::current(*self).is_some() {
            keys.push("files.panel".to_string());
        }
        keys
    }

    fn clamped_selection(&self, panel: FilePanel) -> usize {
        let length = panel.len(*self);
        (*panel.selection(*self).peek()).min(length.saturating_sub(1))
    }

    fn reference(&self, index: usize) -> Option<RefItem> {
        self.references.peek().get(index).cloned()
    }

    fn toggle_explorer(&self) {
        self.explorer.toggle(self.mode);
    }

    fn reveal_in_explorer(&self) {
        let mut view = self.sidebar_view;
        view.set(SidebarView::Explorer);
        self.explorer.reveal_current(self.mode);
    }

    fn close_find(&self) {
        let mut open = self.find_open;
        open.set(false);
    }

    fn open_find(&self, forward: bool) {
        let mut open = self.find_open;
        let mut direction = self.find_forward;
        direction.set(forward);
        open.set(true);
        spawn(async move {
            sleep_ms(0).await;
            crate::ui::focus_find_input();
        });
    }

    fn open_find_in_files(&self) {
        let mut view = self.sidebar_view;
        view.set(SidebarView::Search);
        self.explorer.show(self.mode);
        spawn(async move {
            sleep_ms(0).await;
            FocusClaim::new(SEARCH_INPUT_ID).request();
        });
    }
}

#[derive(Clone, Copy)]
pub struct Completions {
    pub open: Signal<bool>,
    pub anchor: Signal<(u32, u32)>,
    pub items: Signal<Vec<CompletionItem>>,
    pub lines: Signal<Vec<FileLine>>,
    pub cursor: Signal<vmux_core::editor::CursorPos>,
}

impl Completions {
    pub fn matching(&self) -> Vec<CompletionItem> {
        if !(self.open)() {
            return Vec::new();
        }
        let (anchor_line, anchor_column) = (self.anchor)();
        let mut text = String::new();
        for line in (self.lines)().iter() {
            if line.line_no != anchor_line {
                continue;
            }
            for span in line.spans.iter() {
                text.push_str(&span.text);
            }
            break;
        }
        let characters: Vec<char> = text.chars().collect();
        let caret = (self.cursor)().col as usize;
        let from = anchor_column as usize;
        let mut prefix = String::new();
        if from <= caret && from <= characters.len() {
            for character in &characters[from..caret.min(characters.len())] {
                prefix.push(*character);
            }
        }
        let prefix = prefix.to_lowercase();
        let mut matching = Vec::new();
        for item in (self.items)() {
            if item.label.to_lowercase().starts_with(&prefix) {
                matching.push(item);
            }
        }
        matching
    }
}
