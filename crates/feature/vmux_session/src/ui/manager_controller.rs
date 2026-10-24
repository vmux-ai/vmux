use std::collections::HashSet;

use dioxus::prelude::*;
use vmux_session::SessionSummary;
use vmux_ui::hooks::{CollectionController, OptimisticMap, send};

use crate::event::{ResumeSession, SessionsArchive, SessionsCleanup, SessionsStageChange};

use super::manager_model::{ArchiveTarget, SessionFilter, SessionFilterStep, StageTarget};

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

    pub fn dismiss(self, controls: CollectionController<SessionFilter>) {
        controls.dismiss_filter();
        self.reset_filter_input();
    }

    pub fn toggle(self, controls: CollectionController<SessionFilter>) {
        controls.toggle_filter();
        self.reset_filter_input();
    }

    pub fn select_step(
        mut self,
        controls: CollectionController<SessionFilter>,
        step: SessionFilterStep,
    ) {
        self.filter_step.set(step);
        self.filter_value.set(String::new());
        controls.set_query("#".to_string());
    }

    pub fn set_value(mut self, value: String) {
        self.filter_value.set(value);
    }

    pub fn apply(self, controls: CollectionController<SessionFilter>, filter: SessionFilter) {
        controls.update_filters(|filters| SessionFilter::upsert(filters, filter));
        controls.dismiss_filter();
        self.reset_filter_input();
    }

    fn reset_filter_input(mut self) {
        self.filter_step.set(SessionFilterStep::Field);
        self.filter_value.set(String::new());
    }
}

#[derive(Clone, Copy)]
pub(super) struct SessionMutations {
    stages: OptimisticMap<String, String>,
    archives: OptimisticMap<String, bool>,
    leaving: Signal<HashSet<String>>,
}

impl SessionMutations {
    pub fn new(
        stages: OptimisticMap<String, String>,
        archives: OptimisticMap<String, bool>,
    ) -> Self {
        Self {
            stages,
            archives,
            leaving: use_signal(HashSet::new),
        }
    }

    pub fn change(self, target: StageTarget, stage: String) {
        if target.stage == stage {
            return;
        }
        self.stages
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
            self.stages.clear(&id);
        }
    }

    pub fn archive(mut self, targets: Vec<ArchiveTarget>) {
        let leaving = self.leaving.read();
        let targets = targets
            .into_iter()
            .filter(|target| !target.archived && !leaving.contains(&target.id))
            .collect::<Vec<_>>();
        drop(leaving);
        if targets.is_empty() {
            return;
        }
        self.leaving
            .write()
            .extend(targets.iter().map(|target| target.id.clone()));
        for target in &targets {
            self.archives.set(target.id.clone(), target.archived, true);
        }
        if send(&SessionsArchive {
            ids: targets.iter().map(|target| target.id.clone()).collect(),
            archived: true,
        })
        .is_err()
        {
            for target in &targets {
                self.archives.clear(&target.id);
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
