pub mod vim;
pub mod vscode;

pub use vmux_core::KeymapKind;

use crate::edit::command::{EditCommand, EditMode};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl Mods {
    pub fn cmd(&self) -> bool {
        self.meta || self.ctrl
    }
    pub fn word(&self) -> bool {
        self.alt || self.ctrl
    }
}

#[derive(Clone, Debug)]
pub struct KeyInput {
    pub key: String,
    pub mods: Mods,
    pub repeat: bool,
}

pub trait Keymap: Send + Sync {
    fn handle(&mut self, k: &KeyInput) -> Vec<EditCommand>;
    fn mode(&self) -> EditMode;
    /// Report text that reached the buffer without passing through [`Keymap::handle`].
    ///
    /// Insert-mode characters arrive over the IME path, so a keymap that replays input — for
    /// dot-repeat or macros — only sees a whole change if the host reports them here.
    fn record_text(&mut self, _text: &str) {}
    /// The `:`, `/`, or `?` prompt currently being typed, prompt character included.
    fn command_line(&self) -> Option<String> {
        None
    }
    fn pointer_selection_mode(&mut self, _extend: bool) -> Option<EditCommand> {
        None
    }
    fn mode_label(&self) -> String {
        self.mode().label().to_string()
    }
}

pub trait KeymapKindExt {
    fn make(self) -> Box<dyn Keymap>;
    fn initial_mode(self) -> EditMode;
}

impl KeymapKindExt for KeymapKind {
    fn make(self) -> Box<dyn Keymap> {
        match self {
            KeymapKind::Vscode => Box::new(vscode::VscodeKeymap),
            KeymapKind::Vim => Box::new(vim::VimKeymap::default()),
        }
    }
    fn initial_mode(self) -> EditMode {
        match self {
            KeymapKind::Vscode => EditMode::Insert,
            KeymapKind::Vim => EditMode::Normal,
        }
    }
}
