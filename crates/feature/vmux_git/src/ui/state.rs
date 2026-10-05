use dioxus::prelude::*;
use vmux_ui::hooks::use_ui_state;
use vmux_ui::scroll::ScrollIntoView;

use crate::state::{GitDirectoryState, GitPageControllerState, GitPageSnapshot, GitUiState};

#[derive(Clone, Copy)]
pub(super) struct GitPageState {
    pub(super) snapshot: Memo<GitPageSnapshot>,
    pub(super) controller: Memo<GitPageControllerState>,
    pub(super) directory: Memo<GitDirectoryState>,
    handled_selection_reveal: Signal<u64>,
}

impl GitPageState {
    pub(super) fn use_state() -> Self {
        let root = use_ui_state::<GitUiState>().state;
        let state = Self {
            snapshot: use_memo(move || root().snapshot),
            controller: use_memo(move || root().controller),
            directory: use_memo(move || root().directory),
            handled_selection_reveal: use_signal(|| 0),
        };
        state.subscribe(root);
        state
    }

    fn subscribe(self, root: Signal<GitUiState>) {
        use_effect(move || {
            let Some(request) = root().selection_reveal else {
                return;
            };
            if request.revision <= (self.handled_selection_reveal)() {
                return;
            }
            let mut handled = self.handled_selection_reveal;
            handled.set(request.revision);
            ScrollIntoView::nearest(&request.id);
        });
    }
}
