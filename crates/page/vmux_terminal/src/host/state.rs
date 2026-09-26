use std::time::Instant;

use bevy::prelude::*;
use vmux_command::shortcut::KeyCombo;

#[derive(Component, Default, Clone, Copy, Debug)]
pub(crate) struct TerminalMode {
    pub(crate) mouse_capture: bool,
    pub(crate) copy_mode: bool,
    pub(crate) alt_screen: bool,
    pub(crate) focus_reporting: bool,
}

impl TerminalMode {
    pub(crate) fn agent_ready(&self) -> bool {
        self.alt_screen || self.mouse_capture || self.focus_reporting
    }
}

#[derive(Component, Default)]
pub(crate) struct TerminalCopyMode {
    pub(crate) active: bool,
    pub(crate) input: CopyModeInputState,
}

impl TerminalCopyMode {
    pub(crate) fn set(&mut self, active: bool) {
        self.active = active;
        if !active {
            self.input = CopyModeInputState::default();
        }
    }
}

#[derive(Default)]
pub(crate) struct CopyModeInputState {
    pub(crate) pending_key: Option<CopyModePendingKey>,
    pub(crate) count: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyModePendingKey {
    G,
    FindForward,
    FindBackward,
    TillForward,
    TillBackward,
}

#[derive(Component, Default)]
pub(crate) struct TerminalShortcutState {
    pub(crate) pending_prefix: Option<(KeyCombo, Instant)>,
}
