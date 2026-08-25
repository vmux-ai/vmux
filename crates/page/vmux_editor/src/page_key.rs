use crate::page::{ExplorerPane, Mode, focus_file_input};
use dioxus::prelude::*;
use vmux_core::event::{
    CompletionItem, FILE_KEY_EVENT, FileCompletionCommit, FileGotoRequest, FileKey, FileLine,
    RefItem,
};
use vmux_core::input::{PageKeyContext, Unclaimed};
use vmux_ui::hooks::{KeyClaim, MenuDirection, move_selection, send, use_key_claim, use_listener};

pub fn use_file_keys(page: FilePage) -> FileKeys {
    let keys = FileKeys {
        page,
        claim: use_key_claim(Unclaimed::Types, move || page.key_context()),
    };
    keys.listen();
    use_drop(move || {
        let _ = send(&PageKeyContext { keys: Vec::new() });
    });
    keys
}

#[derive(Clone, Copy)]
pub struct FileKeys {
    page: FilePage,
    claim: KeyClaim,
}

impl FileKeys {
    pub fn offer(&self, event: &Event<KeyboardData>) -> bool {
        self.claim.on_keydown(event, |_| false);
        !event.default_action_enabled()
    }

    fn listen(&self) {
        let keys = *self;
        let _resolved = use_listener::<FileKey, _>(FILE_KEY_EVENT, move |key| keys.apply(key));
    }

    fn apply(&self, key: FileKey) {
        match key {
            FileKey::ToggleExplorer => self.page.toggle_explorer(),
            FileKey::RevealInExplorer => self.page.reveal_in_explorer(),
            FileKey::PanelNext => self.move_panel(MenuDirection::Next),
            FileKey::PanelPrevious => self.move_panel(MenuDirection::Previous),
            FileKey::PanelChoose => self.choose(),
            FileKey::PanelDismiss => self.dismiss(),
        }
    }

    fn move_panel(&self, direction: MenuDirection) {
        let Some(panel) = FilePanel::of(self.page) else {
            return;
        };
        panel.move_by(self.page, direction);
    }

    fn choose(&self) {
        let Some(panel) = FilePanel::of(self.page) else {
            return;
        };
        panel.choose(self.page);
    }

    fn dismiss(&self) {
        let Some(panel) = FilePanel::of(self.page) else {
            return;
        };
        panel.dismiss(self.page);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FilePanel {
    References,
    Completion,
}

impl FilePanel {
    fn of(page: FilePage) -> Option<Self> {
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
pub struct FilePage {
    pub mode: Signal<Mode>,
    pub explorer: ExplorerPane,
    pub completion_open: Signal<bool>,
    pub completion_selection: Signal<usize>,
    pub completion_anchor: Signal<(u32, u32)>,
    pub completions: Memo<Vec<CompletionItem>>,
    pub references_open: Signal<bool>,
    pub reference_selection: Signal<usize>,
    pub references: Signal<Vec<RefItem>>,
}

impl FilePage {
    fn key_context(&self) -> Vec<String> {
        let mut keys = vec!["files".to_string()];
        if FilePanel::of(*self).is_some() {
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
        self.explorer.reveal_current(self.mode);
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
