use dioxus::prelude::*;
use vmux_ui::hooks::use_ui_state;

use crate::state::LayoutUiState;

#[derive(Clone, Copy)]
pub(crate) struct LayoutUi {
    state: Signal<LayoutUiState>,
    error: Signal<Option<String>>,
}

impl LayoutUi {
    pub(crate) fn use_state() -> Self {
        let root = use_ui_state::<LayoutUiState>();
        Self {
            state: root.state,
            error: root.error,
        }
    }

    pub(crate) fn provide(self) {
        use_context_provider(|| self);
    }

    pub(crate) fn current() -> Self {
        use_context::<Self>()
    }

    pub(crate) fn value(self) -> LayoutUiState {
        (self.state)()
    }

    pub(crate) fn error(self) -> Option<String> {
        (self.error)()
    }
}
