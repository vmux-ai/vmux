pub const PAGE_URL: &str = "vmux://shortcuts/";
pub struct ShortcutUrl;

impl ShortcutUrl {
    pub fn canonical(url: &str) -> Option<&'static str> {
        matches!(
            url.trim().trim_end_matches('/'),
            "vmux://shortcuts" | "vmux://cheatsheet" | "vmux://cheetsheet"
        )
        .then_some(PAGE_URL)
    }
}

#[cfg(host)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutCaptureToken {
    pub target: bevy::prelude::Entity,
    pub generation: u64,
}

#[vmux_api::ui_event(Copy, Default, Eq, target = "shortcuts")]
pub struct ShortcutCaptureRequest {
    pub active: bool,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub struct ShortcutCaptureState {
    pub active: bool,
}

#[vmux_api::contract(Default, Eq)]
pub struct ShortcutPressed {
    pub stroke: ShortcutStroke,
    pub pressed_at_ms: i64,
}

#[vmux_api::contract(Default, Eq)]
pub struct ShortcutCatalog {
    pub groups: Vec<ShortcutGroup>,
    pub chord_timeout_ms: u64,
}

impl ShortcutCatalog {
    pub fn bindings(&self) -> impl Iterator<Item = (&ShortcutEntry, &ShortcutBinding)> {
        self.groups.iter().flat_map(|group| {
            group.entries.iter().flat_map(|entry| {
                entry
                    .shortcuts
                    .iter()
                    .map(move |shortcut| (entry, shortcut))
            })
        })
    }

    pub fn resolutions(&self) -> impl Iterator<Item = (&ShortcutEntry, &ShortcutBinding)> {
        self.bindings().filter(|(_, shortcut)| shortcut.resolves)
    }
}

#[vmux_api::ui_state_patch]
pub enum ShortcutUiStatePatch {
    Catalog(ShortcutCatalog),
    Capture(ShortcutCaptureState),
    Pressed(ShortcutPressed),
}

#[vmux_api::ui_state(Default, target = "shortcuts")]
pub struct ShortcutUiState {
    pub sequence: u64,
    pub patches: Vec<ShortcutUiStatePatch>,
}

#[cfg(host)]
pub type ShortcutUiStateUpdates = vmux_core::host::UiStateUpdates<ShortcutUiState>;

#[vmux_api::contract(Default, Eq)]
pub struct ShortcutGroup {
    pub name: String,
    pub entries: Vec<ShortcutEntry>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ShortcutEntry {
    pub id: String,
    pub name: String,
    pub shortcuts: Vec<ShortcutBinding>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ShortcutBinding {
    pub label: String,
    pub strokes: Vec<ShortcutStroke>,
    pub resolves: bool,
    pub contexts: Vec<String>,
}

impl ShortcutBinding {
    pub fn starts_with(&self, sequence: &[ShortcutStroke]) -> bool {
        sequence.len() <= self.strokes.len()
            && self
                .strokes
                .iter()
                .zip(sequence)
                .all(|(expected, actual)| expected.same_press(actual))
    }

    pub fn matches(&self, sequence: &[ShortcutStroke]) -> bool {
        self.strokes.len() == sequence.len() && self.starts_with(sequence)
    }
}

#[vmux_api::contract(Default, Eq, Hash)]
pub struct ShortcutStroke {
    pub code: String,
    pub label: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
}

impl ShortcutStroke {
    pub fn keycaps(&self) -> Vec<String> {
        let mut keycaps = Vec::new();
        if self.ctrl {
            keycaps.push("⌃".to_string());
        }
        if self.alt {
            keycaps.push("⌥".to_string());
        }
        if self.shift {
            keycaps.push("⇧".to_string());
        }
        if self.super_key {
            keycaps.push("⌘".to_string());
        }
        keycaps.push(self.label.clone());
        keycaps
    }

    pub fn same_press(&self, other: &Self) -> bool {
        self.code == other.code
            && self.ctrl == other.ctrl
            && self.shift == other.shift
            && self.alt == other.alt
            && self.super_key == other.super_key
    }

    pub fn after(&self, prefix: &Self) -> Self {
        let mut stroke = self.clone();
        stroke.ctrl &= !prefix.ctrl;
        stroke.alt &= !prefix.alt;
        stroke.super_key &= !prefix.super_key;
        stroke
    }
}

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::{ShortcutCaptureSet, ShortcutCaptureTarget, ShortcutPlugin};

#[cfg(ui)]
pub mod native_page;
#[cfg(ui)]
pub mod ui;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_state_preserves_catalog_and_pressed_order() {
        let state = ShortcutUiState {
            sequence: 2,
            patches: vec![
                ShortcutCatalog::default().into(),
                ShortcutPressed {
                    stroke: ShortcutStroke {
                        code: "KeyK".to_string(),
                        label: "K".to_string(),
                        ctrl: true,
                        ..Default::default()
                    },
                    pressed_at_ms: 9,
                }
                .into(),
            ],
        };

        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&state).unwrap();
        let decoded = rkyv::from_bytes::<ShortcutUiState, rkyv::rancor::Error>(&bytes).unwrap();

        assert!(matches!(
            decoded.patches.as_slice(),
            [
                ShortcutUiStatePatch::Catalog(_),
                ShortcutUiStatePatch::Pressed(_)
            ]
        ));
    }
}
