use crate::event::{
    CommandBarUiState, HistoryEntry, HistorySuggestionsRequest, HistorySuggestionsResponse,
    PathCompleteRequest, PathCompleteResponse, PathEntry,
};
use crate::ui::signals::PaletteSignals;
use dioxus::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use vmux_api::chat::ResumableSessionEntry;
use vmux_ui::hooks::{send, use_ui_state_patch};
use vmux_ui::launcher::palette::{CompletionQuery, PaletteDraft, PaletteSurface};
use vmux_ui::platform::sleep_ms;

pub const HOST_SEARCH_DEBOUNCE_MS: u32 = 300;

const COMPLETION_DEBOUNCE_MS: u32 = 60;

const HISTORY_SUGGESTION_LIMIT: u32 = 5;

#[derive(Clone, Default)]
pub struct HostSearchTimer(Rc<RefCell<Option<Rc<Cell<bool>>>>>);

impl HostSearchTimer {
    pub fn cancel(&self) {
        if let Some(cancelled) = self.0.borrow_mut().take() {
            cancelled.set(true);
        }
    }

    pub fn schedule(&self, delay_ms: u32, callback: impl FnOnce() + 'static) {
        self.cancel();
        let cancelled = Rc::new(Cell::new(false));
        *self.0.borrow_mut() = Some(cancelled.clone());
        let slot = self.clone();
        spawn(async move {
            sleep_ms(delay_ms).await;
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
    pub completions_partial: Signal<bool>,
    pub completions_total: Signal<usize>,
    pub completion_id: Signal<u64>,
    pub suggestions: Signal<Vec<HistoryEntry>>,
    pub suggestion_id: Signal<u64>,
    pub sessions: Signal<Vec<ResumableSessionEntry>>,
    pub sessions_asked: Signal<bool>,
    pub sessions_total: Signal<u32>,
    pub sessions_loading: Signal<bool>,
}

pub fn use_palette_feeds() -> PaletteFeeds {
    PaletteFeeds {
        completions: use_signal(Vec::<PathEntry>::new),
        completions_partial: use_signal(|| false),
        completions_total: use_signal(|| 0usize),
        completion_id: use_signal(|| 0u64),
        suggestions: use_signal(Vec::<HistoryEntry>::new),
        suggestion_id: use_signal(|| 0u64),
        sessions: use_signal(Vec::<ResumableSessionEntry>::new),
        sessions_asked: use_signal(|| false),
        sessions_total: use_signal(|| 0u32),
        sessions_loading: use_signal(|| false),
    }
}

impl PaletteFeeds {
    pub fn draft(&self, signals: PaletteSignals) -> PaletteDraft {
        PaletteDraft {
            query: (signals.query)(),
            target_url: (signals.target_url)(),
            completions: (self.completions)(),
            completions_partial: (self.completions_partial)(),
            completions_total: (self.completions_total)(),
            history: (self.suggestions)(),
            sessions: (self.sessions)(),
            sessions_pending: (self.sessions_loading)(),
            ..PaletteDraft::default()
        }
    }

    pub fn clear(&self) {
        let mut completions = self.completions;
        let mut partial = self.completions_partial;
        let mut total = self.completions_total;
        let mut suggestions = self.suggestions;
        completions.set(Vec::new());
        partial.set(false);
        total.set(0);
        suggestions.set(Vec::new());
    }

    pub fn watch(&self) {
        let _ = (self.completions)();
        let _ = (self.completions_partial)();
        let _ = (self.completions_total)();
        let _ = (self.suggestions)();
    }

    pub fn listen(self, signals: PaletteSignals, search: &HostSearch, surface: PaletteSurface) {
        self.complete_paths(signals, search.completions.clone());
        self.suggest_history(signals, search.suggestions.clone(), surface);
    }

    fn complete_paths(self, signals: PaletteSignals, timer: HostSearchTimer) {
        let mut completions = self.completions;
        let mut partial = self.completions_partial;
        let mut total = self.completions_total;
        let mut request_id = self.completion_id;
        let responses = use_ui_state_patch::<CommandBarUiState, PathCompleteResponse>();
        use_effect(move || {
            let Some(data) = responses.take().into_iter().last() else {
                return;
            };
            if data.request_id != *request_id.read() {
                return;
            }
            completions.set(data.completions);
            partial.set(data.truncated);
            total.set(data.total as usize);
        });

        let query = signals.query;
        use_effect(move || {
            let typed = query();
            let id = (*request_id.peek()).wrapping_add(1).max(1);
            request_id.set(id);
            let Some(path_query) = CompletionQuery::parse(&typed) else {
                timer.cancel();
                completions.set(Vec::new());
                partial.set(false);
                total.set(0);
                return;
            };
            timer.schedule(COMPLETION_DEBOUNCE_MS, move || {
                if *request_id.peek() != id {
                    return;
                }
                let _ = send(&PathCompleteRequest {
                    request_id: id,
                    query: path_query,
                });
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
        let responses = use_ui_state_patch::<CommandBarUiState, HistorySuggestionsResponse>();
        use_effect(move || {
            let Some(response) = responses.take().into_iter().last() else {
                return;
            };
            if response.request_id != *request_id.read() {
                return;
            }
            suggestions.set(response.entries);
        });

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
            if HistoryQuery::parse(trimmed).is_none() {
                timer.cancel();
                suggestions.set(Vec::new());
                return;
            }
            let query = trimmed.to_string();
            timer.schedule(HOST_SEARCH_DEBOUNCE_MS, move || {
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
    pub fn parse(trimmed: &str) -> Option<&str> {
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
        assert_eq!(HistoryQuery::parse("rust docs"), Some("rust docs"));
        assert_eq!(HistoryQuery::parse("example.com"), Some("example.com"));
        assert_eq!(HistoryQuery::parse(""), None);
        assert_eq!(HistoryQuery::parse("> close"), None);
        assert_eq!(HistoryQuery::parse("/usr/bin"), None);
        assert_eq!(HistoryQuery::parse("~/notes"), None);
        assert_eq!(HistoryQuery::parse("vmux://settings/"), None);
        assert_eq!(HistoryQuery::parse("file:///tmp/a"), None);
    }
}
