use dioxus::prelude::*;
use vmux_core::event::FileTextInput;
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::{PressedKey, send};
use vmux_ui::ime::ImeGuard;

pub(super) const CONTAINER_ID: &str = "file-container";
pub(super) const INPUT_ID: &str = "file-input";
pub(crate) const FIND_INPUT_ID: &str = "file-find-input";

pub(super) fn focus_container() {
    FocusClaim::new(CONTAINER_ID).request();
}

pub(crate) fn focus_file_input() {
    FocusClaim::new(INPUT_ID).request();
}

pub(crate) fn focus_find_input() {
    FocusClaim::new(FIND_INPUT_ID).request();
}

pub(super) fn send_committed_text(mut field: Signal<String>, text: String) {
    if text.is_empty() {
        return;
    }
    let _ = send(&FileTextInput { text });
    field.set(String::new());
}

pub(super) fn forward_file_key(
    event: &Event<KeyboardData>,
    mode: vmux_core::editor::EditMode,
) -> bool {
    let Some(stroke) = PressedKey::new(&event.data()).stroke() else {
        return false;
    };
    if mode.accepts_text() && stroke.is_text_input() {
        return false;
    }
    event.prevent_default();
    let _ = send(&stroke);
    true
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct PreeditField(bool);

impl From<ImeGuard> for PreeditField {
    fn from(ime: ImeGuard) -> Self {
        Self(ime.active())
    }
}

impl PreeditField {
    pub(super) fn text_color(self) -> &'static str {
        match self.0 {
            true => "inherit",
            false => "transparent",
        }
    }

    pub(super) fn caret_class(self) -> &'static str {
        match self.0 {
            true => "",
            false => "caret-transparent",
        }
    }

    pub(super) fn owns_caret(self) -> bool {
        self.0
    }
}
