use super::readline_driver::TextEditCommand;
use dioxus::prelude::*;
use vmux_api::command_bar::{CommandBarOpenEvent, OpenId};
use vmux_ui::dom::{DomCaret, DomSelection};
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::KeyClaim;

pub const COMMAND_BAR_INPUT_ID: &str = "command-bar-input";

#[derive(Clone, Copy, PartialEq)]
pub struct PaletteInput {
    pub last_focus_open_id: Signal<OpenId>,
    pub last_input_revision: Signal<u64>,
}

pub fn use_palette_input() -> PaletteInput {
    PaletteInput {
        last_focus_open_id: use_signal(|| OpenId(u64::MAX)),
        last_input_revision: use_signal(|| 0),
    }
}

impl PaletteInput {
    pub fn refocus(&mut self, open_id: OpenId) -> bool {
        if !open_id.should_refocus((self.last_focus_open_id)()) {
            return false;
        }
        self.last_focus_open_id.set(open_id);
        true
    }

    pub fn apply_host_input(&mut self, revision: u64, input_id: &'static str) {
        if revision == 0 || revision <= (self.last_input_revision)() {
            return;
        }
        self.last_input_revision.set(revision);
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
    pub fn focus(opened: &CommandBarOpenEvent) {
        if opened.caret_at_end {
            FocusClaim::new(COMMAND_BAR_INPUT_ID)
                .caret_at_end()
                .request();
            return;
        }
        FocusClaim::new(COMMAND_BAR_INPUT_ID).request();
        DomCaret::in_field(COMMAND_BAR_INPUT_ID).select_all_from_start_next_frame();
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
        let edit = TextEditCommand::from_command(&command)?;

        event.prevent_default();
        event.stop_propagation();
        Some(Self::edit(
            query,
            edit,
            ghost,
            DomSelection::caret_in(input_id),
            input_id,
        ))
    }

    fn edit(
        query: &str,
        edit: TextEditCommand,
        ghost: &str,
        caret: usize,
        input_id: &'static str,
    ) -> String {
        let ghost = match edit {
            TextEditCommand::End => ghost,
            _ => "",
        };

        let edited = edit.apply(query, caret, ghost);
        DomCaret::in_field(input_id).place(edited.caret);
        edited.value
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
