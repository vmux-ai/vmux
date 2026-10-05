use super::format::{PromptEdit, PromptHistoryDirection};
use super::state::Chat;
use dioxus::prelude::*;
use vmux_api::input::{KeyContextRequest, KeyStroke};
use vmux_ui::components::composer::{PROMPT_INPUT_ID, PromptFocus};
use vmux_ui::dom::DomSelection;
use vmux_ui::hooks::Unclaimed;
use vmux_ui::hooks::{KeyClaim, send, use_key_claim};

#[derive(Clone, Copy)]
pub struct ChatKeys {
    handler: ChatKeyHandler,
    claim: KeyClaim,
}

pub fn use_chat_keys(chat: Chat) -> ChatKeys {
    let handler = ChatKeyHandler(chat);
    let keys = ChatKeys {
        handler,
        claim: use_key_claim(Unclaimed::Types, move || {
            chat.selector.value.read().key_context.clone()
        }),
    };
    use_drop(move || {
        let _ = send(&KeyContextRequest { keys: Vec::new() });
    });
    keys
}

impl ChatKeys {
    pub fn on_prompt_keydown(&self, event: KeyboardEvent) {
        event.stop_propagation();
        if self.handler.submits(&event) {
            event.prevent_default();
            self.handler.0.submit();
            return;
        }
        if self.claim.resolves() {
            self.claim
                .on_keydown(&event, |stroke| self.handler.wanted_locally(stroke));
        }
    }

    pub fn on_root_keydown(&self, event: KeyboardEvent) {
        if self.claim.resolves() {
            self.claim
                .on_keydown(&event, |stroke| self.handler.wanted_locally(stroke));
            if event.default_action_enabled() {
                self.handler.type_into_draft(&event);
            }
            return;
        }
        if event.default_action_enabled() {
            self.handler.type_into_draft(&event);
        }
    }
}

#[derive(Clone, Copy)]
struct ChatKeyHandler(Chat);

impl ChatKeyHandler {
    fn submits(&self, event: &KeyboardEvent) -> bool {
        let modifiers = event.modifiers();
        event.key() == Key::Enter
            && !modifiers.shift()
            && !modifiers.ctrl()
            && !modifiers.alt()
            && !modifiers.meta()
            && self.0.menu.opened().is_none()
            && self.0.selector.value.read().active.is_none()
    }

    fn wanted_locally(&self, stroke: &KeyStroke) -> bool {
        if Self::copies(stroke) {
            return DomSelection::in_document();
        }
        if !Self::moves_the_caret(stroke) {
            return false;
        }
        if self.0.selector.value.read().active.is_some() {
            return false;
        }
        self.history_direction(&stroke.key, stroke.mods.ctrl)
            .is_none()
    }

    fn copies(stroke: &KeyStroke) -> bool {
        stroke.mods.ctrl && !stroke.mods.alt && !stroke.mods.super_key && stroke.code == "KeyC"
    }

    fn moves_the_caret(stroke: &KeyStroke) -> bool {
        if stroke.mods.super_key || stroke.mods.alt {
            return false;
        }
        match stroke.code.as_str() {
            "ArrowUp" | "ArrowDown" => !stroke.mods.ctrl,
            "KeyN" | "KeyP" => stroke.mods.ctrl,
            _ => false,
        }
    }

    fn history_direction(&self, key: &str, ctrl: bool) -> Option<PromptHistoryDirection> {
        let draft = self.0.draft();
        let (start, end) = DomSelection::in_field(PROMPT_INPUT_ID);
        PromptHistoryDirection::from_key(key, ctrl, &draft, start, end)
    }
}

impl ChatKeyHandler {
    fn type_into_draft(&self, event: &KeyboardEvent) {
        let modifiers = event.modifiers();
        if modifiers.meta() || modifiers.ctrl() || modifiers.alt() {
            return;
        }
        let key = event.key().to_string();
        let edit = match key.as_str() {
            "Backspace" => PromptEdit::Backspace,
            "Delete" => PromptEdit::Delete,
            _ if key.chars().count() == 1 => PromptEdit::Insert(key),
            _ => return,
        };
        event.prevent_default();
        let current = self.0.draft();
        let end = current.encode_utf16().count() as u32;
        let (value, _caret) = edit.apply(&current, end, end);
        self.0.edit_draft(value);
        PromptFocus::end(PROMPT_INPUT_ID);
    }
}
