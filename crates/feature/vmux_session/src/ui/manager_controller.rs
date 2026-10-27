use std::collections::HashSet;

use dioxus::prelude::*;
use vmux_session::SessionSummary;
use vmux_ui::hooks::{CollectionController, OptimisticMap, send, try_send};
use vmux_ui::navigation::{NavigationGesture, PageNavigation};

use crate::event::{
    ProviderSessionTarget, ResumeSession, SessionsArchive, SessionsCleanup, SessionsRestoreDeleted,
    SessionsStageChange,
};
use crate::route::Route;
use crate::{Id, Session};

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
    deleted_providers: Signal<Vec<ProviderSessionTarget>>,
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
            deleted_providers: use_signal(Vec::new),
        }
    }

    pub fn change(self, target: StageTarget, stage: String) {
        if target.stage == stage {
            return;
        }
        self.stages
            .set(target.id.clone(), target.stage.clone(), stage.clone());
        let id = target.id.clone();
        if try_send(&SessionsStageChange {
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

    pub fn set_archived(mut self, targets: Vec<ArchiveTarget>, archived: bool) {
        let leaving = self.leaving.read();
        let targets = targets
            .into_iter()
            .filter(|target| target.archived != archived && !leaving.contains(&target.id))
            .collect::<Vec<_>>();
        drop(leaving);
        if targets.is_empty() {
            return;
        }
        self.leaving
            .write()
            .extend(targets.iter().map(|target| target.id.clone()));
        self.archives.set_many(
            targets
                .iter()
                .map(|target| (target.id.clone(), target.archived, archived)),
        );
        if try_send(&SessionsArchive {
            ids: targets.iter().map(|target| target.id.clone()).collect(),
            archived,
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

    pub fn archive(self, targets: Vec<ArchiveTarget>) {
        self.set_archived(targets, true);
    }

    pub fn restore(self, targets: Vec<ArchiveTarget>) {
        self.set_archived(targets, false);
    }

    pub fn is_leaving(self, id: &str) -> bool {
        self.leaving.read().contains(id)
    }

    pub fn cleanup(self, target: DeleteTarget) {
        self.cleanup_many(vec![target]);
    }

    pub fn cleanup_many(mut self, targets: Vec<DeleteTarget>) {
        let leaving = self.leaving.read();
        let mut targets = targets
            .into_iter()
            .filter(|target| !leaving.contains(&target.id))
            .collect::<Vec<_>>();
        drop(leaving);
        targets.sort_by(|left, right| left.id.cmp(&right.id));
        targets.dedup_by(|left, right| left.id == right.id);
        if targets.is_empty() {
            return;
        }
        let ids = targets
            .iter()
            .map(|target| target.id.clone())
            .collect::<Vec<_>>();
        let mut providers = targets
            .iter()
            .filter_map(|target| target.provider.clone())
            .collect::<Vec<_>>();
        providers.sort_by(|left, right| (&left.kind, &left.sid).cmp(&(&right.kind, &right.sid)));
        providers.dedup();
        self.leaving.write().extend(ids.iter().cloned());
        if try_send(&SessionsCleanup { ids: ids.clone() }).is_err() {
            let ids = ids.iter().map(String::as_str).collect::<HashSet<_>>();
            self.leaving.write().retain(|id| !ids.contains(id.as_str()));
            return;
        }
        if !providers.is_empty() {
            let mut deleted = self.deleted_providers.write();
            deleted.extend(providers);
            deleted.sort_by(|left, right| (&left.kind, &left.sid).cmp(&(&right.kind, &right.sid)));
            deleted.dedup();
        }
        spawn(async move {
            vmux_ui::platform::Platform::sleep(1_500).await;
            let ids = ids.iter().map(String::as_str).collect::<HashSet<_>>();
            self.leaving.write().retain(|id| !ids.contains(id.as_str()));
        });
    }

    pub fn deleted_provider_count(self) -> usize {
        self.deleted_providers.read().len()
    }

    pub fn restore_deleted(mut self) {
        let sessions = self.deleted_providers.peek().clone();
        if sessions.is_empty() {
            return;
        }
        if try_send(&SessionsRestoreDeleted { sessions }).is_ok() {
            self.deleted_providers.set(Vec::new());
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct ResumeTarget {
    id: String,
    kind: String,
    sid: String,
    cwd: String,
    managed: bool,
}

impl From<&SessionSummary> for ResumeTarget {
    fn from(session: &SessionSummary) -> Self {
        Self {
            id: session.id.clone(),
            kind: session.kind.clone(),
            sid: session.sid.clone(),
            cwd: session.cwd.clone(),
            managed: session.managed,
        }
    }
}

impl ResumeTarget {
    pub fn click(&self, event: &MouseEvent) {
        let gesture = NavigationGesture::from_mouse(event);
        if self.managed {
            PageNavigation::gesture(
                Route::Session(Id::<Session>::new(self.id.clone())).url(),
                gesture,
            );
            return;
        }
        send(&ResumeSession {
            kind: self.kind.clone(),
            sid: self.sid.clone(),
            cwd: self.cwd.clone(),
            open: Some(gesture.open_target()),
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
