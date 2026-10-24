use dioxus::prelude::*;
use vmux_ui::hooks::UiState;

use crate::state::UiLayout;

#[derive(Clone, Copy)]
pub(crate) struct LayoutUi {
    state: Signal<UiLayout>,
    error: Signal<Option<String>>,
}

impl LayoutUi {
    pub(crate) fn use_state() -> Self {
        let root = UiState::<UiLayout>::use_context();
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

    pub(crate) fn use_selector<S>(self, select: impl Fn(&UiLayout) -> S + 'static) -> Memo<S>
    where
        S: Clone + PartialEq + 'static,
    {
        use_memo(move || select(&self.state.read()))
    }

    pub(crate) fn error(self) -> Option<String> {
        (self.error)()
    }
}
