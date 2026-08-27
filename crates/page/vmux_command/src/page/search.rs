use crate::event::{
    HISTORY_SUGGESTIONS_RESPONSE_EVENT, HistoryEntry, HistorySuggestionsRequest,
    HistorySuggestionsResponse, PATH_COMPLETE_RESPONSE, PathCompleteRequest, PathCompleteResponse,
    PathEntry,
};
use crate::page::signals::PaletteSignals;
use dioxus::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use vmux_ui::hooks::{send, use_listener};
use vmux_ui::launcher::palette::{CompletionQuery, PaletteDraft, PaletteSurface};
use vmux_ui::platform::sleep_ms;

const HOST_SEARCH_DEBOUNCE_MS: u32 = 300;

const HISTORY_SUGGESTION_LIMIT: u32 = 5;

#[derive(Clone, Default)]
pub struct HostSearchTimer(Rc<RefCell<Option<Rc<Cell<bool>>>>>);

impl HostSearchTimer {
    pub fn cancel(&self) {
        if let Some(cancelled) = self.0.borrow_mut().take() {
            cancelled.set(true);
        }
    }

    pub fn schedule(&self, callback: impl FnOnce() + 'static) {
        self.cancel();
        let cancelled = Rc::new(Cell::new(false));
        *self.0.borrow_mut() = Some(cancelled.clone());
        let slot = self.clone();
        spawn(async move {
            sleep_ms(HOST_SEARCH_DEBOUNCE_MS).await;
            if cancelled.get() {
                return;
            }
            slot.0.borrow_mut().take();
            callback();
        });
    }
}

#[derive(Clone, Default)]
pub struct HostSearch {
    pub completions: HostSearchTimer,
    pub suggestions: HostSearchTimer,
    pub media: HostSearchTimer,
}

pub fn use_host_search() -> HostSearch {
    use_hook(HostSearch::default)
}

impl HostSearch {
    pub fn cancel_all(&self) {
        self.completions.cancel();
        self.suggestions.cancel();
        self.media.cancel();
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct PaletteFeeds {
    pub completions: Signal<Vec<PathEntry>>,
    pub completion_id: Signal<u64>,
    pub suggestions: Signal<Vec<HistoryEntry>>,
    pub suggestion_id: Signal<u64>,
}

pub fn use_palette_feeds() -> PaletteFeeds {
    PaletteFeeds {
        completions: use_signal(Vec::<PathEntry>::new),
        completion_id: use_signal(|| 0u64),
        suggestions: use_signal(Vec::<HistoryEntry>::new),
        suggestion_id: use_signal(|| 0u64),
    }
}

impl PaletteFeeds {
    pub fn draft(&self, signals: PaletteSignals) -> PaletteDraft {
        PaletteDraft {
            query: (signals.query)(),
            target_url: (signals.target_url)(),
            completions: (self.completions)(),
            history: (self.suggestions)(),
            ..PaletteDraft::default()
        }
    }

    pub fn clear(&self) {
        let mut completions = self.completions;
        let mut suggestions = self.suggestions;
        completions.set(Vec::new());
        suggestions.set(Vec::new());
    }

    pub fn watch(&self) {
        let _ = (self.completions)();
        let _ = (self.suggestions)();
    }

    pub fn listen(self, signals: PaletteSignals, search: &HostSearch, surface: PaletteSurface) {
        self.complete_paths(signals, search.completions.clone());
        self.suggest_history(signals, search.suggestions.clone(), surface);
    }

    fn complete_paths(self, signals: PaletteSignals, timer: HostSearchTimer) {
        let mut completions = self.completions;
        let mut request_id = self.completion_id;
        let _response =
            use_listener::<PathCompleteResponse, _>(PATH_COMPLETE_RESPONSE, move |data| {
                completions.set(data.completions);
            });

        let query = signals.query;
        use_effect(move || {
            let typed = query();
            let id = (*request_id.peek()).wrapping_add(1).max(1);
            request_id.set(id);
            let Some(path_query) = CompletionQuery::of(&typed) else {
                timer.cancel();
                completions.set(Vec::new());
                return;
            };
            timer.schedule(move || {
                if *request_id.peek() != id {
                    return;
                }
                let _ = send(&PathCompleteRequest { query: path_query });
            });
        });
    }

    fn suggest_history(
        self,
        signals: PaletteSignals,
        timer: HostSearchTimer,
        surface: PaletteSurface,
    ) {
        let mut suggestions = self.suggestions;
        let mut request_id = self.suggestion_id;
        let _response = use_listener::<HistorySuggestionsResponse, _>(
            HISTORY_SUGGESTIONS_RESPONSE_EVENT,
            move |response| {
                if response.request_id != *request_id.read() {
                    return;
                }
                suggestions.set(response.entries);
            },
        );

        let query = signals.query;
        let is_start = surface.is_start();
        use_effect(move || {
            if is_start {
                timer.cancel();
                suggestions.set(Vec::new());
                return;
            }
            let typed = query();
            let trimmed = typed.trim();
            let id = (*request_id.peek()).wrapping_add(1).max(1);
            request_id.set(id);
            if HistoryQuery::of(trimmed).is_none() {
                timer.cancel();
                suggestions.set(Vec::new());
                return;
            }
            let query = trimmed.to_string();
            timer.schedule(move || {
                if *request_id.peek() != id {
                    return;
                }
                let _ = send(&HistorySuggestionsRequest {
                    query,
                    limit: HISTORY_SUGGESTION_LIMIT,
                    request_id: id,
                });
            });
        });
    }
}

pub struct HistoryQuery;

impl HistoryQuery {
    pub fn of(trimmed: &str) -> Option<&str> {
        if trimmed.is_empty()
            || trimmed.starts_with('>')
            || trimmed.starts_with('/')
            || trimmed.starts_with('~')
            || trimmed.starts_with("vmux://")
            || trimmed.starts_with("file:")
        {
            return None;
        }
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_asked_only_for_text_that_could_be_a_visited_page() {
        assert_eq!(HistoryQuery::of("rust docs"), Some("rust docs"));
        assert_eq!(HistoryQuery::of("example.com"), Some("example.com"));
        assert_eq!(HistoryQuery::of(""), None);
        assert_eq!(HistoryQuery::of("> close"), None);
        assert_eq!(HistoryQuery::of("/usr/bin"), None);
        assert_eq!(HistoryQuery::of("~/notes"), None);
        assert_eq!(HistoryQuery::of("vmux://settings/"), None);
        assert_eq!(HistoryQuery::of("file:///tmp/a"), None);
    }
}
