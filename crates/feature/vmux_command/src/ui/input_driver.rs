use super::readline_driver::Edited;
use dioxus::prelude::*;
use vmux_api::command_bar::{CommandBarOpenEvent, OpenId};
use vmux_ui::dom::{DomCaret, DomSelection};
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::KeyClaim;

#[derive(Clone, Copy, PartialEq)]
pub struct PaletteInput {
    pub last_focus_open_id: Signal<OpenId>,
    pub last_host_input: Signal<(OpenId, u64)>,
}

impl PaletteInput {
    pub fn use_hook() -> Self {
        Self {
            last_focus_open_id: use_signal(|| OpenId(u64::MAX)),
            last_host_input: use_signal(|| (OpenId(u64::MAX), 0)),
        }
    }
    pub fn refocus(&mut self, open_id: OpenId) -> bool {
        if !open_id.should_refocus((self.last_focus_open_id)()) {
            return false;
        }
        self.last_focus_open_id.set(open_id);
        true
    }

    pub fn apply_host_input(&mut self, open_id: OpenId, revision: u64, input_id: &'static str) {
        if revision == 0 {
            return;
        }
        let (last_open_id, last_revision) = (self.last_host_input)();
        if last_open_id != open_id {
            self.last_host_input.set((open_id, revision));
            return;
        }
        if revision <= last_revision {
            return;
        }
        self.last_host_input.set((open_id, revision));
        DomCaret::in_field(input_id).to_end();
    }
}

pub struct TypedDigit;

impl TypedDigit {
    pub fn from_event(event: &KeyboardEvent) -> Option<usize> {
        let Key::Character(typed) = event.key() else {
            return None;
        };
        let character = typed.chars().next()?;
        if !character.is_ascii_digit() {
            return None;
        }
        let digit = character.to_digit(10)?;
        Some(digit as usize)
    }
}

pub struct CommandBarField;

impl CommandBarField {
    pub const INPUT_ID: &str = "command-bar-input";

    pub fn focus(opened: &CommandBarOpenEvent) {
        if opened.caret_at_end {
            FocusClaim::new(Self::INPUT_ID).caret_at_end().request();
            return;
        }
        FocusClaim::new(Self::INPUT_ID).request();
        DomCaret::in_field(Self::INPUT_ID).select_all_from_start_next_frame();
    }
}

pub struct Readline;

impl Readline {
    pub fn chord(
        event: &KeyboardEvent,
        keys: KeyClaim,
        query: &str,
        ghost: &str,
        input_id: &'static str,
    ) -> Option<String> {
        if Self::select_all(event, input_id) {
            return Some(query.to_string());
        }
        let command = keys.command(event)?;
        if !Edited::handles(&command) {
            return None;
        }

        event.prevent_default();
        event.stop_propagation();
        let edited =
            Edited::apply_command(&command, query, DomSelection::caret_in(input_id), ghost)?;
        DomCaret::in_field(input_id).place(edited.caret);
        Some(edited.value)
    }

    fn select_all(event: &KeyboardEvent, input_id: &'static str) -> bool {
        let modifiers = event.modifiers();
        let plain_meta = modifiers.contains(Modifiers::META)
            && !modifiers.contains(Modifiers::CONTROL)
            && !modifiers.contains(Modifiers::ALT)
            && !modifiers.contains(Modifiers::SHIFT);
        if !plain_meta || event.code() != Code::KeyA {
            return false;
        }

        event.prevent_default();
        event.stop_propagation();
        DomCaret::in_field(input_id).select_all();
        true
    }
}
