use std::collections::HashSet;

use dioxus::prelude::*;
use vmux_session::SessionSummary;
use vmux_ui::components::manager::ManagerView;
use vmux_ui::hooks::{OptimisticMap, send};

use crate::event::{ResumeSession, SessionsCleanup, SessionsStageChange};

use super::manager_model::{SessionFilter, SessionFilterStep, SessionFilters, StageTarget};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct ManagerControls {
    query: Signal<String>,
    view: Signal<ManagerView>,
    filters: Signal<SessionFilters>,
    filter_step: Signal<SessionFilterStep>,
    filter_value: Signal<String>,
}

impl ManagerControls {
    pub fn use_state() -> Self {
        Self {
            query: use_signal(String::new),
            view: use_signal(ManagerView::default),
            filters: use_signal(SessionFilters::default),
            filter_step: use_signal(|| SessionFilterStep::Field),
            filter_value: use_signal(String::new),
        }
    }

    pub fn query(self) -> String {
        (self.query)()
    }

    pub fn view(self) -> ManagerView {
        (self.view)()
    }

    pub fn filters(self) -> SessionFilters {
        (self.filters)()
    }

    pub fn filter_step(self) -> SessionFilterStep {
        (self.filter_step)()
    }

    pub fn filter_value(self) -> String {
        (self.filter_value)()
    }

    pub fn set_query(mut self, query: String) {
        self.query.set(query);
    }

    pub fn set_view(mut self, view: ManagerView) {
        self.view.set(view);
    }

    pub fn remove_filter(mut self, index: usize) {
        self.filters.write().remove(index);
    }

    pub fn pop_filter(mut self) {
        self.filters.write().pop();
    }

    pub fn dismiss_filter(mut self) {
        self.query.set(String::new());
        self.reset_filter_input();
    }

    pub fn toggle_filter(mut self) {
        if (self.query)().starts_with('#') {
            self.query.set(String::new());
        } else {
            self.query.set("#".to_string());
        }
        self.reset_filter_input();
    }

    pub fn select_filter_step(mut self, step: SessionFilterStep) {
        self.filter_step.set(step);
        self.filter_value.set(String::new());
        self.query.set("#".to_string());
    }

    pub fn set_filter_value(mut self, value: String) {
        self.filter_value.set(value);
    }

    pub fn apply_filter(mut self, filter: SessionFilter) {
        self.filters.write().upsert(filter);
        self.dismiss_filter();
    }

    pub fn select_stage(mut self, stage: Option<String>) {
        self.filters.write().set_stage(stage);
        self.query.set(String::new());
    }

    fn reset_filter_input(mut self) {
        self.filter_step.set(SessionFilterStep::Field);
        self.filter_value.set(String::new());
    }
}

#[derive(Clone, Copy)]
pub(super) struct SessionStageMutations {
    optimistic: OptimisticMap<String, String>,
    leaving: Signal<HashSet<String>>,
    archive_stage: Memo<String>,
}

impl SessionStageMutations {
    pub fn new(optimistic: OptimisticMap<String, String>, archive_stage: Memo<String>) -> Self {
        Self {
            optimistic,
            leaving: use_signal(HashSet::new),
            archive_stage,
        }
    }

    pub fn archive_stage(self) -> String {
        (self.archive_stage)()
    }

    pub fn change(self, target: StageTarget, stage: String) {
        if target.stage == stage {
            return;
        }
        self.optimistic
            .set(target.id.clone(), target.stage.clone(), stage.clone());
        let id = target.id.clone();
        if send(&SessionsStageChange {
            id: target.id,
            kind: target.kind,
            sid: target.sid,
            stage,
        })
        .is_err()
        {
            self.optimistic.clear(&id);
        }
    }

    pub fn archive(mut self, targets: Vec<StageTarget>) {
        let archive_stage = self.archive_stage();
        if archive_stage.is_empty() {
            return;
        }
        let leaving = self.leaving.read();
        let targets = targets
            .into_iter()
            .filter(|target| target.stage != archive_stage && !leaving.contains(&target.id))
            .collect::<Vec<_>>();
        drop(leaving);
        if targets.is_empty() {
            return;
        }
        self.leaving
            .write()
            .extend(targets.iter().map(|target| target.id.clone()));
        for target in &targets {
            self.optimistic.set(
                target.id.clone(),
                target.stage.clone(),
                archive_stage.clone(),
            );
            if send(&SessionsStageChange {
                id: target.id.clone(),
                kind: target.kind.clone(),
                sid: target.sid.clone(),
                stage: archive_stage.clone(),
            })
            .is_err()
            {
                self.optimistic.clear(&target.id);
                self.leaving.write().remove(&target.id);
            }
        }
        spawn(async move {
            vmux_ui::platform::Platform::sleep(200).await;
            let ids = targets
                .iter()
                .map(|target| target.id.as_str())
                .collect::<HashSet<_>>();
            self.leaving.write().retain(|id| !ids.contains(id.as_str()));
        });
    }

    pub fn is_leaving(self, id: &str) -> bool {
        self.leaving.read().contains(id)
    }

    pub fn cleanup(mut self, id: String) {
        if self.leaving.read().contains(&id) {
            return;
        }
        self.leaving.write().insert(id.clone());
        if send(&SessionsCleanup { id: id.clone() }).is_err() {
            self.leaving.write().remove(&id);
            return;
        }
        spawn(async move {
            vmux_ui::platform::Platform::sleep(1_500).await;
            self.leaving.write().remove(&id);
        });
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct ResumeTarget {
    kind: String,
    sid: String,
    cwd: String,
}

impl From<&SessionSummary> for ResumeTarget {
    fn from(session: &SessionSummary) -> Self {
        Self {
            kind: session.kind.clone(),
            sid: session.sid.clone(),
            cwd: session.cwd.clone(),
        }
    }
}

impl ResumeTarget {
    pub fn open(&self) {
        let _ = send(&ResumeSession {
            kind: self.kind.clone(),
            sid: self.sid.clone(),
            cwd: self.cwd.clone(),
        });
    }
}

#[derive(Clone, Copy)]
pub(super) struct SessionPager(Signal<usize>);

impl SessionPager {
    const PAGE: usize = 40;

    pub fn use_state() -> Self {
        Self(use_signal(|| Self::PAGE))
    }

    pub fn visible(self, total: usize) -> usize {
        self.0().min(total)
    }

    pub fn load_more(self, event: Event<VisibleData>, total: usize) {
        if !event.is_intersecting().unwrap_or(false) {
            return;
        }
        let current = self.0();
        if current >= total {
            return;
        }
        let mut visible = self.0;
        visible.set(current.saturating_add(Self::PAGE).min(total));
    }
}
