use crate::page::search::{HostSearch, HostSearchTimer};
use crate::page::signals::PaletteSignals;
use crate::prompt_media::{
    ChatAttachPaths, ChatAttachment, ChatMediaEntries, ChatMediaEntry, ChatMediaListRequest,
    inline_media_query, replace_inline_media_query,
};
use dioxus::prelude::*;
use std::collections::HashMap;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, PromptComposerAttachment, focus_prompt_end};
use vmux_ui::components::prompt_media_options::PromptMediaOption;
use vmux_ui::hooks::send;
use vmux_ui::launcher::palette::PaletteSurface;

#[derive(Clone, Copy, PartialEq)]
pub struct PromptMedia {
    pub attachments: Signal<Vec<ChatAttachment>>,
    pub previews: Signal<HashMap<String, ChatAttachment>>,
    pub entries: Signal<Vec<ChatMediaEntry>>,
    pub request_id: Signal<u64>,
    pub requested_query: Signal<Option<String>>,
    pub loading: Signal<bool>,
    pub selected: Signal<usize>,
}

pub fn use_prompt_media() -> PromptMedia {
    PromptMedia {
        attachments: use_signal(Vec::<ChatAttachment>::new),
        previews: use_signal(HashMap::<String, ChatAttachment>::new),
        entries: use_signal(Vec::<ChatMediaEntry>::new),
        request_id: use_signal(|| 0u64),
        requested_query: use_signal(|| None::<String>),
        loading: use_signal(|| false),
        selected: use_signal(|| 0usize),
    }
}

impl PromptMedia {
    pub fn listen(self, signals: PaletteSignals, search: &HostSearch, surface: PaletteSurface) {
        self.search(signals, search.media.clone(), surface);
    }

    fn search(mut self, signals: PaletteSignals, timer: HostSearchTimer, surface: PaletteSurface) {
        let query = signals.query;
        let is_start = surface.is_start();
        use_effect(move || {
            if !is_start {
                return;
            }
            let value = query();
            let Some(media_query) = inline_media_query(&value).map(|found| found.query.to_string())
            else {
                self.request_id.set(self.next_request_id());
                timer.cancel();
                self.forget_matches();
                return;
            };
            if self.requested_query.peek().as_deref() == Some(media_query.as_str()) {
                return;
            }
            let request_id = self.next_request_id();
            self.request_id.set(request_id);
            self.requested_query.set(Some(media_query.clone()));
            self.entries.set(Vec::new());
            self.loading.set(true);
            self.selected.set(0);
            timer.schedule(crate::page::search::HOST_SEARCH_DEBOUNCE_MS, move || {
                if *self.request_id.peek() != request_id
                    || self.requested_query.peek().as_deref() != Some(media_query.as_str())
                {
                    return;
                }
                if send(&ChatMediaListRequest {
                    request_id,
                    query: media_query,
                })
                .is_err()
                {
                    self.loading.set(false);
                }
            });
        });
    }

    fn next_request_id(&self) -> u64 {
        (*self.request_id.peek()).wrapping_add(1).max(1)
    }

    pub fn reset(&mut self) {
        self.attachments.set(Vec::new());
        self.forget_matches();
    }

    fn forget_matches(&mut self) {
        self.entries.set(Vec::new());
        self.requested_query.set(None);
        self.loading.set(false);
        self.selected.set(0);
    }

    pub fn receive(&mut self, response: ChatMediaEntries) {
        if response.request_id != (self.request_id)() {
            return;
        }
        self.entries.set(response.entries.clone());
        self.loading.set(false);
        self.selected.set(0);
    }

    pub fn remember_previews(&mut self, loaded: &[ChatAttachment]) {
        let mut previews = self.previews.peek().clone();
        for attachment in loaded {
            previews.insert(attachment.path.clone(), attachment.clone());
        }
        self.previews.set(previews);
        let mut current = self.attachments.peek().clone();
        for preview in loaded {
            let Some(attachment) = current
                .iter_mut()
                .find(|attachment| attachment.path == preview.path)
            else {
                continue;
            };
            attachment.preview_data_url = preview.preview_data_url.clone();
        }
        self.attachments.set(current);
    }

    pub fn remove_attachment(&mut self, index: usize) {
        let mut next = self.attachments.peek().clone();
        if index >= next.len() {
            return;
        }
        next.remove(index);
        self.attachments.set(next);
    }

    pub fn highlighted(&self) -> usize {
        (self.selected)().min(self.entries.read().len().saturating_sub(1))
    }

    pub fn options(&self) -> Vec<PromptMediaOption> {
        let mut options = Vec::new();
        for entry in self.entries.read().iter() {
            options.push(PromptMediaOption {
                key: format!("media-{}", entry.path),
                name: entry.name.clone(),
                display_path: entry.display_path(),
                preview_data_url: entry.preview_data_url.clone(),
                label: FileLabel::of(&entry.name),
                is_dir: entry.is_dir,
            });
        }
        options
    }

    pub fn composer_attachments(&self) -> Vec<PromptComposerAttachment> {
        let previews = self.previews.read();
        let mut listed = Vec::new();
        for (index, attachment) in self.attachments.read().iter().enumerate() {
            let preview_data_url = match previews.get(&attachment.path) {
                Some(preview) => preview.preview_data_url.clone(),
                None => attachment.preview_data_url.clone(),
            };
            listed.push(PromptComposerAttachment {
                key: format!("start-attachment-{}", attachment.path),
                name: attachment.name.clone(),
                label: FileLabel::of(&attachment.name),
                preview_data_url,
                remove_index: Some(index),
            });
        }
        listed
    }

    pub fn handle_key(
        &mut self,
        event: &KeyboardEvent,
        go_down: bool,
        go_up: bool,
        query: Signal<String>,
    ) -> bool {
        let highlighted = self.highlighted();
        if go_down {
            event.prevent_default();
            let last = self.entries.read().len().saturating_sub(1);
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
            self.pick_at(highlighted, query);
            return true;
        }
        if event.key() == Key::Escape {
            event.prevent_default();
            let value = query.peek().clone();
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
        let Some(entry) = self.entries.peek().get(index).cloned() else {
            return;
        };
        self.pick(&entry, query);
    }

    fn pick(&mut self, entry: &ChatMediaEntry, mut query: Signal<String>) {
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

pub struct FileLabel;

impl FileLabel {
    pub fn of(name: &str) -> String {
        let Some(extension) = std::path::Path::new(name).extension() else {
            return "FILE".to_string();
        };
        let Some(extension) = extension.to_str() else {
            return "FILE".to_string();
        };
        let extension = extension.to_ascii_uppercase();
        if extension.is_empty() {
            return "FILE".to_string();
        }
        extension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_attachment_is_labelled_by_its_extension() {
        assert_eq!(FileLabel::of("shot.PNG"), "PNG");
        assert_eq!(FileLabel::of("notes.md"), "MD");
        assert_eq!(FileLabel::of("Makefile"), "FILE");
        assert_eq!(FileLabel::of(".gitignore"), "FILE");
        assert_eq!(FileLabel::of("archive.tar.gz"), "GZ");
    }
}
