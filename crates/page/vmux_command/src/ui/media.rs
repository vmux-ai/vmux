use std::collections::HashMap;

use crate::event::{CommandPaletteRemoveAttachmentRequest, CommandPaletteState, OpenId};
use crate::prompt_media::{
    ChatAttachPaths, ChatAttachment, ChatMediaEntry, inline_media_query, replace_inline_media_query,
};
use dioxus::prelude::*;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, PromptComposerAttachment, focus_prompt_end};
use vmux_ui::components::prompt_media_options::PromptMediaOption;
use vmux_ui::file_icon::FilePath;
use vmux_ui::hooks::send;

#[derive(Clone, Copy)]
pub struct PromptMedia {
    state: Signal<CommandPaletteState>,
    open_id: OpenId,
    pub selected: Signal<usize>,
    selected_query: Signal<Option<String>>,
}

pub fn use_prompt_media(state: Signal<CommandPaletteState>, open_id: OpenId) -> PromptMedia {
    PromptMedia {
        state,
        open_id,
        selected: use_signal(|| 0usize),
        selected_query: use_signal(|| None::<String>),
    }
}

impl PromptMedia {
    pub fn sync_query(&mut self, value: &str) {
        let query = inline_media_query(value).map(|query| query.query.to_string());
        if *self.selected_query.peek() == query {
            return;
        }
        self.selected_query.set(query);
        self.selected.set(0);
    }

    pub fn remove_attachment(&self, index: usize) {
        let state = self.state.read();
        if state.open_id != self.open_id {
            return;
        }
        let Some(attachment) = state.attachments.get(index) else {
            return;
        };
        let _ = send(&CommandPaletteRemoveAttachmentRequest {
            open_id: self.open_id,
            path: attachment.path.clone(),
        });
    }

    pub fn highlighted(&self, entries: usize) -> usize {
        (self.selected)().min(entries.saturating_sub(1))
    }

    pub fn options(entries: &[ChatMediaEntry]) -> Vec<PromptMediaOption> {
        let mut options = Vec::with_capacity(entries.len());
        for entry in entries {
            options.push(PromptMediaOption {
                key: format!("media-{}", entry.path),
                name: entry.name.clone(),
                display_path: entry.display_path(),
                preview_data_url: entry.preview_data_url.clone(),
                label: FilePath(&entry.name).extension_label(),
                is_dir: entry.is_dir,
            });
        }
        options
    }

    pub fn loading(&self, query: &str) -> bool {
        let Some(query) = inline_media_query(query).map(|query| query.query) else {
            return false;
        };
        let state = self.state.read();
        if state.open_id != self.open_id || state.media_query.as_deref() != Some(query) {
            return true;
        }
        state.media_loading
    }

    pub fn composer_attachments(attachments: &[ChatAttachment]) -> Vec<PromptComposerAttachment> {
        PromptComposerAttachment::removable(attachments, &HashMap::new())
    }

    pub fn handle_key(
        &mut self,
        event: &KeyboardEvent,
        go_down: bool,
        go_up: bool,
        query: Signal<String>,
    ) -> bool {
        let value = query.peek().clone();
        let entries = self.entries(&value);
        let highlighted = (self.selected)().min(entries.len().saturating_sub(1));
        if go_down {
            event.prevent_default();
            let last = entries.len().saturating_sub(1);
            self.selected.set((highlighted + 1).min(last));
            return true;
        }
        if go_up {
            event.prevent_default();
            self.selected.set(highlighted.saturating_sub(1));
            return true;
        }
        if event.key() == Key::Enter && !event.modifiers().shift() {
            event.prevent_default();
            self.pick(entries.get(highlighted), query);
            return true;
        }
        if event.key() == Key::Escape {
            event.prevent_default();
            if let Some(found) = inline_media_query(&value) {
                let mut query = query;
                query.set(replace_inline_media_query(&value, found, ""));
            }
            self.selected.set(0);
            return true;
        }
        false
    }

    pub fn pick_at(&mut self, index: usize, query: Signal<String>) {
        let value = query.peek().clone();
        let entries = self.entries(&value);
        self.pick(entries.get(index), query);
    }

    pub fn entries(&self, query: &str) -> Vec<ChatMediaEntry> {
        let Some(query) = inline_media_query(query).map(|query| query.query) else {
            return Vec::new();
        };
        let state = self.state.read();
        if state.open_id != self.open_id || state.media_query.as_deref() != Some(query) {
            return Vec::new();
        }
        state.media_entries.clone()
    }

    fn pick(&mut self, entry: Option<&ChatMediaEntry>, mut query: Signal<String>) {
        let Some(entry) = entry else {
            return;
        };
        let value = query.peek().clone();
        let Some(media_query) = inline_media_query(&value) else {
            return;
        };
        let reference = entry.reference();
        let replacement = if entry.is_dir {
            format!("@{reference}/")
        } else {
            if send(&ChatAttachPaths {
                paths: vec![entry.path.clone()],
            })
            .is_err()
            {
                return;
            }
            String::new()
        };
        query.set(replace_inline_media_query(
            &value,
            media_query,
            &replacement,
        ));
        self.selected.set(0);
        focus_prompt_end(PROMPT_INPUT_ID);
    }
}
