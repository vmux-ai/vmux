use crate::event::{CommandPaletteState, OpenId};
use crate::ui::signals::PaletteSignals;
use dioxus::prelude::*;
use vmux_ui::launcher::palette::PaletteDraft;

#[derive(Clone, Copy)]
pub struct PaletteFeeds {
    state: Signal<CommandPaletteState>,
    open_id: OpenId,
}

impl PaletteFeeds {
    pub fn new(state: Signal<CommandPaletteState>, open_id: OpenId) -> Self {
        Self { state, open_id }
    }

    pub fn draft(&self, signals: PaletteSignals) -> PaletteDraft {
        let state = self.state.read();
        let mut draft = PaletteDraft {
            query: (signals.query)(),
            target_url: (signals.target_url)(),
            ..PaletteDraft::default()
        };
        if state.open_id != self.open_id {
            return draft;
        }
        draft.completions.clone_from(&state.completions);
        draft.completions_partial = state.completions_partial;
        draft.completions_total = state.completions_total as usize;
        draft.history.clone_from(&state.history);
        draft.sessions.clone_from(&state.sessions);
        draft.sessions_pending = state.sessions_loading;
        draft
    }

    pub fn watch(&self) {
        let _ = self.state.read().open_id;
    }
}
