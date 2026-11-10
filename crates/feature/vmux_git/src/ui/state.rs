use dioxus::prelude::*;
use vmux_ui::hooks::{send, use_ui_state};
use vmux_ui::scroll::{ScrollIntoView, ScrollRetention};

use crate::event::GitRepositoryRequest;
use crate::event::{GitDiffRow, GitDiffViewport};
use crate::state::{
    GitDiffAppend, GitPageControllerState, GitPageSnapshot, GitPanel, GitProjects,
    GitReviewUiState, GitUiState,
};

#[derive(Clone, Copy)]
pub(super) struct GitPageState {
    root: Signal<GitUiState>,
    pub(super) snapshot: Memo<GitPageSnapshot>,
    pub(super) controller: Memo<GitPageControllerState>,
    pub(super) review: Signal<GitReviewUiState>,
    pub(super) projects: Memo<GitProjects>,
    pub(super) diff: GitDiffState,
    pub(super) location: Signal<vmux_api::page_location::PageLocationUiState>,
    pub(super) active_branch_file: Signal<String>,
    pending_repository: Signal<Option<PendingRepository>>,
    repository_request_sequence: Signal<u64>,
    handled_selection_reveal: Signal<u64>,
}

#[derive(Clone)]
struct PendingRepository {
    request: GitRepositoryRequest,
}

impl PendingRepository {
    fn waiting_for(&self, snapshot: &GitPageSnapshot) -> bool {
        snapshot.repository_request_id < self.request.request_id
    }
}

impl GitPageState {
    pub(super) fn use_state() -> Self {
        let root = use_ui_state::<GitUiState>().state;
        let pending_repository = use_signal(|| None::<PendingRepository>);
        let state = Self {
            root,
            snapshot: use_memo(move || {
                let root = root.read();
                let pending = pending_repository.read();
                let Some(request) = pending.as_ref() else {
                    return root.snapshot.clone();
                };
                if root.snapshot.repository_request_id == request.request.request_id {
                    return root.snapshot.clone();
                }
                GitPageSnapshot::loading(
                    &request.request,
                    root.snapshot.nonce.wrapping_add(1).max(1),
                )
            }),
            controller: use_memo(move || {
                let root = root.read();
                let pending = pending_repository.read();
                let Some(request) = pending.as_ref() else {
                    return root.controller.clone();
                };
                if root.snapshot.repository_request_id == request.request.request_id {
                    return root.controller.clone();
                }
                GitPageControllerState::for_repository(request.request.branch.clone())
            }),
            review: use_ui_state::<GitReviewUiState>().state,
            projects: use_memo(move || root.read().projects.clone()),
            diff: GitDiffState::new(root),
            location: use_ui_state::<vmux_api::page_location::PageLocationUiState>().state,
            active_branch_file: use_signal(String::new),
            pending_repository,
            repository_request_sequence: use_signal(|| 0),
            handled_selection_reveal: use_signal(|| 0),
        };
        state.subscribe(root);
        state
    }

    pub(super) fn request_repository(mut self, mut request: GitRepositoryRequest) {
        let root = self.root.peek();
        let retained = root.snapshot.repository_request_id;
        let request_id = (*self.repository_request_sequence.peek())
            .max(retained)
            .wrapping_add(1)
            .max(1);
        request.request_id = request_id;
        self.repository_request_sequence.set(request_id);
        self.pending_repository.set(Some(PendingRepository {
            request: request.clone(),
        }));
        self.diff.begin_repository(&request.path);
        if send(&request).is_err() {
            self.pending_repository.set(None);
            let root = self.root.peek();
            self.diff.synchronize(&root.snapshot, &root.controller);
        }
    }

    fn subscribe(self, mut root: Signal<GitUiState>) {
        use_effect(move || {
            let snapshot = root.read().snapshot.clone();
            let controller = root.read().controller.clone();
            let pending = self.pending_repository.peek().clone();
            if let Some(pending) = pending {
                if pending.waiting_for(&snapshot) {
                    return;
                }
                let mut pending_repository = self.pending_repository;
                pending_repository.set(None);
            }
            self.diff.synchronize(&snapshot, &controller);
            let updates = if root.read().diff_updates.is_empty() {
                Vec::new()
            } else {
                std::mem::take(&mut root.write().diff_updates)
            };
            self.diff.append_new(&updates);
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
    workspace: Signal<String>,
    selection: Signal<DiffSelection>,
    pub(super) viewport: Signal<Option<GitDiffViewport>>,
    pub(super) rows: Signal<Vec<GitDiffRow>>,
    pub(super) rows_from: Signal<Option<u32>>,
    pub(super) loading: Signal<bool>,
    update_cursor: Signal<DiffUpdateCursor>,
}

#[derive(Clone, PartialEq)]
enum DiffSelection {
    None,
    File { path: String, bytes: Vec<u8> },
    Branch(String),
    Commit(String),
    Stash(String),
}

impl DiffSelection {
    fn of(controller: &GitPageControllerState) -> Self {
        match controller.focused_panel {
            GitPanel::Files if !controller.selected_abs_path.is_empty() => Self::File {
                path: controller.selected_abs_path.clone(),
                bytes: controller.selected_path_bytes.clone(),
            },
            GitPanel::Branches if !controller.selected_branch.is_empty() => {
                Self::Branch(controller.selected_branch.clone())
            }
            GitPanel::Commits => Self::Commit(controller.selected_commit.clone()),
            GitPanel::Stash if !controller.selected_stash.is_empty() => {
                Self::Stash(controller.selected_stash.clone())
            }
            _ => Self::None,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct DiffUpdateCursor {
    consumed: usize,
    generation: u64,
    revision: u64,
}

impl DiffUpdateCursor {
    fn start(self, updates: &[GitDiffAppend]) -> usize {
        if self.consumed == 0 || self.consumed > updates.len() {
            return 0;
        }
        let previous = &updates[self.consumed - 1].viewport;
        if previous.generation == self.generation && previous.revision == self.revision {
            return self.consumed;
        }
        0
    }

    fn capture(&mut self, updates: &[GitDiffAppend]) {
        let Some(last) = updates.last() else {
            *self = Self::default();
            return;
        };
        self.consumed = updates.len();
        self.generation = last.viewport.generation;
        self.revision = last.viewport.revision;
    }
}

impl GitDiffState {
    fn new(root: Signal<GitUiState>) -> Self {
        Self {
            workspace: use_signal(move || root.peek().snapshot.workspace.clone()),
            selection: use_signal(move || DiffSelection::of(&root.peek().controller)),
            viewport: use_signal(move || root.peek().snapshot.diff_viewport.clone()),
            rows: use_signal(move || root.peek().snapshot.diff_rows.clone()),
            rows_from: use_signal(|| None),
            loading: use_signal(move || root.peek().snapshot.diff_loading),
            update_cursor: use_signal(DiffUpdateCursor::default),
        }
    }

    fn synchronize(self, snapshot: &GitPageSnapshot, controller: &GitPageControllerState) {
        let selection = DiffSelection::of(controller);
        if *self.workspace.peek() != snapshot.workspace || *self.selection.peek() != selection {
            self.begin_repository(&snapshot.workspace);
            let mut current = self.selection;
            current.set(selection);
        }
        let mut loading = self.loading;
        if *loading.peek() != snapshot.diff_loading {
            loading.set(snapshot.diff_loading);
        }
        let replace = {
            let viewport = self.viewport.peek();
            let rows = self.rows.peek();
            Self::replaces(viewport.as_ref(), &rows, snapshot)
        };
        if !replace {
            return;
        }
        let preserve_scroll = self
            .viewport
            .peek()
            .as_ref()
            .zip(snapshot.diff_viewport.as_ref())
            .is_some_and(|(current, incoming)| {
                current.generation == incoming.generation
                    && current.first_line == incoming.first_line
            });
        if preserve_scroll {
            ScrollRetention::preserve_next_render();
        }
        let mut viewport = self.viewport;
        viewport.set(snapshot.diff_viewport.clone());
        let mut rows = self.rows;
        rows.set(snapshot.diff_rows.clone());
        let mut rows_from = self.rows_from;
        rows_from.set(None);
        let mut update_cursor = self.update_cursor;
        update_cursor.set(DiffUpdateCursor::default());
    }

    fn begin_repository(mut self, workspace: &str) {
        self.workspace.set(workspace.to_string());
        self.viewport.set(None);
        self.rows.set(Vec::new());
        self.rows_from.set(None);
        self.loading.set(true);
        self.update_cursor.set(DiffUpdateCursor::default());
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
            (Some(_), None) => !snapshot.diff_loading,
            (Some(current), Some(incoming)) => {
                if incoming.generation != current.generation
                    || incoming.first_line != current.first_line
                {
                    return true;
                }
                if incoming.revision < current.revision {
                    return false;
                }
                incoming != current || snapshot.diff_rows != current_rows
            }
        }
    }

    fn append_new(mut self, updates: &[GitDiffAppend]) {
        let start = self.update_cursor.peek().start(updates);
        if start >= updates.len() {
            return;
        }
        let mut viewport = self.viewport.write();
        let mut rows = self.rows.write();
        let rows_from = Self::append_all_to(&mut viewport, &mut rows, &updates[start..]);
        let mut update_cursor = self.update_cursor;
        update_cursor.write().capture(updates);
        let Some(rows_from) = rows_from else {
            return;
        };
        self.rows_from.set(Some(rows_from));
        ScrollRetention::preserve_next_render();
        drop(rows);
        drop(viewport);
        let mut loading = self.loading;
        loading.set(false);
    }

    fn append_all_to(
        viewport: &mut Option<GitDiffViewport>,
        rows: &mut Vec<GitDiffRow>,
        updates: &[GitDiffAppend],
    ) -> Option<u32> {
        let mut rows_from = None;
        for update in updates {
            if let Some(changed_from) = Self::append_to(viewport, rows, update) {
                rows_from =
                    Some(rows_from.map_or(changed_from, |current: u32| current.min(changed_from)));
            }
        }
        rows_from
    }

    fn append_to(
        viewport: &mut Option<GitDiffViewport>,
        rows: &mut Vec<GitDiffRow>,
        update: &GitDiffAppend,
    ) -> Option<u32> {
        let current = viewport.as_mut()?;
        if !current.append(&update.viewport) {
            return None;
        }
        let retained = rows.partition_point(|row| row.end() <= update.rows_from);
        rows.truncate(retained);
        rows.extend(update.rows.iter().copied());
        Some(update.rows_from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{DiffKind, DiffLine};
    use std::{
        cell::RefCell,
        future::Future,
        rc::Rc,
        task::{Context, Waker},
    };
    use vmux_api::BinEvent;
    use vmux_ui::hooks::EventListenerError;
    use vmux_ui::transport::{BytesListener, HostScope, PageHost};

    #[derive(Default)]
    struct PreviewHost(RefCell<Vec<BytesListener>>);

    impl PageHost for PreviewHost {
        fn send(&self, _: &str, _: &[u8]) -> Result<(), EventListenerError> {
            Ok(())
        }

        fn listen(&self, id: &str, listener: BytesListener) -> Result<(), EventListenerError> {
            if id == GitUiState::id() {
                self.0.borrow_mut().push(listener);
            }
            Ok(())
        }
    }

    struct DiffPreview {
        host: Rc<PreviewHost>,
        dom: VirtualDom,
        _scope: HostScope,
    }

    impl DiffPreview {
        fn new() -> Self {
            let host = Rc::new(PreviewHost::default());
            let scope = HostScope::enter(host.clone());
            let mut dom = VirtualDom::new(CachedDiffPreview);
            dom.provide_root_context(vmux_ui::transport::HostBinding(host.clone()));
            dom.rebuild_to_vec();
            let mut preview = Self {
                host,
                dom,
                _scope: scope,
            };
            preview.render();
            preview
        }

        fn publish(&mut self, state: GitUiState) -> String {
            let frame =
                vmux_api::UiStateFrame::<GitUiState, crate::state::GitUiStatePatch>::Snapshot(
                    state,
                );
            self.receive(frame);
            self.render()
        }

        fn receive(
            &mut self,
            frame: vmux_api::UiStateFrame<GitUiState, crate::state::GitUiStatePatch>,
        ) {
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap();
            for listener in self.host.0.borrow_mut().iter_mut() {
                listener(&bytes);
            }
        }

        fn render(&mut self) -> String {
            for _ in 0..4 {
                let mut context = Context::from_waker(Waker::noop());
                let _ = std::pin::pin!(self.dom.wait_for_work()).poll(&mut context);
                self.dom.render_immediate_to_vec();
            }
            dioxus_ssr::render(&self.dom)
        }
    }

    #[component]
    fn CachedDiffPreview() -> Element {
        let state = GitPageState::use_state();
        let viewport = (state.diff.viewport)();
        let rows = (state.diff.rows)();
        rsx! { div {
            "data-revision": viewport.as_ref().map(|viewport| viewport.revision).unwrap_or_default(),
            "data-lines": viewport.as_ref().map(|viewport| viewport.lines.len()).unwrap_or_default(),
            if viewport.is_some() && !rows.is_empty() { "populated diff" } else { "empty diff" }
        } }
    }

    #[test]
    fn diff_chunks_survive_other_updates_before_the_next_document_frame() {
        let mut preview = DiffPreview::new();
        let controller = GitPageControllerState {
            focused_panel: GitPanel::Branches,
            selected_branch: "feature".to_string(),
            ..Default::default()
        };
        preview.publish(GitUiState {
            snapshot: GitPageSnapshot {
                workspace: "/repo".to_string(),
                diff_viewport: Some(GitDiffViewport {
                    generation: 1,
                    revision: 1,
                    first_line: 0,
                    total_lines: 3,
                    has_more: true,
                    lines: vec![line(1)],
                    markers: Vec::new(),
                    error: String::new(),
                }),
                diff_rows: vec![GitDiffRow::Line(0)],
                ..Default::default()
            },
            controller: controller.clone(),
            ..Default::default()
        });
        for revision in 2..=3 {
            preview.receive(vmux_api::UiStateFrame::Updates(vec![
                GitDiffAppend {
                    viewport: GitDiffViewport {
                        generation: 1,
                        revision,
                        first_line: revision as u32 - 1,
                        total_lines: 3,
                        has_more: revision < 3,
                        lines: vec![line(revision as u32)],
                        markers: Vec::new(),
                        error: String::new(),
                    },
                    rows_from: revision as u32 - 1,
                    rows: vec![GitDiffRow::Line(revision as u32 - 1)],
                }
                .into(),
            ]));
        }
        preview.receive(vmux_api::UiStateFrame::Updates(vec![controller.into()]));

        let rendered = preview.render();
        assert!(rendered.contains("data-revision=3"), "{rendered}");
        assert!(rendered.contains("data-lines=3"), "{rendered}");
    }

    #[test]
    fn a_loaded_diff_is_present_in_the_first_recovered_document_frame() {
        let host = Rc::new(PreviewHost::default());
        let _host = HostScope::enter(host.clone());
        let mut page = vmux_page::PageDom::mount(CachedDiffPreview, vmux_page::Instance::default());
        page.rebuild();
        for _ in 0..4 {
            page.flushed();
            let _ = page.render();
        }
        let state = GitUiState {
            snapshot: GitPageSnapshot {
                workspace: "/repo".to_string(),
                diff_viewport: Some(GitDiffViewport {
                    generation: 1,
                    revision: 1,
                    first_line: 0,
                    total_lines: 1,
                    has_more: false,
                    lines: vec![line(1)],
                    markers: Vec::new(),
                    error: String::new(),
                }),
                diff_rows: vec![GitDiffRow::Line(0)],
                ..Default::default()
            },
            ..Default::default()
        };
        let frame =
            vmux_api::UiStateFrame::<GitUiState, crate::state::GitUiStatePatch>::Snapshot(state);
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap();
        for listener in host.0.borrow_mut().iter_mut() {
            listener(&bytes);
        }
        for _ in 0..4 {
            page.flushed();
            let _ = page.render();
        }
        let recovered = page.rebuild();
        let recovered = String::from_utf8_lossy(&recovered);
        assert!(recovered.contains("populated diff"));
        assert!(!recovered.contains("empty diff"));
    }

    #[test]
    fn branch_changes_clear_old_diff_while_refresh_and_file_seek_keep_it() {
        let mut preview = DiffPreview::new();
        let loaded = GitUiState {
            snapshot: GitPageSnapshot {
                workspace: "/repo".to_string(),
                diff_viewport: Some(GitDiffViewport {
                    generation: 1,
                    revision: 1,
                    first_line: 0,
                    total_lines: 1,
                    has_more: false,
                    lines: vec![line(1)],
                    markers: Vec::new(),
                    error: String::new(),
                }),
                diff_rows: vec![GitDiffRow::Line(0)],
                ..Default::default()
            },
            controller: GitPageControllerState {
                focused_panel: GitPanel::Branches,
                selected_branch: "feature".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(preview.publish(loaded.clone()).contains("populated diff"));
        let mut pending = loaded.clone();
        pending.snapshot.diff_viewport = None;
        pending.snapshot.diff_rows.clear();
        pending.snapshot.diff_loading = true;
        assert!(!preview.publish(pending.clone()).contains("empty diff"));
        pending.controller.selected_branch_path = "second.rs".to_string();
        pending.controller.selected_branch_path_bytes = b"second.rs".to_vec();
        assert!(!preview.publish(pending.clone()).contains("empty diff"));
        pending.controller.selected_branch = "main".to_string();
        assert!(preview.publish(pending).contains("empty diff"));
        let mut incoming = loaded;
        incoming.controller.selected_branch = "main".to_string();
        assert!(preview.publish(incoming).contains("populated diff"));
    }

    fn line(number: u32) -> DiffLine {
        DiffLine {
            kind: DiffKind::Context,
            old_no: Some(number),
            new_no: Some(number),
            hunk: None,
            spans: Vec::new(),
        }
    }

    fn append(generation: u64, revision: u64) -> GitDiffAppend {
        GitDiffAppend {
            viewport: GitDiffViewport {
                generation,
                revision,
                first_line: 0,
                total_lines: 0,
                has_more: false,
                lines: Vec::new(),
                markers: Vec::new(),
                error: String::new(),
            },
            rows_from: 0,
            rows: Vec::new(),
        }
    }

    #[test]
    fn pending_repository_waits_until_acknowledged_or_superseded() {
        let pending = PendingRepository {
            request: GitRepositoryRequest {
                request_id: 7,
                ..GitRepositoryRequest::at("/repo")
            },
        };

        assert!(pending.waiting_for(&GitPageSnapshot {
            nonce: 4,
            ..Default::default()
        }));
        assert!(!pending.waiting_for(&GitPageSnapshot {
            repository_request_id: 7,
            nonce: 5,
            ..Default::default()
        }));
        assert!(pending.waiting_for(&GitPageSnapshot {
            repository_request_id: 0,
            nonce: 5,
            ..Default::default()
        }));
        assert!(!pending.waiting_for(&GitPageSnapshot {
            repository_request_id: 8,
            nonce: 5,
            ..Default::default()
        }));
    }

    #[test]
    fn diff_update_cursor_processes_only_unseen_updates() {
        let mut cursor = DiffUpdateCursor::default();
        let mut updates = vec![append(3, 1), append(3, 2)];

        assert_eq!(cursor.start(&updates), 0);
        cursor.capture(&updates);
        assert_eq!(cursor.start(&updates), 2);

        updates.push(append(3, 3));
        assert_eq!(cursor.start(&updates), 2);

        let replaced = vec![append(4, 1), append(4, 2)];
        assert_eq!(cursor.start(&replaced), 0);
    }

    #[test]
    fn diff_updates_append_without_replacing_the_loaded_buffer() {
        let mut viewport = Some(GitDiffViewport {
            generation: 7,
            revision: 1,
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
                revision: 2,
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

        assert_eq!(
            GitDiffState::append_to(&mut viewport, &mut rows, &update),
            Some(1)
        );
        let viewport = viewport.unwrap();
        assert_eq!(viewport.lines.len(), 4);
        assert_eq!(viewport.revision, 2);
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
            revision: 1,
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
                revision: 2,
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

        assert_eq!(
            GitDiffState::append_to(&mut viewport, &mut rows, &update),
            None
        );
        assert_eq!(viewport.unwrap().lines.len(), 1);
        assert_eq!(rows, [GitDiffRow::Line(0)]);
    }

    #[test]
    fn stale_diff_update_does_not_block_a_following_current_update() {
        let mut viewport = Some(GitDiffViewport {
            generation: 8,
            revision: 1,
            first_line: 0,
            total_lines: 3,
            has_more: true,
            lines: vec![line(1)],
            markers: Vec::new(),
            error: String::new(),
        });
        let mut rows = vec![GitDiffRow::Line(0)];
        let updates = [
            GitDiffAppend {
                viewport: GitDiffViewport {
                    generation: 7,
                    revision: 2,
                    first_line: 1,
                    total_lines: 2,
                    has_more: false,
                    lines: vec![line(2)],
                    markers: Vec::new(),
                    error: String::new(),
                },
                rows_from: 0,
                rows: vec![GitDiffRow::Line(1)],
            },
            GitDiffAppend {
                viewport: GitDiffViewport {
                    generation: 8,
                    revision: 2,
                    first_line: 1,
                    total_lines: 3,
                    has_more: false,
                    lines: vec![line(2), line(3)],
                    markers: Vec::new(),
                    error: String::new(),
                },
                rows_from: 0,
                rows: vec![
                    GitDiffRow::Line(0),
                    GitDiffRow::Line(1),
                    GitDiffRow::Line(2),
                ],
            },
        ];

        assert_eq!(
            GitDiffState::append_all_to(&mut viewport, &mut rows, &updates),
            Some(0)
        );
        let viewport = viewport.unwrap();
        assert_eq!(viewport.lines.len(), 3);
        assert_eq!(viewport.revision, 2);
        assert_eq!(
            rows,
            [
                GitDiffRow::Line(0),
                GitDiffRow::Line(1),
                GitDiffRow::Line(2)
            ]
        );
    }

    #[test]
    fn a_full_projection_replaces_rows_for_the_same_loaded_lines() {
        let viewport = GitDiffViewport {
            generation: 9,
            revision: 1,
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

    #[test]
    fn a_full_projection_replaces_same_sized_stale_content() {
        let current = GitDiffViewport {
            generation: 9,
            revision: 1,
            first_line: 0,
            total_lines: 2,
            has_more: false,
            lines: vec![line(1), line(2)],
            markers: Vec::new(),
            error: String::new(),
        };
        let mut incoming = current.clone();
        incoming.revision = 2;
        incoming.has_more = true;
        incoming.lines[1] = line(3);
        let snapshot = GitPageSnapshot {
            diff_viewport: Some(incoming),
            diff_rows: vec![GitDiffRow::Line(0), GitDiffRow::Line(1)],
            ..Default::default()
        };

        assert!(GitDiffState::replaces(
            Some(&current),
            &[GitDiffRow::Line(0), GitDiffRow::Line(1)],
            &snapshot,
        ));
    }

    #[test]
    fn an_older_base_projection_does_not_rewind_appended_diff() {
        let current = GitDiffViewport {
            generation: 9,
            revision: 3,
            first_line: 0,
            total_lines: 3,
            has_more: false,
            lines: vec![line(1), line(2), line(3)],
            markers: Vec::new(),
            error: String::new(),
        };
        let snapshot = GitPageSnapshot {
            diff_viewport: Some(GitDiffViewport {
                generation: 9,
                revision: 1,
                first_line: 0,
                total_lines: 3,
                has_more: true,
                lines: vec![line(1)],
                markers: Vec::new(),
                error: String::new(),
            }),
            diff_rows: vec![GitDiffRow::Line(0)],
            ..Default::default()
        };

        assert!(!GitDiffState::replaces(
            Some(&current),
            &[
                GitDiffRow::Line(0),
                GitDiffRow::Line(1),
                GitDiffRow::Line(2),
            ],
            &snapshot,
        ));
    }
}
