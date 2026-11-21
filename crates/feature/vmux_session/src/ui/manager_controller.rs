use std::collections::HashSet;

use crate::event::{
    ManagerCategorySelected, ManagerDateFilterAdded, ManagerFilterAdded, ManagerFilterRemoved,
    ManagerModeChanged, ManagerQueryChanged, ManagerSelectionChanged, ManagerSelectionCleared,
    ManagerWithinFilterAdded,
};
use crate::state::ManagerUiState;
use dioxus::prelude::*;
use vmux_session::SessionSummary;
use vmux_ui::components::manager::ManagerView;
use vmux_ui::hooks::send;
use vmux_ui::navigation::{NavigationGesture, PageNavigation};

use crate::SessionId;
use crate::event::{SessionsArchive, SessionsCleanup, SessionsRestoreDeleted, SessionsStageChange};
use crate::route::Route;

use super::manager_model::{
    ArchiveTarget, DeleteTarget, SessionFilter, SessionFilterStep, StageTarget,
};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct SessionFilterPicker {
    filter_step: Signal<SessionFilterStep>,
    filter_value: Signal<String>,
}

impl SessionFilterPicker {
    pub fn use_state() -> Self {
        Self {
            filter_step: use_signal(|| SessionFilterStep::Field),
            filter_value: use_signal(String::new),
        }
    }

    pub fn filter_step(self) -> SessionFilterStep {
        (self.filter_step)()
    }

    pub fn filter_value(self) -> String {
        (self.filter_value)()
    }

    pub fn toggle(self, controls: SessionControls) {
        controls.toggle_filter();
        self.reset_filter_input();
    }

    pub fn select_step(mut self, controls: SessionControls, step: SessionFilterStep) {
        self.filter_step.set(step);
        self.filter_value.set(String::new());
        controls.set_query("#".to_string());
    }

    pub fn set_value(mut self, value: String) {
        self.filter_value.set(value);
    }

    pub fn apply(self, controls: SessionControls, filter: SessionFilter) {
        match filter {
            SessionFilter::Updated(_) => match self.filter_step() {
                SessionFilterStep::Date(comparison) => {
                    controls.date(comparison, self.filter_value())
                }
                SessionFilterStep::Within => {
                    if let Ok(days) = self.filter_value().parse() {
                        controls.within(days);
                    }
                }
                _ => return,
            },
            filter => controls.add_filter(filter),
        }
        controls.dismiss_filter();
        self.reset_filter_input();
    }

    fn reset_filter_input(mut self) {
        self.filter_step.set(SessionFilterStep::Field);
        self.filter_value.set(String::new());
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct SessionControls(pub Signal<ManagerUiState>);

impl SessionControls {
    pub fn query(self) -> String {
        self.0.read().query.clone()
    }
    pub fn view(self) -> ManagerView {
        if self.0.read().grid {
            ManagerView::Grid
        } else {
            ManagerView::List
        }
    }
    pub fn filters(self) -> Vec<SessionFilter> {
        self.0.read().filters.clone()
    }
    pub fn set_query(self, query: String) {
        let _ = send(&ManagerQueryChanged { query });
    }
    pub fn set_view(self, view: ManagerView) {
        let _ = send(&ManagerModeChanged {
            grid: view == ManagerView::Grid,
        });
    }
    pub fn add_filter(self, filter: SessionFilter) {
        let _ = send(&ManagerFilterAdded { filter });
    }
    pub fn remove_filter(self, index: usize) {
        let Some(id) = self.0.read().filter_ids.get(index).copied() else {
            return;
        };
        let _ = send(&ManagerFilterRemoved { id });
    }
    pub fn pop_filter(self) {
        if let Some(index) = self.filters().len().checked_sub(1) {
            self.remove_filter(index);
        }
    }
    pub fn dismiss_filter(self) {
        self.set_query(String::new());
    }
    pub fn toggle_filter(self) {
        self.set_query(if self.query().starts_with('#') {
            String::new()
        } else {
            "#".into()
        });
    }
    pub fn select_stage(self, stage: Option<String>) {
        let _ = send(&ManagerCategorySelected {
            stage,
            archived: false,
        });
    }
    pub fn archived(self) {
        let _ = send(&ManagerCategorySelected {
            stage: None,
            archived: true,
        });
    }
    pub fn date(self, comparison: crate::manager::UpdatedComparison, value: String) {
        let _ = send(&ManagerDateFilterAdded { comparison, value });
    }
    pub fn within(self, days: u64) {
        let _ = send(&ManagerWithinFilterAdded { days });
    }
}

#[derive(Clone, Copy)]
pub(super) struct SessionMutations(Option<Signal<ManagerUiState>>);

impl SessionMutations {
    pub fn new(state: Signal<ManagerUiState>) -> Self {
        Self(Some(state))
    }
    pub fn collection() -> Self {
        Self(None)
    }
    pub fn change(self, target: StageTarget, stage: String) {
        let _ = send(&SessionsStageChange {
            id: target.id,
            kind: target.kind,
            sid: target.sid,
            stage,
        });
    }
    pub fn set_archived(self, targets: Vec<ArchiveTarget>, archived: bool) {
        let _ = send(&SessionsArchive {
            ids: targets.into_iter().map(|target| target.id).collect(),
            archived,
        });
    }
    pub fn archive(self, targets: Vec<ArchiveTarget>) {
        self.set_archived(targets, true);
    }
    pub fn restore(self, targets: Vec<ArchiveTarget>) {
        self.set_archived(targets, false);
    }
    pub fn is_leaving(self, id: &str) -> bool {
        self.0
            .is_some_and(|state| state.read().leaving_ids.contains(id))
    }
    pub fn cleanup(self, target: DeleteTarget) {
        self.cleanup_many(vec![target]);
    }
    pub fn cleanup_many(self, targets: Vec<DeleteTarget>) {
        let _ = send(&SessionsCleanup {
            ids: targets.into_iter().map(|target| target.id).collect(),
        });
    }
    pub fn deleted_provider_count(self) -> usize {
        self.0.map_or(0, |state| state.read().restorable.len())
    }
    pub fn restore_deleted(self) {
        let Some(state) = self.0 else {
            return;
        };
        let _ = send(&SessionsRestoreDeleted {
            sessions: state.read().restorable.clone(),
        });
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct SessionSelection(pub Signal<ManagerUiState>);

impl SessionSelection {
    pub fn ids(self) -> HashSet<String> {
        self.0.read().selected_ids.clone()
    }
    pub fn toggle(self, id: String, selected: bool) {
        let _ = send(&ManagerSelectionChanged { id, selected });
    }
    pub fn clear(self) {
        let _ = send(&ManagerSelectionCleared);
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct ResumeTarget {
    id: String,
    kind: String,
    sid: String,
    managed: bool,
}

impl From<&SessionSummary> for ResumeTarget {
    fn from(session: &SessionSummary) -> Self {
        Self {
            id: session.id.clone(),
            kind: session.kind.clone(),
            sid: session.sid.clone(),
            managed: session.managed,
        }
    }
}

impl ResumeTarget {
    pub fn click(&self, event: &MouseEvent) {
        let gesture = NavigationGesture::from_mouse(event);
        if self.managed {
            PageNavigation::gesture(Route::Session(SessionId(self.id.clone())).url(), gesture);
            return;
        }
        PageNavigation::gesture(
            Route::Provider {
                kind: self.kind.clone(),
                sid: self.sid.clone(),
            }
            .url(),
            gesture,
        );
    }
}

#[derive(Clone, Copy)]
pub(super) struct SessionPager(Signal<usize>, usize);

impl SessionPager {
    const PAGE: usize = 40;

    pub fn use_state(preview: Option<usize>) -> Self {
        let step = preview.unwrap_or(Self::PAGE).max(1);
        Self(use_signal(move || step), step)
    }

    pub fn visible(self, total: usize) -> usize {
        self.0().min(total)
    }

    pub fn load_more(self, event: Event<VisibleData>, total: usize) {
        if !event.is_intersecting().unwrap_or(false) {
            return;
        }
        self.more(total);
    }

    pub fn more(self, total: usize) {
        let current = self.0();
        if current >= total {
            return;
        }
        let mut visible = self.0;
        visible.set(current.saturating_add(self.1).min(total));
    }
}
