use super::composer::options::ChatMenuSet;
use super::state::Chat;
use crate::event::{ApprovalDecision, ChatItem, ChatKey};
use crate::format::{
    PromptEdit, PromptHistoryDirection, edit_prompt, move_prompt_history, prompt_history_direction,
};
use dioxus::prelude::*;
use vmux_core::input::{KeyStroke, PageKeyContext, Unclaimed};
use vmux_ui::caret::{EventSelection, byte_offset_to_utf16};
use vmux_ui::components::composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::components::composer_bar::ComposerMenuKind;
use vmux_ui::hooks::{
    KeyClaim, MenuDirection, choice_number_index, move_selection, send, use_key_claim,
    use_ui_state_patch,
};

const APPROVAL_OPTION_COUNT: usize = 3;

#[derive(Clone, Copy)]
pub struct ChatKeys {
    handler: ChatKeyHandler,
    claim: KeyClaim,
}

pub fn use_chat_keys(chat: Chat) -> ChatKeys {
    let handler = ChatKeyHandler(chat);
    let events = use_ui_state_patch::<crate::state::ChatUiState, ChatKey>();
    use_effect(move || events.for_each(|key| handler.apply(key)));
    let keys = ChatKeys {
        handler,
        claim: use_key_claim(Unclaimed::Types, move || chat.key_context()),
    };
    use_drop(move || {
        let _ = send(&PageKeyContext { keys: Vec::new() });
    });
    keys
}

impl ChatKeys {
    pub fn on_prompt_keydown(&self, event: KeyboardEvent) {
        event.stop_propagation();
        if self.handler.answered_by_number(&event) {
            return;
        }
        if self.handler.moves_list_locally(&event) {
            return;
        }
        if self.handler.submits_prompt(&event) {
            return;
        }
        self.hand_over(&event);
    }

    pub fn on_root_keydown(&self, event: KeyboardEvent) {
        if self.handler.answered_by_number(&event) {
            return;
        }
        if self.handler.moves_list_locally(&event) {
            return;
        }
        self.hand_over(&event);
        if event.default_action_enabled() {
            self.handler.type_into_draft(&event);
        }
    }

    fn hand_over(&self, event: &KeyboardEvent) {
        if !self.claim.resolves() {
            return self.handler.recall_alone(event);
        }
        self.claim
            .on_keydown(event, |stroke| self.handler.wanted_locally(stroke));
    }
}

#[derive(Clone, Copy)]
struct ChatKeyHandler(Chat);

impl ChatKeyHandler {
    fn moves_list_locally(&self, event: &KeyboardEvent) -> bool {
        if ChatList::current(self.0).is_none() {
            return false;
        }
        let Some(direction) = MenuDirection::from_key(&event.data()) else {
            return false;
        };
        event.prevent_default();
        self.move_list(direction);
        true
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
        if ChatList::current(self.0).is_some() {
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

    fn apply(&self, key: ChatKey) {
        match key {
            ChatKey::ListNext => self.move_list(MenuDirection::Next),
            ChatKey::ListPrevious => self.move_list(MenuDirection::Previous),
            ChatKey::ListChoose => self.choose(),
            ChatKey::HistoryOlder => self.recall(PromptHistoryDirection::Older),
            ChatKey::HistoryNewer => self.recall(PromptHistoryDirection::Newer),
            ChatKey::Submit => self.0.submit(),
            ChatKey::DismissSelector => self.0.dismiss_selector(),
            ChatKey::Interrupt => self.0.interrupt(),
            ChatKey::Cancel => self.0.cancel(),
        }
    }

    fn move_list(&self, direction: MenuDirection) {
        let Some(list) = ChatList::current(self.0) else {
            return;
        };
        list.move_by(self.0, direction);
    }

    fn choose(&self) {
        let Some(list) = ChatList::current(self.0) else {
            return;
        };
        let index = *list.selection(self.0).peek();
        list.choose(self.0, index);
    }

    fn recall_direction(&self, key: &str, ctrl: bool) -> Option<PromptHistoryDirection> {
        let draft = self.0.draft();
        let (start, end) = EventSelection::in_field(PROMPT_INPUT_ID);
        let direction = prompt_history_direction(
            key,
            ctrl,
            &draft,
            byte_offset_to_utf16(&draft, start),
            byte_offset_to_utf16(&draft, end),
        )?;
        let usable = match direction {
            PromptHistoryDirection::Older => !self.0.prompt_history().is_empty(),
            PromptHistoryDirection::Newer => self.0.composer.history_cursor.peek().is_some(),
        };
        usable.then_some(direction)
    }

    fn recall(&self, direction: PromptHistoryDirection) {
        let mut draft = self.0.composer.draft;
        let mut history_cursor = self.0.composer.history_cursor;
        let mut history_scratch = self.0.composer.history_scratch;
        let scratch = history_scratch.peek().clone();
        let (value, next_cursor, scratch) = move_prompt_history(
            &self.0.prompt_history(),
            *history_cursor.peek(),
            &scratch,
            &self.0.draft(),
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
        let list = match ChatList::current(self.0) {
            Some(list @ (ChatList::Approval | ChatList::Choice)) => list,
            _ => return false,
        };
        let key = event.key().to_string();
        let Some(index) = choice_number_index(&key, list.len(self.0)) else {
            return false;
        };
        event.prevent_default();
        list.choose(self.0, index);
        true
    }

    fn submits_prompt(&self, event: &KeyboardEvent) -> bool {
        let modifiers = event.modifiers();
        if event.key() != Key::Enter
            || modifiers.shift()
            || modifiers.meta()
            || modifiers.ctrl()
            || modifiers.alt()
            || ChatList::current(self.0).is_some()
        {
            return false;
        }
        event.prevent_default();
        self.0.submit();
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
        let mut draft = self.0.composer.draft;
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
                let Some(decision) = ApprovalDecision::for_index(index) else {
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
                    chat.run_slash_command(&command.name);
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
