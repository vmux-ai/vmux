//! What a keystroke means to the chat page.
//!
//! Two entry points, because a key can arrive with the prompt focused or with it not: the composer
//! stops propagation while it has focus, so anything reaching the page root was aimed somewhere
//! else. The root handler decides whether the key still belongs to the prompt and forwards it, so
//! the rules live in one place rather than being restated for the unfocused case.

use super::state::Chat;
use crate::event::{ChatCancel, ChatEscape, ChatItem};
use crate::format::composer::{
    PromptEdit, PromptHistoryDirection, SelectorMode, approval_decision_for_index, edit_prompt,
    move_prompt_history, prompt_history_direction, selector_mode, should_clear_draft_on_escape,
};
use dioxus::prelude::*;
#[cfg(web)]
use vmux_ui::components::prompt_composer::prompt_textarea;
use vmux_ui::components::prompt_composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::hooks::{choice_number_index, menu_direction, move_selection, send};
use vmux_wire::prompt_media::inline_media_query;

/// Allow, allow always, deny.
const APPROVAL_OPTION_COUNT: usize = 3;

impl Chat {
    /// A key with the prompt focused. Pending approvals and choices claim it first, then whichever
    /// selector the draft has opened, then prompt-history recall, then the composer itself.
    pub fn prompt_keydown(&self, event: KeyboardEvent) {
        // The page root also listens, to catch typing aimed elsewhere. Stopping here is what
        // tells it this keystroke already had a home — and it is why composition can never
        // reach the root, since an IME only ever composes into a focused field.
        event.stop_propagation();
        if self.approval_keydown(&event) || self.choice_keydown(&event) {
            return;
        }
        if self.selector_keydown(&event) {
            return;
        }
        if self.history_keydown(&event) {
            return;
        }
        self.composer_keydown(&event);
    }

    /// Keys that arrive with the prompt unfocused. Navigation, approvals and choices mean exactly
    /// what they mean with it focused, so they are handed to that handler rather than restated;
    /// anything else that is a plain edit is typed into the draft, which is what puts a keystroke
    /// aimed at the transcript into the prompt.
    pub fn root_keydown(&self, event: KeyboardEvent) {
        let key = event.key().to_string();
        let modifiers = event.modifiers();
        let selector_open = self.selector_open();
        let approval_open = self.run.approval.peek().is_some();
        let choice_len = self.run.choice_options.peek().len();
        let unmodified = !modifiers.meta() && !modifiers.ctrl() && !modifiers.alt();
        let direction = if modifiers.meta() || modifiers.alt() {
            None
        } else {
            menu_direction(&key, modifiers.ctrl())
        };
        let choice_key = direction.is_some()
            || (unmodified && (key == "Enter" || choice_number_index(&key, choice_len).is_some()));
        let approval_key = direction.is_some()
            || (unmodified
                && (key == "Enter" || choice_number_index(&key, APPROVAL_OPTION_COUNT).is_some()));
        let selector_key =
            direction.is_some() || (unmodified && matches!(key.as_str(), "Enter" | "Escape"));

        if (approval_open && approval_key)
            || (choice_len > 0 && choice_key)
            || direction.is_some()
            || (selector_open && selector_key)
        {
            self.prompt_keydown(event);
            return;
        }
        if !unmodified {
            return;
        }
        let edit = match key.as_str() {
            "Backspace" => PromptEdit::Backspace,
            "Delete" => PromptEdit::Delete,
            _ if key.chars().count() == 1 => PromptEdit::Insert(&key),
            _ => return,
        };
        event.prevent_default();
        // The draft signal is the source of truth, so editing it is enough — the textarea is
        // rendered from it. Appending at the end matches where focus_prompt_end puts the caret.
        let mut draft = self.composer.draft;
        let current = draft.peek().clone();
        let end = current.encode_utf16().count() as u32;
        let (value, _caret) = edit_prompt(&current, end, end, edit);
        draft.set(value);
        focus_prompt_end(PROMPT_INPUT_ID);
    }

    /// A pending tool approval takes arrows, `1`-`3` and Enter until it is answered.
    fn approval_keydown(&self, event: &KeyboardEvent) -> bool {
        let Some((call_id, _, _)) = self.run.approval.peek().clone() else {
            return false;
        };
        let mut approval_sel = self.run.approval_sel;
        let key = event.key().to_string();
        if !event.modifiers().meta()
            && !event.modifiers().alt()
            && let Some(direction) = menu_direction(&key, event.modifiers().ctrl())
        {
            event.prevent_default();
            approval_sel.set(move_selection(
                approval_sel(),
                APPROVAL_OPTION_COUNT,
                direction,
            ));
            return true;
        }
        if !Self::picks_option(event, APPROVAL_OPTION_COUNT) {
            return false;
        }
        event.prevent_default();
        let index = choice_number_index(&key, APPROVAL_OPTION_COUNT).unwrap_or(approval_sel());
        if let Some(decision) = approval_decision_for_index(index) {
            self.answer_approval(call_id, decision);
        }
        true
    }

    /// A pending multiple-choice question takes the same keys, over its own options.
    fn choice_keydown(&self, event: &KeyboardEvent) -> bool {
        let options = self.run.choice_options.peek().clone();
        if options.is_empty() {
            return false;
        }
        let mut menu_sel = self.slash.menu_sel;
        let key = event.key().to_string();
        if !event.modifiers().meta()
            && !event.modifiers().alt()
            && let Some(direction) = menu_direction(&key, event.modifiers().ctrl())
        {
            event.prevent_default();
            let selected = *menu_sel.peek();
            menu_sel.set(move_selection(selected, options.len(), direction));
            return true;
        }
        if !Self::picks_option(event, options.len()) {
            return false;
        }
        event.prevent_default();
        let selected = *menu_sel.peek();
        self.answer_choice(choice_number_index(&key, options.len()).unwrap_or(selected));
        true
    }

    /// A number key or a bare Enter, either of which commits the highlighted option.
    fn picks_option(event: &KeyboardEvent, count: usize) -> bool {
        let key = event.key().to_string();
        let numbered = !event.modifiers().meta()
            && !event.modifiers().ctrl()
            && !event.modifiers().alt()
            && choice_number_index(&key, count).is_some();
        let entered = event.key() == Key::Enter
            && !event.modifiers().shift()
            && !event.modifiers().meta()
            && !event.modifiers().ctrl()
            && !event.modifiers().alt();
        numbered || entered
    }

    /// Arrows, Enter and Escape belong to whichever picker the draft has opened.
    fn selector_keydown(&self, event: &KeyboardEvent) -> bool {
        let draft = self.draft();
        let media_open = inline_media_query(&draft).is_some();
        let (session_open, model_open) = match selector_mode(&draft) {
            SelectorMode::Resume(_) => (true, false),
            SelectorMode::Models(_) => (false, true),
            _ => (false, false),
        };
        let commands = self.filtered_commands();
        if !(media_open || session_open || model_open || !commands.is_empty()) {
            return false;
        }
        let mut menu_sel = self.slash.menu_sel;
        let key = event.key().to_string();
        let command_modifier =
            event.modifiers().meta() || event.modifiers().ctrl() || event.modifiers().alt();
        let direction = if event.modifiers().meta() || event.modifiers().alt() {
            None
        } else {
            menu_direction(&key, event.modifiers().ctrl())
        };
        let media_items = if media_open {
            self.media.entries.peek().clone()
        } else {
            Vec::new()
        };
        let sessions = self.filtered_sessions();
        let models = self.filtered_models();
        if let Some(direction) = direction {
            event.prevent_default();
            let len = if media_open {
                media_items.len()
            } else if session_open {
                sessions.len()
            } else if model_open {
                models.len()
            } else {
                commands.len()
            };
            let selected = *menu_sel.peek();
            menu_sel.set(move_selection(selected, len, direction));
            return true;
        }
        if event.key() == Key::Enter && !event.modifiers().shift() && !command_modifier {
            event.prevent_default();
            let selected = *menu_sel.peek();
            if media_open {
                if let Some(entry) = media_items.get(selected) {
                    self.select_media_entry(entry);
                }
            } else if session_open {
                if let Some(session) = sessions.get(selected) {
                    self.select_resume_session(session);
                }
            } else if model_open {
                if let Some(model) = models.get(selected) {
                    self.select_model(model);
                }
            } else if let Some(command) = commands.get(selected) {
                self.run_slash_command(&command.name);
            }
            return true;
        }
        if event.key() == Key::Escape && !command_modifier {
            event.prevent_default();
            self.dismiss_selector();
            return true;
        }
        // Enter and Escape belong to an open picker even when a modifier stopped the branches
        // above from acting on them, so they must not fall through to submit or interrupt.
        (media_open || session_open || model_open)
            && matches!(event.key(), Key::Enter | Key::Escape)
    }

    /// Up and Down walk earlier prompts, when the caret is somewhere that leaves them free to.
    fn history_keydown(&self, event: &KeyboardEvent) -> bool {
        if self.selector_open() || event.modifiers().meta() || event.modifiers().alt() {
            return false;
        }
        let key = event.key().to_string();
        let draft = self.draft();
        let Some((start, end)) = prompt_caret() else {
            return false;
        };
        let Some(direction) =
            prompt_history_direction(&key, event.modifiers().ctrl(), &draft, start, end)
        else {
            return false;
        };
        let mut history_cursor = self.composer.history_cursor;
        let mut history_scratch = self.composer.history_scratch;
        let mut prompt = self.composer.draft;
        let history = self.prompt_history();
        let cursor = *history_cursor.peek();
        let should_handle = match direction {
            PromptHistoryDirection::Older => !history.is_empty(),
            PromptHistoryDirection::Newer => cursor.is_some(),
        };
        if !should_handle {
            return false;
        }
        event.prevent_default();
        let (value, next_cursor, scratch) =
            move_prompt_history(&history, cursor, &history_scratch.peek(), &draft, direction);
        prompt.set(value);
        history_cursor.set(next_cursor);
        history_scratch.set(scratch);
        focus_prompt_end(PROMPT_INPUT_ID);
        true
    }

    /// Enter submits, Escape interrupts and may clear the draft, Ctrl+C cancels the turn.
    fn composer_keydown(&self, event: &KeyboardEvent) {
        if event.key() == Key::Enter && !event.modifiers().shift() {
            event.prevent_default();
            self.submit();
            return;
        }
        if event.key() == Key::Escape {
            event.prevent_default();
            let _ = send(&ChatEscape);
            let mut draft = self.composer.draft;
            if should_clear_draft_on_escape(
                self.streaming(),
                self.queue.queued.peek().is_empty(),
                draft.peek().is_empty(),
            ) {
                draft.set(String::new());
            }
            return;
        }
        if event.modifiers().ctrl()
            && matches!(event.key(), Key::Character(c) if c == "c")
            && !has_text_selection()
        {
            event.prevent_default();
            let _ = send(&ChatCancel);
        }
    }

    /// Whether any picker is showing, which is what makes arrows and Enter mean navigation.
    fn selector_open(&self) -> bool {
        let draft = self.draft();
        if inline_media_query(&draft).is_some() {
            return true;
        }
        match selector_mode(&draft) {
            SelectorMode::Resume(_) | SelectorMode::Models(_) => true,
            SelectorMode::Commands(query) => {
                let query = query.to_lowercase();
                self.slash
                    .commands
                    .peek()
                    .iter()
                    .any(|command| command.name.starts_with(&query))
            }
            SelectorMode::None => false,
        }
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

/// Where the caret sits in the prompt, which decides whether Up moves within the text or recalls
/// the previous prompt.
#[cfg(web)]
fn prompt_caret() -> Option<(u32, u32)> {
    let textarea = prompt_textarea(PROMPT_INPUT_ID)?;
    let start = textarea
        .selection_start()
        .ok()
        .flatten()
        .unwrap_or_default();
    let end = textarea.selection_end().ok().flatten().unwrap_or(start);
    Some((start, end))
}

/// Nothing to measure without an element handle. Reporting the start is what makes Up recall
/// history rather than appear to do nothing.
#[cfg(not(web))]
fn prompt_caret() -> Option<(u32, u32)> {
    Some((0, 0))
}

/// True when the page has a non-collapsed text selection — so Ctrl+C should copy, not interrupt.
#[cfg(web)]
fn has_text_selection() -> bool {
    web_sys::window()
        .and_then(|w| w.get_selection().ok().flatten())
        .map(|s| !s.is_collapsed())
        .unwrap_or(false)
}

/// A touch host has neither a caret nor a Ctrl+C, so the question never arises and the answer
/// that leaves the shortcut meaning "interrupt" is the right one.
#[cfg(not(web))]
fn has_text_selection() -> bool {
    false
}
