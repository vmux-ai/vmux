//! The file page's keyboard, on the far side of the keymap.
//!
//! Two keyboards read this page, and the split between them is the whole design. The *app* keymap
//! owns the explorer toggle and the verbs of whichever panel is open; it lives in `settings.json`,
//! is resolved on the host, and comes back as a [`FileKey`]. The *modal* keymap — vim or vscode —
//! owns everything that moves a caret or changes text; it is a different concern with its own
//! modes, counts and operators, and it stays where it is.
//!
//! Both read the same keystroke, so something has to say who wins. That is not decided here: the
//! page hands a key over and the host arbitrates once, in `ScopedKeys`, by asking whether a
//! *context-scoped* binding matched. Keeping the two families disjoint is what makes the answer
//! boring — the app keymap never binds a text-editing key, so `Escape` closing a panel and
//! `Escape` leaving insert mode are two presses in two contexts, never one press meaning both.
//!
//! Three things stay on this side, because the keymap cannot answer them in time or at all.
//!
//! Which panel a verb lands in is the page's, decided in the tick the key arrives. The completion
//! list is filtered by the prefix under the caret, and the caret moves on keystrokes the host has
//! not seen yet, so a row index resolved there would commit the wrong completion.
//!
//! Typing stays with the browser, because the page is [`Unclaimed::Types`]: a key nobody claimed
//! is never sent from here, so caret motion, selection, undo and IME keep working in the
//! `<textarea>` without a round trip. There is no `wanted_locally` question to ask — the app
//! keymap binds no key the caret has an opinion about, so there is nothing for the caret to
//! contradict. What the browser would have thrown away, [`FileKeys::offer`] leaves to the caller,
//! which is where the modal forward still happens.
//!
//! `j` and `k` in the references panel stay local, and cannot do otherwise: the core deliberately
//! never claims a printable key pressed alone, because a claim set one context behind would swallow
//! a character. The arrow keys beside them are rebindable; these two aliases are not.

use crate::page::{Mode, focus_file_input, reveal_current_in_explorer, toggle_explorer};
use dioxus::prelude::*;
use vmux_core::event::{
    CompletionItem, FILE_KEY_EVENT, FileCompletionCommit, FileGotoRequest, FileKey, FileLine,
    RefItem,
};
use vmux_core::input::{PageKeyContext, Unclaimed};
use vmux_ui::hooks::{KeyClaim, MenuDirection, move_selection, send, use_key_claim, use_listener};

/// Subscribe the page to the keyboard seam and start listening for what its keys turned out to
/// mean.
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

/// One file page's keyboard: what the core claimed from it, and what it does with the answer.
///
/// `Copy`, and reached through a context rather than a prop, because the explorer sidebar is a
/// component of its own and [`KeyClaim`] is not comparable — a prop would have to be.
#[derive(Clone, Copy)]
pub struct FileKeys {
    page: FilePage,
    claim: KeyClaim,
}

impl FileKeys {
    /// Offer one `keydown` to the app keymap.
    ///
    /// Returns whether the key was taken, so a caller with local handling of its own — the modal
    /// forward, the directory browser, a bare `j` — can run it on exactly the presses the keymap
    /// did not want. Asked first in every handler, because the surfaces below this one prevent the
    /// default on keys they act on and the claim would never see them otherwise.
    pub fn offer(&self, event: &Event<KeyboardData>) -> bool {
        self.claim.on_keydown(event, |_| false);
        !event.default_action_enabled()
    }

    /// Take the host's answers about keys this page handed over.
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

/// Which of the page's two panels a panel verb lands in.
///
/// One type rather than a branch per key, because the two differ only in where their rows come
/// from — and because "which panel is showing" is the one thing about the file keyboard that
/// genuinely belongs to the page. References outrank the completion popup because opening the
/// references list takes focus off the buffer the popup is anchored to.
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

    /// Commit the highlighted row. A row that is no longer there does nothing: the completion list
    /// is filtered by the prefix under the caret, so a stored index can outlive what it pointed at.
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

/// The slice of the file page a key verb can act on.
///
/// Every field is a signal the page already owns; nothing is duplicated here. It exists so the
/// keyboard has one value to hang off rather than the fifteen arguments the explorer shortcut and
/// the four inline key tables used to take between them.
#[derive(Clone, Copy)]
pub struct FilePage {
    pub mode: Signal<Mode>,
    pub explorer_visible: Signal<bool>,
    pub explorer_preferred_visible: Signal<bool>,
    pub explorer_width: Signal<u32>,
    pub explorer_client_id: Signal<u64>,
    pub explorer_request_id: Signal<u64>,
    pub completion_open: Signal<bool>,
    pub completion_selection: Signal<usize>,
    pub completion_anchor: Signal<(u32, u32)>,
    /// The rows the popup is showing, as a memo so that a caret move which does not change the
    /// list does not republish the page's context. The unfiltered list would: it is derived from
    /// the viewport, which is replaced on every scroll.
    pub completions: Memo<Vec<CompletionItem>>,
    pub references_open: Signal<bool>,
    pub reference_selection: Signal<usize>,
    pub references: Signal<Vec<RefItem>>,
}

impl FilePage {
    /// What is true of this page now, as the context keys a binding's `when` is resolved against.
    ///
    /// Read reactively — every signal touched here becomes a trigger to republish — so the page
    /// says what it is rather than remembering to announce a change. One key for both panels: the
    /// keymap has no reason to know there are two, and [`FilePanel::of`] decides which is meant in
    /// the tick the key arrives.
    fn key_context(&self) -> Vec<String> {
        let mut keys = vec!["files".to_string()];
        if FilePanel::of(*self).is_some() {
            keys.push("files.panel".to_string());
        }
        keys
    }

    /// The highlighted row, kept inside a list that may have shrunk under it.
    fn clamped_selection(&self, panel: FilePanel) -> usize {
        let length = panel.len(*self);
        (*panel.selection(*self).peek()).min(length.saturating_sub(1))
    }

    fn reference(&self, index: usize) -> Option<RefItem> {
        self.references.peek().get(index).cloned()
    }

    fn toggle_explorer(&self) {
        toggle_explorer(
            self.explorer_visible,
            self.explorer_preferred_visible,
            self.explorer_width,
            self.explorer_client_id,
            self.explorer_request_id,
            self.mode,
        );
    }

    fn reveal_in_explorer(&self) {
        reveal_current_in_explorer(
            self.explorer_visible,
            self.explorer_preferred_visible,
            self.explorer_width,
            self.explorer_client_id,
            self.explorer_request_id,
            self.mode,
        );
    }
}

/// The completion popup's rows, and the caret that filters them.
///
/// A type rather than a block inside the page because two readers have to agree on the answer: the
/// popup draws these rows, and a panel verb commits the highlighted one. A second copy of the
/// filter would let the row the user is looking at and the row that gets inserted disagree.
#[derive(Clone, Copy)]
pub struct Completions {
    pub open: Signal<bool>,
    pub anchor: Signal<(u32, u32)>,
    pub items: Signal<Vec<CompletionItem>>,
    pub lines: Signal<Vec<FileLine>>,
    pub cursor: Signal<vmux_core::editor::CursorPos>,
}

impl Completions {
    /// The rows still matching the prefix between the popup's anchor and the caret.
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
