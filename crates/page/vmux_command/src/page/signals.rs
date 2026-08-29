use crate::event::{CommandBarKey, CommandBarOpenEvent, OpenId};
use dioxus::prelude::*;
use vmux_ui::caret::{EventSelection, TextCaret};
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::MenuDirection;
use vmux_ui::launcher::keyboard::{CtrlEditAction, CtrlKeyCapture, ctrl_key_capture_for_code};
use vmux_ui::launcher::palette::{PaletteDraft, PaletteMode, PaletteRows, PaletteState};

pub const COMMAND_BAR_INPUT_ID: &str = "command-bar-input";

#[derive(Clone, Copy, PartialEq)]
pub struct PaletteSignals {
    pub query: Signal<String>,
    pub selected: Signal<usize>,
    pub nav_mode: Signal<bool>,
    pub target_url: Signal<String>,
    pub last_open_id: Signal<OpenId>,
    pub last_focus_open_id: Signal<OpenId>,
}

pub fn use_palette_signals() -> PaletteSignals {
    PaletteSignals {
        query: use_signal(String::new),
        selected: use_signal(|| 0usize),
        nav_mode: use_signal(|| false),
        target_url: use_signal(String::new),
        last_open_id: use_signal(|| OpenId(u64::MAX)),
        last_focus_open_id: use_signal(|| OpenId(u64::MAX)),
    }
}

impl PaletteSignals {
    pub fn draft(&self) -> PaletteDraft {
        PaletteDraft {
            query: (self.query)(),
            selected: (self.selected)(),
            nav_mode: (self.nav_mode)(),
            target_url: (self.target_url)(),
            ..PaletteDraft::default()
        }
    }

    pub fn reopened(&mut self, open_id: OpenId) -> bool {
        if (self.last_open_id)() == open_id {
            return false;
        }
        self.last_open_id.set(open_id);
        true
    }

    pub fn restart(&mut self, opened: &CommandBarOpenEvent) {
        self.query.set(opened.url.clone());
        self.selected.set(PaletteState::opening_selection(opened));
        self.nav_mode.set(false);
    }

    pub fn refocus(&mut self, open_id: OpenId) -> bool {
        if !open_id.should_refocus((self.last_focus_open_id)()) {
            return false;
        }
        self.last_focus_open_id.set(open_id);
        true
    }

    pub fn retype(&mut self, value: String) {
        self.query.set(value);
        self.selected.set(0);
        self.nav_mode.set(false);
    }

    pub fn retarget(&mut self, url: String) {
        self.target_url.set(url);
        self.selected.set(0);
        self.nav_mode.set(false);
    }

    pub fn highlight(&mut self, index: usize) {
        self.selected.set(index);
        self.nav_mode.set(true);
    }

    pub fn watch(&self) {
        let _ = (self.query)();
        let _ = (self.selected)();
        let _ = (self.nav_mode)();
    }
}

#[derive(Clone, Copy)]
pub struct PaletteKeys {
    pub rows: Memo<PaletteRows>,
    pub signals: PaletteSignals,
    pub on_dismiss: EventHandler<()>,
}

impl PaletteKeys {
    pub fn apply(&mut self, key: CommandBarKey) {
        match key {
            CommandBarKey::Next => self.move_selection(MenuDirection::Next),
            CommandBarKey::Previous => self.move_selection(MenuDirection::Previous),
            CommandBarKey::Complete => self.accept_completion(),
            CommandBarKey::Dismiss => self.on_dismiss.call(()),
        }
    }

    fn move_selection(&mut self, direction: MenuDirection) {
        let rows = self.rows.read();
        let landed = rows.step(rows.selected(*self.signals.selected.peek()), direction);
        drop(rows);
        self.signals.highlight(landed);
    }

    fn accept_completion(&mut self) {
        let rows = self.rows.read();
        if rows.ghost.is_empty() {
            return;
        }
        let completed = rows.completed(&self.signals.query.peek());
        drop(rows);
        self.signals.query.set(completed.clone());
        self.signals.selected.set(0);
        TextCaret::in_field(COMMAND_BAR_INPUT_ID).place(completed.len());
    }
}

pub struct TypedDigit;

impl TypedDigit {
    pub fn of(event: &KeyboardEvent) -> Option<usize> {
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
        if PaletteMode::opened(opened).opens_at_end(&opened.url) {
            FocusClaim::new(COMMAND_BAR_INPUT_ID)
                .caret_at_end()
                .request();
            return;
        }
        FocusClaim::new(COMMAND_BAR_INPUT_ID).request();
        TextCaret::in_field(COMMAND_BAR_INPUT_ID).select_all_from_start_next_frame();
    }
}

pub struct Readline;

impl Readline {
    pub fn chord(
        event: &KeyboardEvent,
        mut query: Signal<String>,
        ghost: &str,
        input_id: &'static str,
    ) -> bool {
        if Self::select_all(event, input_id) {
            return true;
        }
        if !event.modifiers().contains(Modifiers::CONTROL) {
            return false;
        }

        let action = match ctrl_key_capture_for_code(&event.code().to_string()) {
            CtrlKeyCapture::Ignore => return false,
            CtrlKeyCapture::PassToDioxus => {
                event.prevent_default();
                return false;
            }
            CtrlKeyCapture::Edit(action) => action,
        };

        event.prevent_default();
        event.stop_propagation();
        Self::edit(
            &mut query,
            action,
            ghost,
            EventSelection::caret_in(input_id),
            input_id,
        );
        true
    }

    fn edit(
        query: &mut Signal<String>,
        action: CtrlEditAction,
        ghost: &str,
        caret: usize,
        input_id: &'static str,
    ) {
        let value = query.peek().clone();
        let ghost = match action {
            CtrlEditAction::End => ghost,
            _ => "",
        };

        let edited = action.apply(&value, caret, ghost);
        if edited.value != value {
            query.set(edited.value);
        }
        TextCaret::in_field(input_id).place(edited.caret);
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
        TextCaret::in_field(input_id).select_all();
        true
    }
}
