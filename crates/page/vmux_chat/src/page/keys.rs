//! The chat page's keyboard, on the far side of the keymap.
//!
//! Nothing here names a key. The page says what is true of it — `chat`, plus `chat.list` and
//! `chat.selector` when one is showing — hands over whatever the core claimed, and performs the
//! verb that comes back. That is the only reason `Enter` can mean four things: the four bindings
//! carry mutually exclusive `when` clauses, and `settings.json` can move any of them without this
//! file agreeing.
//!
//! Three things stay on this side, because the keymap cannot answer them in time or at all.
//!
//! The highlighted row stays a page signal. Which list `Enter` lands in is derived from the draft
//! text, and the draft lives in the browser's own `<textarea>` so that typing, selection, undo and
//! IME keep working — so the list kind and its length change on a keystroke the host has not seen
//! yet, and an index resolved there would select the wrong row.
//!
//! The caret answers for itself, through `wanted_locally`. `ArrowUp` recalls an earlier prompt
//! only from the first line of the draft, and `Ctrl+C` interrupts only when nothing is selected.
//! Both are continuous DOM state with no context transition to publish on, so neither could ever
//! be a pushed claim.
//!
//! A bare digit picks an option locally, and cannot do otherwise: the core deliberately never
//! claims a printable key pressed alone, because a claim set that is one context behind would
//! swallow a character. So `1`-`3` on an approval is the one chat shortcut that is not rebindable.
//!
//! [`ChatKeys::hand_over`] is where a host with no keymap parts company, and it is the only place:
//! the keymap is `settings.json`, so a host that does not hold it cannot say what `Enter` means and
//! the page falls back to the one verb the caret decides by itself. Everything else on this page —
//! the numbers, the lists, the verbs — is reached the same way either way.

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

/// Allow, allow always, deny.
const APPROVAL_OPTION_COUNT: usize = 3;

/// One chat page's keyboard: what the core claimed from it, and what it does with the answer.
///
/// `Copy`, and reached through a context rather than a prop, because the composer is several
/// components below the page root and [`KeyClaim`] is not comparable — a prop would have to be.
#[derive(Clone, Copy)]
pub struct ChatKeys {
    chat: Chat,
    claim: KeyClaim,
}

/// Subscribe the page to the keyboard seam and start listening for what its keys turned out to
/// mean.
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
    /// A key with the prompt focused.
    ///
    /// Stopping here is what tells the page root this keystroke already had a home — and it is why
    /// composition can never reach the root, since an IME only ever composes into a focused field.
    pub fn on_prompt_keydown(&self, event: KeyboardEvent) {
        event.stop_propagation();
        if self.answered_by_number(&event) {
            return;
        }
        self.hand_over(&event);
    }

    /// A key that arrived with the prompt unfocused, so it was aimed at the transcript.
    ///
    /// Anything the keymap did not want and the browser would only have thrown away is typed into
    /// the draft instead, which is what puts a keystroke aimed at the transcript into the prompt.
    pub fn on_root_keydown(&self, event: KeyboardEvent) {
        if self.answered_by_number(&event) {
            return;
        }
        self.hand_over(&event);
        if event.default_action_enabled() {
            self.type_into_draft(&event);
        }
    }

    /// Give the stroke to whoever can say what it meant.
    fn hand_over(&self, event: &KeyboardEvent) {
        if !self.claim.resolves() {
            return self.recall_alone(event);
        }
        self.claim
            .on_keydown(event, |stroke| self.wanted_locally(stroke));
    }

    /// The whole keyboard of a host with no keymap: prompt recall, which the caret decides and no
    /// binding is consulted for.
    ///
    /// Deliberately not a second table of what a key means. A key whose meaning lives in
    /// `settings.json` has to be looked up by whoever holds that file, and here nobody does.
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

    /// The page's own answer about one stroke, asked in the same tick so it never disagrees with
    /// what the user can see.
    ///
    /// Only the strokes the `<textarea>` has a meaning of its own for are ever kept. A key the user
    /// bound deliberately is not shielded, even if it happens to be one of these: the shield exists
    /// because `ArrowUp` doubles as caret movement, not because prompt recall is optional.
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

    /// True when the browser would copy a selection rather than let this mean "interrupt".
    fn copies(stroke: &KeyStroke) -> bool {
        stroke.mods.ctrl && !stroke.mods.alt && !stroke.mods.super_key && stroke.code == "KeyC"
    }

    /// The strokes a multi-line `<textarea>` already moves the caret with, which is what makes
    /// them the page's to answer.
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

    /// Take the host's answers about keys this page handed over.
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

    /// Which way prompt recall would go for this key, or `None` when the caret has somewhere else
    /// to be or there is nothing left to recall.
    ///
    /// A host with nothing to report answers with the start of the draft, which is what makes Up
    /// recall rather than appear to do nothing.
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

    /// Walk the prompt history, setting aside the half-written draft on the way out.
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

    /// A bare number key naming one of an approval's or a question's options.
    ///
    /// Handled here rather than through the keymap because the core never claims a printable key
    /// pressed alone — see the module docs.
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

    /// Type a key nobody wanted into the draft, and put the caret after it.
    ///
    /// The draft signal is the source of truth, so editing it is enough — the textarea is rendered
    /// from it. Appending at the end matches where `focus_prompt_end` puts the caret.
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

/// Which of the page's lists a list verb lands in.
///
/// One type rather than a branch per key, because the four pickers differ only in where their rows
/// come from — and because "which list is showing" is the one thing about the chat keyboard that
/// genuinely belongs to the page. The order [`ChatList::of`] tries them in is the page's own
/// precedence: what the agent is blocked on outranks what the draft has opened.
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

    /// Whether this list is one the draft opened, which is what makes `Escape` close it rather
    /// than interrupt the turn.
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

    /// The signal holding this list's highlighted row. An approval keeps its own, because it can
    /// be showing over a draft that has already opened a picker.
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

    /// Commit a row. A row that is no longer there does nothing: the lists are filtered by the
    /// draft, so a stored index can outlive what it pointed at.
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
    /// What is true of this page now, as the context keys a binding's `when` is resolved against.
    ///
    /// Read reactively — every signal touched here becomes a trigger to republish — so the page
    /// says what it is rather than remembering to announce a change.
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

    /// Everything already asked in this conversation, oldest first, with the queue on the end.
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
