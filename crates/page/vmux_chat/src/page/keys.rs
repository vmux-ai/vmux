use super::state::Chat;
use crate::event::{ApprovalDecision, CHAT_KEY_EVENT, ChatItem, ChatKey};
use crate::format::composer::{
    PromptEdit, PromptHistoryDirection, edit_prompt, move_prompt_history, prompt_history_direction,
};
use dioxus::prelude::*;
use vmux_core::input::{KeyStroke, PageKeyContext, Unclaimed};
use vmux_ui::caret::{EventSelection, byte_offset_to_utf16};
use vmux_ui::components::composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::hooks::{
    KeyClaim, MenuDirection, choice_number_index, move_selection, send, use_key_claim, use_listener,
};

const APPROVAL_OPTION_COUNT: usize = 3;

#[derive(Clone, Copy)]
pub struct ChatKeys {
    chat: Chat,
    claim: KeyClaim,
}

pub fn use_chat_keys(chat: Chat) -> ChatKeys {
    let keys = ChatKeys {
        chat,
        claim: use_key_claim(Unclaimed::Types, move || chat.key_context()),
    };
    keys.listen();
    use_drop(move || {
        let _ = send(&PageKeyContext { keys: Vec::new() });
    });
    keys
}

impl ChatKeys {
    pub fn on_prompt_keydown(&self, event: KeyboardEvent) {
        event.stop_propagation();
        if self.answered_by_number(&event) {
            return;
        }
        self.hand_over(&event);
    }

    pub fn on_root_keydown(&self, event: KeyboardEvent) {
        if self.answered_by_number(&event) {
            return;
        }
        self.hand_over(&event);
        if event.default_action_enabled() {
            self.type_into_draft(&event);
        }
    }

    fn hand_over(&self, event: &KeyboardEvent) {
        if !self.claim.resolves() {
            return self.recall_alone(event);
        }
        self.claim
            .on_keydown(event, |stroke| self.wanted_locally(stroke));
    }

    fn recall_alone(&self, event: &KeyboardEvent) {
        let modifiers = event.modifiers();
        if modifiers.meta() || modifiers.alt() {
            return;
        }
        let key = event.key().to_string();
        let Some(direction) = self.recall_direction(&key, modifiers.ctrl()) else {
            return;
        };
        event.prevent_default();
        self.recall(direction);
    }

    fn wanted_locally(&self, stroke: &KeyStroke) -> bool {
        if Self::copies(stroke) {
            return EventSelection::in_document();
        }
        if !Self::moves_the_caret(stroke) {
            return false;
        }
        if ChatList::of(self.chat).is_some() {
            return false;
        }
        self.recall_direction(&stroke.key, stroke.mods.ctrl)
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

    fn listen(&self) {
        let keys = *self;
        let _resolved = use_listener::<ChatKey, _>(CHAT_KEY_EVENT, move |key| keys.apply(key));
    }

    fn apply(&self, key: ChatKey) {
        match key {
            ChatKey::ListNext => self.move_list(MenuDirection::Next),
            ChatKey::ListPrevious => self.move_list(MenuDirection::Previous),
            ChatKey::ListChoose => self.choose(),
            ChatKey::HistoryOlder => self.recall(PromptHistoryDirection::Older),
            ChatKey::HistoryNewer => self.recall(PromptHistoryDirection::Newer),
            ChatKey::Submit => self.chat.submit(),
            ChatKey::DismissSelector => self.chat.dismiss_selector(),
            ChatKey::Interrupt => self.chat.interrupt(),
            ChatKey::Cancel => self.chat.cancel(),
        }
    }

    fn move_list(&self, direction: MenuDirection) {
        let Some(list) = ChatList::of(self.chat) else {
            return;
        };
        list.move_by(self.chat, direction);
    }

    fn choose(&self) {
        let Some(list) = ChatList::of(self.chat) else {
            return;
        };
        let index = *list.selection(self.chat).peek();
        list.choose(self.chat, index);
    }

    fn recall_direction(&self, key: &str, ctrl: bool) -> Option<PromptHistoryDirection> {
        let draft = self.chat.draft();
        let (start, end) = EventSelection::in_field(PROMPT_INPUT_ID);
        let direction = prompt_history_direction(
            key,
            ctrl,
            &draft,
            byte_offset_to_utf16(&draft, start),
            byte_offset_to_utf16(&draft, end),
        )?;
        let usable = match direction {
            PromptHistoryDirection::Older => !self.chat.prompt_history().is_empty(),
            PromptHistoryDirection::Newer => self.chat.composer.history_cursor.peek().is_some(),
        };
        usable.then_some(direction)
    }

    fn recall(&self, direction: PromptHistoryDirection) {
        let mut draft = self.chat.composer.draft;
        let mut history_cursor = self.chat.composer.history_cursor;
        let mut history_scratch = self.chat.composer.history_scratch;
        let scratch = history_scratch.peek().clone();
        let (value, next_cursor, scratch) = move_prompt_history(
            &self.chat.prompt_history(),
            *history_cursor.peek(),
            &scratch,
            &self.chat.draft(),
            direction,
        );
        draft.set(value);
        history_cursor.set(next_cursor);
        history_scratch.set(scratch);
        focus_prompt_end(PROMPT_INPUT_ID);
    }

    fn answered_by_number(&self, event: &KeyboardEvent) -> bool {
        let modifiers = event.modifiers();
        if modifiers.meta() || modifiers.ctrl() || modifiers.alt() {
            return false;
        }
        let list = match ChatList::of(self.chat) {
            Some(list @ (ChatList::Approval | ChatList::Choice)) => list,
            _ => return false,
        };
        let key = event.key().to_string();
        let Some(index) = choice_number_index(&key, list.len(self.chat)) else {
            return false;
        };
        event.prevent_default();
        list.choose(self.chat, index);
        true
    }

    fn type_into_draft(&self, event: &KeyboardEvent) {
        let modifiers = event.modifiers();
        if modifiers.meta() || modifiers.ctrl() || modifiers.alt() {
            return;
        }
        let key = event.key().to_string();
        let edit = match key.as_str() {
            "Backspace" => PromptEdit::Backspace,
            "Delete" => PromptEdit::Delete,
            _ if key.chars().count() == 1 => PromptEdit::Insert(&key),
            _ => return,
        };
        event.prevent_default();
        let mut draft = self.chat.composer.draft;
        let current = draft.peek().clone();
        let end = current.encode_utf16().count() as u32;
        let (value, _caret) = edit_prompt(&current, end, end, edit);
        draft.set(value);
        focus_prompt_end(PROMPT_INPUT_ID);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChatList {
    Approval,
    Choice,
    Media,
    Session,
    Model,
    Command,
}

impl ChatList {
    fn of(chat: Chat) -> Option<Self> {
        if chat.run.approval.read().is_some() {
            return Some(Self::Approval);
        }
        if !chat.run.choice_options.read().is_empty() {
            return Some(Self::Choice);
        }
        if chat.media_menu_open() {
            return Some(Self::Media);
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
            Self::Media => chat.media.entries.read().len(),
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
        let mut selection = self.selection(chat);
        let landed = move_selection(*selection.peek(), self.len(chat), direction);
        selection.set(landed);
    }

    fn choose(self, chat: Chat, index: usize) {
        match self {
            Self::Approval => {
                let Some((call_id, _, _)) = chat.run.approval.peek().clone() else {
                    return;
                };
                let Some(decision) = ApprovalDecision::for_index(index) else {
                    return;
                };
                chat.answer_approval(call_id, decision);
            }
            Self::Choice => {
                if index < chat.run.choice_options.peek().len() {
                    chat.answer_choice(index);
                }
            }
            Self::Media => {
                let entry = chat.media.entries.peek().get(index).cloned();
                if let Some(entry) = entry {
                    chat.select_media_entry(&entry);
                }
            }
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
                    chat.run_slash_command(&command.name);
                }
            }
        }
    }
}

impl Chat {
    fn key_context(&self) -> Vec<String> {
        let mut keys = vec!["chat".to_string()];
        let Some(list) = ChatList::of(*self) else {
            return keys;
        };
        keys.push("chat.list".to_string());
        if list.is_selector() {
            keys.push("chat.selector".to_string());
        }
        keys
    }

    fn prompt_history(&self) -> Vec<String> {
        let mut history = Vec::new();
        for item in self.transcript.items.peek().iter() {
            let ChatItem::User { text, .. } = item else {
                continue;
            };
            if !text.trim().is_empty() {
                history.push(text.clone());
            }
        }
        for prompt in self.queue.queued.peek().iter() {
            if !prompt.text.trim().is_empty() {
                history.push(prompt.text.clone());
            }
        }
        history
    }
}
