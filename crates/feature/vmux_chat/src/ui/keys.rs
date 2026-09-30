use super::composer::options::ChatMenuSet;
use super::format::{PromptEdit, PromptHistoryDirection, prompt_history_direction};
use super::state::Chat;
use dioxus::prelude::*;
use vmux_core::input::{KeyStroke, UiKeyContext, Unclaimed};
use vmux_ui::caret::{EventSelection, byte_offset_to_utf16};
use vmux_ui::components::composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::components::composer_bar::ComposerMenuKind;
use vmux_ui::hooks::{KeyClaim, MenuDirection, move_selection, send, use_key_claim};

const APPROVAL_OPTION_COUNT: usize = 3;

#[derive(Clone, Copy)]
pub struct ChatKeys {
    handler: ChatKeyHandler,
    claim: KeyClaim,
}

pub fn use_chat_keys(chat: Chat) -> ChatKeys {
    let handler = ChatKeyHandler(chat);
    let keys = ChatKeys {
        handler,
        claim: use_key_claim(Unclaimed::Types, move || chat.key_context()),
    };
    use_drop(move || {
        let _ = send(&UiKeyContext { keys: Vec::new() });
    });
    keys
}

impl ChatKeys {
    pub fn on_prompt_keydown(&self, event: KeyboardEvent) {
        event.stop_propagation();
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
    fn wanted_locally(&self, stroke: &KeyStroke) -> bool {
        if Self::copies(stroke) {
            return EventSelection::in_document();
        }
        if !Self::moves_the_caret(stroke) {
            return false;
        }
        if ChatList::current(self.0).is_some() {
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
        let (start, end) = EventSelection::in_field(PROMPT_INPUT_ID);
        prompt_history_direction(
            key,
            ctrl,
            &draft,
            byte_offset_to_utf16(&draft, start),
            byte_offset_to_utf16(&draft, end),
        )
    }
}

impl Chat {
    pub(super) fn move_active_list(self, direction: MenuDirection) {
        let Some(list) = ChatList::current(self) else {
            return;
        };
        list.move_by(self, direction);
    }

    pub(super) fn choose_active_list(self) {
        let Some(list) = ChatList::current(self) else {
            return;
        };
        let index = *list.selection(self).peek();
        list.choose(self, index);
    }

    pub(super) fn accepts_input_effect(mut self, revision: u64) -> bool {
        if revision <= *self.input_effect_revision.peek() {
            return false;
        }
        self.input_effect_revision.set(revision);
        true
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
        let current = self.0.composer.draft.peek().clone();
        let end = current.encode_utf16().count() as u32;
        let (value, _caret) = edit.apply(&current, end, end);
        self.0.edit_draft(value);
        focus_prompt_end(PROMPT_INPUT_ID);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChatList {
    Approval,
    Choice,
    ComposerMenu(ComposerMenuKind),
    Media,
    Mcp,
    Session,
    Model,
    Command,
}

impl ChatList {
    fn current(chat: Chat) -> Option<Self> {
        if chat.run.approval.read().is_some() {
            return Some(Self::Approval);
        }
        if !chat.run.choice_options.read().is_empty() {
            return Some(Self::Choice);
        }
        if let Some(kind) = chat.menu.opened() {
            return Some(Self::ComposerMenu(kind));
        }
        if chat.media_menu_open() {
            return Some(Self::Media);
        }
        if chat.mcp_menu_open() {
            return Some(Self::Mcp);
        }
        if chat.resume_menu_open() {
            return Some(Self::Session);
        }
        if chat.model_menu_open() {
            return Some(Self::Model);
        }
        if chat.command_menu_open() {
            return Some(Self::Command);
        }
        None
    }

    fn is_selector(self) -> bool {
        !matches!(self, Self::Approval | Self::Choice)
    }

    fn len(self, chat: Chat) -> usize {
        match self {
            Self::Approval => APPROVAL_OPTION_COUNT,
            Self::Choice => chat.run.choice_options.read().len(),
            Self::ComposerMenu(kind) => ChatMenuSet::from(chat).rows(kind),
            Self::Media => chat.media.entries.read().len(),
            Self::Mcp => chat.filtered_mcp_servers().len(),
            Self::Session => chat.filtered_sessions().len(),
            Self::Model => chat.filtered_models().len(),
            Self::Command => chat.filtered_commands().len(),
        }
    }

    fn selection(self, chat: Chat) -> Signal<usize> {
        match self {
            Self::Approval => chat.run.approval_sel,
            _ => chat.slash.menu_sel,
        }
    }

    fn move_by(self, chat: Chat, direction: MenuDirection) {
        if let Self::ComposerMenu(kind) = self {
            chat.menu
                .step(direction, ChatMenuSet::from(chat).rows(kind));
            return;
        }
        let mut selection = self.selection(chat);
        let landed = move_selection(*selection.peek(), self.len(chat), direction);
        selection.set(landed);
    }

    fn choose(self, chat: Chat, index: usize) {
        match self {
            Self::Approval => {
                let Some(approval) = chat.run.approval.peek().clone() else {
                    return;
                };
                let Some(decision) = super::approval::APPROVAL_DECISIONS.get(index).copied() else {
                    return;
                };
                chat.answer_approval(approval.call_id, decision);
            }
            Self::Choice => {
                if index < chat.run.choice_options.peek().len() {
                    chat.answer_choice(index);
                }
            }
            Self::ComposerMenu(kind) => {
                if ChatMenuSet::from(chat).choose(kind, index) {
                    chat.menu.close();
                }
            }
            Self::Media => {
                let entry = chat.media.entries.peek().get(index).cloned();
                if let Some(entry) = entry {
                    chat.select_media_entry(&entry);
                }
            }
            Self::Mcp => chat.activate_mcp_server(index),
            Self::Session => {
                if let Some(session) = chat.filtered_sessions().get(index) {
                    chat.select_resume_session(session);
                }
            }
            Self::Model => {
                if let Some(model) = chat.filtered_models().get(index) {
                    chat.select_model(model);
                }
            }
            Self::Command => {
                if let Some(command) = chat.filtered_commands().get(index) {
                    chat.select_slash_command(command.command);
                }
            }
        }
    }
}

impl Chat {
    fn key_context(&self) -> Vec<String> {
        let mut keys = vec!["chat".to_string()];
        let Some(list) = ChatList::current(*self) else {
            return keys;
        };
        keys.push("chat.list".to_string());
        if matches!(list, ChatList::Approval | ChatList::Choice) {
            keys.push("chat.choice".to_string());
        }
        if list.is_selector() {
            keys.push("chat.selector".to_string());
        }
        keys
    }
}
