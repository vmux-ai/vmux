use std::collections::BTreeMap;

use dioxus::prelude::*;

use crate::event::{
    TermCursor, TermLine, TermLoadingEvent, TermSelectionRange, TermThemeEvent, TerminalUiState,
};
use vmux_ui::hooks::use_ui_state;

#[derive(Clone, PartialEq)]
pub(crate) struct TerminalRowState {
    pub(crate) line: TermLine,
    pub(crate) cursor: Option<TermCursor>,
}

#[derive(Clone, Copy)]
pub(crate) struct TerminalState {
    pub(crate) rows: Memo<BTreeMap<u32, TerminalRowState>>,
    pub(crate) first_row: Memo<u32>,
    pub(crate) raw_title: Memo<String>,
    pub(crate) total_rows: Memo<u32>,
    pub(crate) alt: Memo<bool>,
    pub(crate) mouse: Memo<bool>,
    pub(crate) cols: Memo<u16>,
    pub(crate) cursor: Memo<Option<TermCursor>>,
    pub(crate) selection: Memo<Option<TermSelectionRange>>,
    pub(crate) copy_mode: Memo<bool>,
    pub(crate) theme: Memo<Option<TermThemeEvent>>,
    pub(crate) service_error: Memo<String>,
    pub(crate) loading: Memo<Option<(String, String)>>,
    pub(crate) prompt_draft: Memo<(String, bool)>,
}

impl TerminalState {
    pub(crate) fn use_state() -> Self {
        let root = use_ui_state::<TerminalUiState>().state;
        Self {
            rows: use_memo(move || {
                let state = root.read();
                let cursor = state.viewport.cursor.as_ref();
                state
                    .viewport
                    .rows
                    .iter()
                    .map(|(row, line)| {
                        (
                            *row,
                            TerminalRowState {
                                line: line.clone(),
                                cursor: cursor.filter(|cursor| cursor.row == *row).cloned(),
                            },
                        )
                    })
                    .collect()
            }),
            first_row: use_memo(move || root.read().viewport.first_row),
            raw_title: use_memo(move || root.read().title.clone()),
            total_rows: use_memo(move || root.read().viewport.total_rows),
            alt: use_memo(move || root.read().viewport.alt),
            mouse: use_memo(move || root.read().viewport.mouse),
            cols: use_memo(move || root.read().viewport.cols),
            cursor: use_memo(move || root.read().viewport.cursor.clone()),
            selection: use_memo(move || root.read().viewport.selection),
            copy_mode: use_memo(move || root.read().viewport.copy_mode),
            theme: use_memo(move || root.read().theme.clone()),
            service_error: use_memo(move || root.read().service_error.clone()),
            loading: use_memo(move || {
                root.read()
                    .loading
                    .as_ref()
                    .map(|TermLoadingEvent { label, segment, .. }| (label.clone(), segment.clone()))
            }),
            prompt_draft: use_memo(move || {
                let state = root.read();
                (state.prompt_draft.draft.clone(), state.prompt_draft.skipped)
            }),
        }
    }
}
