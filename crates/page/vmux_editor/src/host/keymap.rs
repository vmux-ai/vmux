use bevy::prelude::Component;

pub mod mapping;
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

impl KeyInput {
    pub fn lsp_action(&self) -> Option<EditCommand> {
        match self.key.as_str() {
            "F12" if self.mods.shift => Some(EditCommand::FindReferences),
            "F12" => Some(EditCommand::GotoDefinition),
            "F2" => Some(EditCommand::BeginRename),
            _ => None,
        }
    }
}

pub trait Keymap: Send + Sync {
    fn handle(&mut self, k: &KeyInput) -> Vec<EditCommand>;
    fn mode(&self) -> EditMode;
    fn record_text(&mut self, _text: &str) {}

    fn pointer_selection_mode(&mut self, _extend: bool) -> Option<EditCommand> {
        None
    }

    fn mode_label(&self) -> String {
        self.mode().label().to_string()
    }
}

pub trait KeymapKindExt {
    fn make(self, mappings: &[vmux_core::editor::KeyMapping], leader: &str) -> Box<dyn Keymap>;
    fn initial_mode(self) -> EditMode;
}

impl KeymapKindExt for KeymapKind {
    fn make(self, mappings: &[vmux_core::editor::KeyMapping], leader: &str) -> Box<dyn Keymap> {
        match self {
            KeymapKind::Vscode => Box::new(vscode::VscodeKeymap),
            KeymapKind::Vim => Box::new(vim::VimKeymap::with_mappings(mappings, leader)),
        }
    }

    fn initial_mode(self) -> EditMode {
        match self {
            KeymapKind::Vscode => EditMode::Insert,
            KeymapKind::Vim => EditMode::Normal,
        }
    }
}

#[derive(Component)]
pub struct EditorKeymap(pub Box<dyn Keymap>);

#[derive(PartialEq, Eq)]
pub(super) struct KeymapConfig {
    kind: vmux_core::KeymapKind,
    maps: Vec<vmux_core::editor::KeyMapping>,
    leader: String,
}

impl KeymapConfig {
    pub(super) fn resolve(settings: Option<&vmux_setting::AppSettings>) -> Self {
        let Some(settings) = settings else {
            return Self {
                kind: vmux_core::KeymapKind::default(),
                maps: Vec::new(),
                leader: " ".to_string(),
            };
        };
        Self {
            kind: settings.editor.keymap,
            maps: settings.editor.mappings.clone(),
            leader: settings.editor.leader.clone(),
        }
    }

    pub(super) fn keymap(&self) -> EditorKeymap {
        EditorKeymap(self.kind.make(&self.maps, &self.leader))
    }

    pub(super) fn initial_mode(&self) -> vmux_core::EditMode {
        self.kind.initial_mode()
    }

    pub(super) fn kind(&self) -> vmux_core::KeymapKind {
        self.kind
    }
}
