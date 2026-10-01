use dioxus::prelude::*;
use vmux_ecs::event::FileTextInput;
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::{PressedKey, send};
use vmux_ui::ime::ImeGuard;

pub(crate) struct EditorFocus;

impl EditorFocus {
    pub(crate) const CONTAINER_ID: &'static str = "file-container";
    pub(crate) const FILE_INPUT_ID: &'static str = "file-input";
    pub(crate) const FIND_INPUT_ID: &'static str = "file-find-input";

    pub(crate) fn container() {
        FocusClaim::new(Self::CONTAINER_ID).request();
    }

    pub(crate) fn file() {
        FocusClaim::new(Self::FILE_INPUT_ID).request();
    }

    pub(crate) fn find() {
        FocusClaim::new(Self::FIND_INPUT_ID).request();
    }
}

pub(super) struct EditorInput;

impl EditorInput {
    pub(super) fn commit(mut field: Signal<String>, text: String) {
        if text.is_empty() {
            return;
        }
        let _ = send(&FileTextInput { text });
        field.set(String::new());
    }

    pub(super) fn forward_key(
        event: &Event<KeyboardData>,
        mode: vmux_api::editor::EditMode,
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
