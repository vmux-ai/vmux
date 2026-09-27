use std::collections::BTreeMap;

use dioxus::prelude::*;

use crate::event::{
    TermCursor, TermLine, TermThemeEvent, TermViewportPatch, TerminalUiState, TerminalUiStatePatch,
};
use vmux_ui::hooks::use_ui_state_root;

#[derive(Clone, PartialEq)]
pub(crate) struct TerminalRowState {
    pub(crate) line: TermLine,
    pub(crate) cursor: Option<TermCursor>,
}

#[derive(Clone, Copy)]
pub(crate) struct TerminalState {
    pub(crate) rows: Signal<BTreeMap<u32, Signal<TerminalRowState>>>,
    pub(crate) first_row: Signal<u32>,
    pub(crate) raw_title: Signal<String>,
    pub(crate) total_rows: Signal<u32>,
    pub(crate) alt: Signal<bool>,
    pub(crate) mouse: Signal<bool>,
    pub(crate) cols: Signal<u16>,
    pub(crate) cursor: Signal<Option<TermCursor>>,
    pub(crate) selection: Signal<Option<crate::event::TermSelectionRange>>,
    pub(crate) copy_mode: Signal<bool>,
    pub(crate) theme: Signal<Option<TermThemeEvent>>,
    pub(crate) service_error: Signal<String>,
    pub(crate) loading: Signal<Option<(String, String)>>,
    pub(crate) prompt_draft: Signal<(String, bool)>,
}

impl TerminalState {
    pub(crate) fn use_state() -> Self {
        let state = Self {
            rows: use_signal(BTreeMap::new),
            first_row: use_signal(|| 0),
            raw_title: use_signal(String::new),
            total_rows: use_signal(|| 0),
            alt: use_signal(|| false),
            mouse: use_signal(|| false),
            cols: use_signal(|| 0),
            cursor: use_signal(|| None),
            selection: use_signal(|| None),
            copy_mode: use_signal(|| false),
            theme: use_signal(|| None),
            service_error: use_signal(String::new),
            loading: use_signal(|| None),
            prompt_draft: use_signal(|| (String::new(), false)),
        };
        state.listen();
        state
    }

    fn listen(self) {
        let root = use_ui_state_root::<TerminalUiState>();
        let mut handled_sequence = use_signal(|| 0);
        use_effect(move || {
            let event = root.state.read();
            if event.sequence == 0 || event.sequence == *handled_sequence.peek() {
                return;
            }
            handled_sequence.set(event.sequence);
            for patch in &event.patches {
                self.apply(patch);
            }
        });
    }

    fn apply(self, patch: &TerminalUiStatePatch) {
        if let Some(event) = &patch.service_unavailable {
            let mut service_error = self.service_error;
            service_error.set(event.message.clone());
        }
        if let Some(viewport) = &patch.viewport {
            self.apply_viewport(viewport);
        }
        if let Some(event) = &patch.theme {
            let mut theme = self.theme;
            theme.set(Some(event.clone()));
        }
        if let Some(event) = &patch.title {
            let mut raw_title = self.raw_title;
            raw_title.set(event.title.clone());
        }
        if let Some(event) = &patch.loading {
            let mut loading = self.loading;
            let mut prompt_draft = self.prompt_draft;
            loading.set(if event.loading {
                Some((event.label.clone(), event.segment.clone()))
            } else {
                prompt_draft.set((String::new(), false));
                None
            });
        }
        if let Some(event) = &patch.prompt_draft {
            let mut prompt_draft = self.prompt_draft;
            prompt_draft.set((event.draft.clone(), event.skipped));
        }
    }

    fn apply_viewport(self, patch: &TermViewportPatch) {
        let mut first_row = self.first_row;
        let mut total_rows = self.total_rows;
        let mut alt = self.alt;
        let mut mouse = self.mouse;
        let mut cols = self.cols;
        let mut rows = self.rows;
        let mut selection = self.selection;
        let mut copy_mode = self.copy_mode;
        let mut cursor = self.cursor;

        let first = patch.first_row;
        if *first_row.peek() != first {
            first_row.set(first);
        }
        if *total_rows.peek() != patch.total_rows {
            total_rows.set(patch.total_rows);
        }
        if *alt.peek() != patch.alt {
            alt.set(patch.alt);
        }
        if *mouse.peek() != patch.mouse {
            mouse.set(patch.mouse);
        }
        if *cols.peek() != patch.cols {
            cols.set(patch.cols);
        }

        let overscan = vmux_core::scroll::overscan_for(
            patch.rows,
            vmux_core::scroll::TERMINAL_OVERSCAN_K,
            vmux_core::scroll::OVERSCAN_FLOOR,
            vmux_core::scroll::OVERSCAN_CAP,
        );
        let keep_hi =
            (first + patch.rows as u32 + overscan * 2 + 2).min(patch.total_rows.saturating_sub(1));
        let previous_cursor = cursor.peek().clone();
        let next_cursor = patch.cursor.clone();
        if patch.full {
            let next = patch
                .changed_lines
                .iter()
                .filter(|(doc_row, _)| *doc_row >= first && *doc_row <= keep_hi)
                .map(|(doc_row, line)| {
                    (
                        *doc_row,
                        Signal::new(TerminalRowState {
                            line: line.clone(),
                            cursor: (next_cursor.row == *doc_row).then_some(next_cursor.clone()),
                        }),
                    )
                })
                .collect();
            rows.set(next);
        } else {
            let mut missing = Vec::new();
            for (doc_row, line) in &patch.changed_lines {
                let state = TerminalRowState {
                    line: line.clone(),
                    cursor: (next_cursor.row == *doc_row).then_some(next_cursor.clone()),
                };
                if let Some(mut existing) = rows.peek().get(doc_row).copied() {
                    if *existing.peek() != state {
                        existing.set(state);
                    }
                } else {
                    missing.push((*doc_row, state));
                }
            }

            if previous_cursor.as_ref().map(|cursor| cursor.row) != Some(next_cursor.row)
                && let Some(old_row) = previous_cursor.as_ref().map(|cursor| cursor.row)
                && !patch.changed_row_indices().any(|row| row == old_row)
                && let Some(mut state) = rows.peek().get(&old_row).copied()
                && state.peek().cursor.is_some()
            {
                let line = state.peek().line.clone();
                state.set(TerminalRowState { line, cursor: None });
            }
            if !patch
                .changed_row_indices()
                .any(|row| row == next_cursor.row)
                && let Some(mut state) = rows.peek().get(&next_cursor.row).copied()
            {
                let current = state.peek().clone();
                if current.cursor.as_ref() != Some(&next_cursor) {
                    state.set(TerminalRowState {
                        line: current.line,
                        cursor: Some(next_cursor.clone()),
                    });
                }
            }

            let prune = rows
                .peek()
                .keys()
                .any(|doc_row| *doc_row < first || *doc_row > keep_hi);
            if !missing.is_empty() || prune {
                rows.with_mut(|map| {
                    for (doc_row, state) in missing {
                        map.insert(doc_row, Signal::new(state));
                    }
                    map.retain(|doc_row, _| *doc_row >= first && *doc_row <= keep_hi);
                });
            }
        }

        if *selection.peek() != patch.selection {
            selection.set(patch.selection);
        }
        if *copy_mode.peek() != patch.copy_mode {
            copy_mode.set(patch.copy_mode);
        }
        if cursor.peek().as_ref() != Some(&patch.cursor) {
            cursor.set(Some(patch.cursor.clone()));
        }
    }
}
