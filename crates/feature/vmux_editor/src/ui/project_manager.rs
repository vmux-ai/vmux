use dioxus::prelude::*;
use vmux_ui::components::filter::{FilterMenu, FilterOption};
use vmux_ui::components::manager::ManagerView;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

use crate::project::{ProjectCollectionViewRequest, ProjectEntry};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct ProjectControls {
    query: Signal<String>,
    view: Signal<ManagerView>,
    filters: Signal<ProjectFilters>,
}

impl ProjectControls {
    pub(super) fn use_state(view: vmux_ecs::CollectionView) -> Self {
        let view = match view {
            vmux_ecs::CollectionView::List => ManagerView::List,
            vmux_ecs::CollectionView::Grid => ManagerView::Grid,
        };
        Self {
            query: use_signal(String::new),
            view: use_signal(|| view),
            filters: use_signal(ProjectFilters::default),
        }
    }

    pub(super) fn query(self) -> String {
        (self.query)()
    }

    pub(super) fn view(self) -> ManagerView {
        (self.view)()
    }

    pub(super) fn set_query(mut self, query: String) {
        self.query.set(query);
    }

    pub(super) fn set_view(mut self, view: ManagerView) {
        self.view.set(view);
        let view = match view {
            ManagerView::List => vmux_ecs::CollectionView::List,
            ManagerView::Grid => vmux_ecs::CollectionView::Grid,
        };
        let _ = send(&ProjectCollectionViewRequest { view });
    }

    pub(super) fn toggle_filter(mut self) {
        if (self.query)().starts_with('#') {
            self.query.set(String::new());
        } else {
            self.query.set("#".to_string());
        }
    }

    pub(super) fn dismiss_filter(mut self) {
        self.query.set(String::new());
    }

    pub(super) fn apply_filter(mut self, filter: ProjectFilter) {
        self.filters.write().set(filter);
        self.dismiss_filter();
    }

    pub(super) fn remove_filter(mut self, index: usize) {
        self.filters.write().remove(index);
    }

    pub(super) fn pop_filter(mut self) {
        self.filters.write().pop();
    }

    pub(super) fn filters(self) -> ProjectFilters {
        (self.filters)()
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct ProjectPresentation {
    pub entries: Vec<ProjectEntry>,
    pub selected_filters: Vec<ProjectFilterPresentation>,
    pub filter_requested: bool,
    pub filter_needle: String,
}

impl ProjectPresentation {
    pub(super) fn new(entries: &[ProjectEntry], query: &str, filters: &ProjectFilters) -> Self {
        let filter_requested = query.starts_with('#');
        let filter_needle = query
            .strip_prefix('#')
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let search = if filter_requested {
            String::new()
        } else {
            query.trim().to_ascii_lowercase()
        };
        let entries = entries
            .iter()
            .filter(|entry| entry.matches(&search) && filters.matches(entry))
            .cloned()
            .collect();
        Self {
            entries,
            selected_filters: filters.presentations(),
            filter_requested,
            filter_needle,
        }
    }
}

impl ProjectEntry {
    fn matches(&self, needle: &str) -> bool {
        needle.is_empty()
            || self.name.to_ascii_lowercase().contains(needle)
            || self.source.to_ascii_lowercase().contains(needle)
            || self.path.to_ascii_lowercase().contains(needle)
            || self.branch.to_ascii_lowercase().contains(needle)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProjectFilter {
    Ongoing,
    Stale,
    Archived,
}

impl ProjectFilter {
    fn label(self) -> String {
        translate(match self {
            Self::Ongoing => "session-stage-in-progress",
            Self::Stale => "sessions-stale",
            Self::Archived => "project-archived",
        })
    }

    fn dot(self) -> &'static str {
        match self {
            Self::Ongoing => "bg-success",
            Self::Stale => "bg-muted-foreground",
            Self::Archived => "bg-muted-foreground",
        }
    }

    fn matches(self, entry: &ProjectEntry) -> bool {
        match self {
            Self::Ongoing => !entry.archived && entry.ongoing_sessions > 0,
            Self::Stale => !entry.archived && entry.stale_sessions > 0,
            Self::Archived => entry.archived,
        }
    }

    fn count(self, entries: &[ProjectEntry]) -> usize {
        entries.iter().filter(|entry| self.matches(entry)).count()
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct ProjectFilters(Vec<ProjectFilter>);

impl ProjectFilters {
    fn matches(&self, entry: &ProjectEntry) -> bool {
        if self.0.is_empty() {
            return !entry.archived;
        }
        self.0.iter().all(|filter| filter.matches(entry))
    }

    fn presentations(&self) -> Vec<ProjectFilterPresentation> {
        self.0
            .iter()
            .map(|filter| ProjectFilterPresentation {
                label: format!("{} · {}", translate("common-status"), filter.label()),
                dot: filter.dot().to_string(),
            })
            .collect()
    }

    fn set(&mut self, filter: ProjectFilter) {
        self.0.clear();
        self.0.push(filter);
    }

    fn remove(&mut self, index: usize) {
        if index < self.0.len() {
            self.0.remove(index);
        }
    }

    fn pop(&mut self) {
        self.0.pop();
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct ProjectFilterPresentation {
    pub label: String,
    pub dot: String,
}

#[component]
pub(super) fn ProjectFilterMenu(
    entries: Vec<ProjectEntry>,
    needle: String,
    onselect: EventHandler<ProjectFilter>,
) -> Element {
    let options = [
        ProjectFilter::Ongoing,
        ProjectFilter::Stale,
        ProjectFilter::Archived,
    ];
    rsx! {
        FilterMenu { title: translate("common-status"),
            for filter in options {
                if needle.is_empty() || filter.label().to_ascii_lowercase().contains(&needle) {
                    FilterOption {
                        key: "project-filter-{filter:?}",
                        label: filter.label(),
                        dot: filter.dot().to_string(),
                        trailing: filter.count(&entries).to_string(),
                        onclick: move |_| onselect.call(filter),
                    }
                }
            }
        }
    }
}
