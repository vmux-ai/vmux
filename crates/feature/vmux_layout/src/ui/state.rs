use dioxus::prelude::*;
use std::collections::HashMap;
use vmux_ui::hooks::{OptimisticMap, send, use_optimistic_map, use_ui_state};

use crate::state::LayoutUiState;

#[derive(Clone, Copy)]
pub(crate) struct LayoutUi {
    state: Signal<LayoutUiState>,
    error: Signal<Option<String>>,
    selected_stacks: OptimisticMap<u64, Option<u64>>,
}

impl LayoutUi {
    pub(crate) fn use_state() -> Self {
        let root = use_ui_state::<LayoutUiState>();
        let mut observed = HashMap::new();
        if let Some(sheet) = &root.state.read().side_sheet {
            for pane in &sheet.panes {
                let active = pane
                    .stacks
                    .iter()
                    .find(|stack| stack.is_active)
                    .map(|stack| stack.id);
                observed.insert(pane.id, active);
            }
        }
        Self {
            state: root.state,
            error: root.error,
            selected_stacks: use_optimistic_map(observed),
        }
    }

    pub(crate) fn provide(self) {
        use_context_provider(|| self);
    }

    pub(crate) fn current() -> Self {
        use_context::<Self>()
    }

    pub(crate) fn use_selector<S>(self, select: impl Fn(&LayoutUiState) -> S + 'static) -> Memo<S>
    where
        S: Clone + PartialEq + 'static,
    {
        use_memo(move || select(&self.state.read()))
    }

    pub(crate) fn error(self) -> Option<String> {
        (self.error)()
    }

    pub(crate) fn selected_stack(self, pane_id: u64, observed: Option<u64>) -> Option<u64> {
        self.selected_stacks.value(&pane_id, &observed)
    }

    pub(crate) fn select_stack(self, pane_id: u64, stack_id: u64) {
        let observed = {
            let state = self.state.read();
            let Some(sheet) = &state.side_sheet else {
                return;
            };
            let Some(pane) = sheet.panes.iter().find(|pane| pane.id == pane_id) else {
                return;
            };
            if !pane.stacks.iter().any(|stack| stack.id == stack_id) {
                return;
            }
            pane.stacks
                .iter()
                .find(|stack| stack.is_active)
                .map(|stack| stack.id)
        };
        self.selected_stacks.set(pane_id, observed, Some(stack_id));
        if send(&crate::event::SideSheetStackActivateRequest { pane_id, stack_id }).is_err() {
            self.selected_stacks.clear(&pane_id);
        }
    }
}
