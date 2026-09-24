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
static CAPTURE_TARGET: std::sync::Mutex<Option<ShortcutCaptureToken>> = std::sync::Mutex::new(None);

#[cfg(host)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutCaptureToken {
    pub target: bevy::prelude::Entity,
    pub generation: u64,
}

#[cfg(host)]
pub fn capture_target() -> Option<ShortcutCaptureToken> {
    *CAPTURE_TARGET
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(host)]
pub fn release_capture(token: ShortcutCaptureToken) -> bool {
    let mut current = CAPTURE_TARGET
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *current != Some(token) {
        return false;
    }
    *current = None;
    true
}

#[cfg(host)]
fn set_capture_target(target: Option<ShortcutCaptureToken>) {
    *CAPTURE_TARGET
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = target;
}

#[vmux_api::ui_event(Copy, Default, Eq, target = "shortcuts")]
pub struct ShortcutCaptureEvent {
    pub active: bool,
}

#[vmux_api::host_event(Copy, Default, Eq, target = "shortcuts")]
pub struct ShortcutCaptureStateEvent {
    pub active: bool,
}

#[vmux_api::host_event(Default, Eq, target = "shortcuts")]
pub struct ShortcutPressedEvent {
    pub stroke: ShortcutStroke,
    pub pressed_at_ms: i64,
}

#[vmux_api::host_event(Default, Eq, target = "shortcuts")]
pub struct ShortcutsEvent {
    pub groups: Vec<ShortcutGroup>,
    pub chord_timeout_ms: u64,
}

impl ShortcutsEvent {
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
pub use host::{ShortcutCaptureTarget, ShortcutPlugin};

#[cfg(ui)]
pub mod page;
