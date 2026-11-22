use bevy::prelude::Component;

use crate::edit::command::{EditCommand, EditMode};
pub use vmux_api::editor::KeymapKind;

pub mod mapping;
pub mod vim;
pub mod vscode;

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

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Component)]
pub struct EditorKeymap(pub KeymapKind);

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct KeymapConfig {
    kind: KeymapKind,
    maps: Vec<vmux_api::editor::KeyMapping>,
    leader: String,
}

impl KeymapConfig {
    pub(super) fn resolve(settings: Option<&vmux_setting::AppSettings>) -> Self {
        let Some(settings) = settings else {
            return Self {
                kind: KeymapKind::default(),
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
        EditorKeymap(self.kind)
    }

    pub(super) fn parser(&self) -> vim::VimKeymap {
        let mut parser = vim::VimKeymap::with_mappings(&self.maps, &self.leader);
        parser.applied_config = Some(self.clone());
        parser
    }

    pub(super) fn initial_mode(&self) -> vmux_api::editor::EditMode {
        match self.kind {
            KeymapKind::Vscode => EditMode::Insert,
            KeymapKind::Vim => EditMode::Normal,
        }
    }

    pub(super) fn kind(&self) -> KeymapKind {
        self.kind
    }
}
