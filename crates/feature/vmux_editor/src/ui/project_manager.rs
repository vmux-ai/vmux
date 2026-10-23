use dioxus::prelude::*;
use vmux_ui::components::filter::{FilterMenu, FilterOption};
use vmux_ui::components::manager::ManagerView;
use vmux_ui::i18n::translate;

use crate::project::{ProjectCollectionViewRequest, ProjectEntry};

#[derive(Clone, PartialEq)]
pub(super) struct ProjectPresentation {
    pub entries: Vec<ProjectEntry>,
    pub selected_filters: Vec<ProjectFilterPresentation>,
    pub filter_requested: bool,
    pub filter_needle: String,
}

impl ProjectPresentation {
    pub(super) fn new(entries: &[ProjectEntry], query: &str, filters: &[ProjectFilter]) -> Self {
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
            .filter(|entry| entry.matches(&search) && ProjectFilter::matches_all(filters, entry))
            .cloned()
            .collect();
        Self {
            entries,
            selected_filters: ProjectFilter::presentations(filters),
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
    pub(super) fn selected(filters: &[Self]) -> Option<Self> {
        filters.first().copied()
    }

    pub(super) fn select(filters: &mut Vec<Self>, filter: Option<Self>) {
        filters.clear();
        if let Some(filter) = filter {
            filters.push(filter);
        }
    }

    pub(super) fn presentations(filters: &[Self]) -> Vec<ProjectFilterPresentation> {
        filters
            .iter()
            .map(|filter| ProjectFilterPresentation {
                label: format!("{} · {}", translate("common-status"), filter.label()),
                dot: filter.dot().to_string(),
            })
            .collect()
    }

    pub(super) fn matches_all(filters: &[Self], entry: &ProjectEntry) -> bool {
        if filters.is_empty() {
            return !entry.archived;
        }
        filters.iter().all(|filter| filter.matches(entry))
    }

    pub(super) fn label(self) -> String {
        translate(match self {
            Self::Ongoing => "session-stage-in-progress",
            Self::Stale => "sessions-stale",
            Self::Archived => "project-archived",
        })
    }

    pub(super) fn dot(self) -> &'static str {
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

    pub(super) fn count(self, entries: &[ProjectEntry]) -> usize {
        entries.iter().filter(|entry| self.matches(entry)).count()
    }
}

pub(super) struct ViewPreference;

impl ViewPreference {
    pub(super) fn manager(view: vmux_ecs::CollectionView) -> ManagerView {
        match view {
            vmux_ecs::CollectionView::List => ManagerView::List,
            vmux_ecs::CollectionView::Grid => ManagerView::Grid,
        }
    }

    pub(super) fn request(view: ManagerView) -> ProjectCollectionViewRequest {
        let view = match view {
            ManagerView::List => vmux_ecs::CollectionView::List,
            ManagerView::Grid => vmux_ecs::CollectionView::Grid,
        };
        ProjectCollectionViewRequest { view }
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
