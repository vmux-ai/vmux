use dioxus::prelude::*;
use vmux_ui::hooks::use_ui_state;
use vmux_ui::scroll::ScrollIntoView;

use crate::event::{GitDiffRow, GitDiffViewport};
use crate::state::{
    GitDiffAppend, GitPageControllerState, GitPageSnapshot, GitProjects, GitUiState, HeaderDropdown,
};

#[derive(Clone, Copy)]
pub(super) struct GitPageState {
    pub(super) snapshot: Memo<GitPageSnapshot>,
    pub(super) controller: Memo<GitPageControllerState>,
    pub(super) header_dropdown: Memo<HeaderDropdown>,
    pub(super) projects: Memo<GitProjects>,
    pub(super) diff: GitDiffState,
    handled_selection_reveal: Signal<u64>,
    handled_diff_update: Signal<u64>,
}

impl GitPageState {
    pub(super) fn use_state() -> Self {
        let root = use_ui_state::<GitUiState>().state;
        let state = Self {
            snapshot: use_memo(move || root.read().snapshot.clone()),
            controller: use_memo(move || root.read().controller.clone()),
            header_dropdown: use_memo(move || root.read().header_dropdown),
            projects: use_memo(move || root.read().projects.clone()),
            diff: GitDiffState::new(),
            handled_selection_reveal: use_signal(|| 0),
            handled_diff_update: use_signal(|| 0),
        };
        state.subscribe(root);
        state
    }

    fn subscribe(self, root: Signal<GitUiState>) {
        use_effect(move || {
            let root = root.read();
            self.diff.synchronize(&root.snapshot);
            if root.diff_update_revision <= *self.handled_diff_update.peek() {
                return;
            }
            if let Some(update) = root.diff_update.as_ref() {
                self.diff.append(update);
            }
            let mut handled = self.handled_diff_update;
            handled.set(root.diff_update_revision);
        });
        use_effect(move || {
            let root = root.read();
            let Some(request) = root.selection_reveal.as_ref() else {
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

#[derive(Clone, Copy)]
pub(super) struct GitDiffState {
    pub(super) viewport: Signal<Option<GitDiffViewport>>,
    pub(super) rows: Signal<Vec<GitDiffRow>>,
    pub(super) loading: Signal<bool>,
}

impl GitDiffState {
    fn new() -> Self {
        Self {
            viewport: use_signal(|| None),
            rows: use_signal(Vec::new),
            loading: use_signal(|| false),
        }
    }

    fn synchronize(self, snapshot: &GitPageSnapshot) {
        let replace = {
            let viewport = self.viewport.peek();
            let rows = self.rows.peek();
            Self::replaces(viewport.as_ref(), &rows, snapshot)
        };
        if !replace {
            return;
        }
        let mut viewport = self.viewport;
        viewport.set(snapshot.diff_viewport.clone());
        let mut rows = self.rows;
        rows.set(snapshot.diff_rows.clone());
        let mut loading = self.loading;
        loading.set(snapshot.diff_loading);
    }

    fn replaces(
        current: Option<&GitDiffViewport>,
        current_rows: &[GitDiffRow],
        snapshot: &GitPageSnapshot,
    ) -> bool {
        let incoming = snapshot.diff_viewport.as_ref();
        match (current, incoming) {
            (None, None) => snapshot.diff_loading,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (Some(current), Some(incoming)) => {
                incoming.generation != current.generation
                    || incoming.first_line != current.first_line
                    || incoming.lines.len() > current.lines.len()
                    || incoming.lines.len() == current.lines.len()
                        && snapshot.diff_rows != current_rows
            }
        }
    }

    fn append(mut self, update: &GitDiffAppend) {
        let mut viewport = self.viewport.write();
        let mut rows = self.rows.write();
        if !Self::append_to(&mut viewport, &mut rows, update) {
            return;
        }
        drop(rows);
        drop(viewport);
        let mut loading = self.loading;
        loading.set(false);
    }

    fn append_to(
        viewport: &mut Option<GitDiffViewport>,
        rows: &mut Vec<GitDiffRow>,
        update: &GitDiffAppend,
    ) -> bool {
        let Some(current) = viewport.as_mut() else {
            return false;
        };
        let loaded = current
            .first_line
            .saturating_add(u32::try_from(current.lines.len()).unwrap_or(u32::MAX));
        if current.generation != update.viewport.generation || update.viewport.first_line != loaded
        {
            return false;
        }
        current.total_lines = update.viewport.total_lines;
        current.has_more = update.viewport.has_more;
        current.lines.extend(update.viewport.lines.clone());
        current.markers.extend(update.viewport.markers.clone());
        current.markers.sort_by_key(|marker| marker.line);
        current.markers.dedup();
        current.error.clone_from(&update.viewport.error);
        rows.retain(|row| row.end() <= update.rows_from);
        rows.extend(update.rows.iter().copied());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{DiffKind, DiffLine};

    fn line(number: u32) -> DiffLine {
        DiffLine {
            kind: DiffKind::Context,
            old_no: Some(number),
            new_no: Some(number),
            hunk: None,
            spans: Vec::new(),
        }
    }

    #[test]
    fn diff_updates_append_without_replacing_the_loaded_buffer() {
        let mut viewport = Some(GitDiffViewport {
            generation: 7,
            first_line: 0,
            total_lines: 4,
            has_more: true,
            lines: vec![line(1), line(2)],
            markers: Vec::new(),
            error: String::new(),
        });
        let mut rows = vec![GitDiffRow::Line(0), GitDiffRow::Line(1)];
        let update = GitDiffAppend {
            viewport: GitDiffViewport {
                generation: 7,
                first_line: 2,
                total_lines: 4,
                has_more: false,
                lines: vec![line(3), line(4)],
                markers: Vec::new(),
                error: String::new(),
            },
            rows_from: 1,
            rows: vec![
                GitDiffRow::Line(1),
                GitDiffRow::Line(2),
                GitDiffRow::Line(3),
            ],
        };

        assert!(GitDiffState::append_to(&mut viewport, &mut rows, &update));
        assert_eq!(viewport.unwrap().lines.len(), 4);
        assert_eq!(
            rows,
            [
                GitDiffRow::Line(0),
                GitDiffRow::Line(1),
                GitDiffRow::Line(2),
                GitDiffRow::Line(3)
            ]
        );
    }

    #[test]
    fn stale_diff_updates_are_ignored() {
        let mut viewport = Some(GitDiffViewport {
            generation: 8,
            first_line: 0,
            total_lines: 2,
            has_more: false,
            lines: vec![line(1)],
            markers: Vec::new(),
            error: String::new(),
        });
        let mut rows = vec![GitDiffRow::Line(0)];
        let update = GitDiffAppend {
            viewport: GitDiffViewport {
                generation: 7,
                first_line: 1,
                total_lines: 2,
                has_more: false,
                lines: vec![line(2)],
                markers: Vec::new(),
                error: String::new(),
            },
            rows_from: 0,
            rows: vec![GitDiffRow::Line(1)],
        };

        assert!(!GitDiffState::append_to(&mut viewport, &mut rows, &update));
        assert_eq!(viewport.unwrap().lines.len(), 1);
        assert_eq!(rows, [GitDiffRow::Line(0)]);
    }

    #[test]
    fn a_full_projection_replaces_rows_for_the_same_loaded_lines() {
        let viewport = GitDiffViewport {
            generation: 9,
            first_line: 0,
            total_lines: 2,
            has_more: false,
            lines: vec![line(1), line(2)],
            markers: Vec::new(),
            error: String::new(),
        };
        let snapshot = GitPageSnapshot {
            diff_viewport: Some(viewport.clone()),
            diff_rows: vec![GitDiffRow::Line(0), GitDiffRow::Line(1)],
            ..Default::default()
        };

        assert!(GitDiffState::replaces(
            Some(&viewport),
            &[GitDiffRow::Line(0)],
            &snapshot,
        ));
    }
}
