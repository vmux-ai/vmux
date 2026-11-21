use crate::event::{ProviderSessionTarget, SessionRepositories};
use chrono::{Duration, Local, NaiveDate, TimeZone};
use std::collections::HashSet;
use vmux_session::{CatalogSnapshot, SessionSummary, StageSummary};
use vmux_ui::i18n::{TranslationValue, translate, translate_humanized, translate_with};

#[vmux_api::ui_state(Eq)]
pub struct ManagerUiState {
    pub loading: bool,
    pub snapshot: CatalogSnapshot,
    pub query: String,
    pub filters: Vec<SessionFilter>,
    pub filter_ids: Vec<u64>,
    pub selected_ids: HashSet<String>,
    pub grid: bool,
    pub leaving_ids: HashSet<String>,
    pub restorable: Vec<ProviderSessionTarget>,
    pub repositories: SessionRepositories,
    pub agent_options: Vec<(String, usize)>,
    pub project_options: Vec<(String, usize)>,
    pub category_groups: Vec<StageGroup>,
    pub groups: Vec<StageGroup>,
    pub selected_stage: Option<String>,
    pub selected_archived: bool,
    pub archived_count: usize,
    pub filter_requested: bool,
    pub filter_needle: String,
    pub visible_sessions: bool,
    pub visible_ids: HashSet<String>,
}

impl Default for ManagerUiState {
    fn default() -> Self {
        Self {
            loading: true,
            snapshot: Default::default(),
            query: Default::default(),
            filters: Default::default(),
            filter_ids: Default::default(),
            selected_ids: Default::default(),
            grid: Default::default(),
            leaving_ids: Default::default(),
            restorable: Default::default(),
            repositories: Default::default(),
            agent_options: Default::default(),
            project_options: Default::default(),
            category_groups: Default::default(),
            groups: Default::default(),
            selected_stage: Default::default(),
            selected_archived: Default::default(),
            archived_count: Default::default(),
            filter_requested: Default::default(),
            filter_needle: Default::default(),
            visible_sessions: Default::default(),
            visible_ids: Default::default(),
        }
    }
}

pub use ManagerUiState as ManagerPresentation;

#[vmux_api::contract(Eq)]
pub enum SessionFilter {
    Current,
    Stage(String),
    Archived,
    Agent(String),
    Project(String),
    Updated(UpdatedFilter),
}

impl SessionFilter {
    pub fn presentations(
        filters: &[Self],
        stages: &[StageSummary],
    ) -> Vec<SessionFilterPresentation> {
        filters
            .iter()
            .map(|filter| filter.presentation(stages))
            .collect()
    }

    pub(crate) fn presentation(&self, stages: &[StageSummary]) -> SessionFilterPresentation {
        match self {
            Self::Current => SessionFilterPresentation {
                label: translate("session-stage-active"),
                dot: "bg-success".to_string(),
            },
            Self::Stage(id) => {
                let name = stages
                    .iter()
                    .find(|stage| stage.id == *id)
                    .map(|stage| translate_humanized(&stage.name))
                    .unwrap_or_else(|| {
                        if id == "stale" {
                            translate("sessions-stale")
                        } else {
                            translate_humanized(id)
                        }
                    });
                SessionFilterPresentation {
                    label: format!("{} · {name}", translate("common-status")),
                    dot: crate::manager::StageGroup::dot(id).to_string(),
                }
            }
            Self::Archived => SessionFilterPresentation {
                label: format!(
                    "{} · {}",
                    translate("common-status"),
                    translate("common-archived")
                ),
                dot: String::new(),
            },
            Self::Agent(agent) => SessionFilterPresentation {
                label: format!("{} · {agent}", translate("resume-column-agent")),
                dot: String::new(),
            },
            Self::Project(project) => SessionFilterPresentation {
                label: format!(
                    "{} · {}",
                    translate("resume-column-project"),
                    std::path::Path::new(project)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(project)
                ),
                dot: String::new(),
            },
            Self::Updated(updated) => SessionFilterPresentation {
                label: format!(
                    "{} · {} · {}",
                    translate("common-updated"),
                    updated.comparison.label(),
                    if updated.comparison == UpdatedComparison::Within {
                        translate_with(
                            "common-days-value",
                            &[(
                                "count",
                                TranslationValue::Number(
                                    updated.value_label.parse().unwrap_or_default(),
                                ),
                            )],
                        )
                    } else {
                        updated.value_label.clone()
                    }
                ),
                dot: String::new(),
            },
        }
    }
}

#[vmux_api::contract(Eq)]
pub struct SessionFilterPresentation {
    pub label: String,
    pub dot: String,
}

#[vmux_api::contract(Copy, Eq)]
pub enum UpdatedComparison {
    Before,
    OnOrBefore,
    After,
    OnOrAfter,
    Within,
}

impl UpdatedComparison {
    pub fn label(self) -> String {
        translate(match self {
            Self::Before => "common-before",
            Self::OnOrBefore => "common-on-or-before",
            Self::After => "common-after",
            Self::OnOrAfter => "common-on-or-after",
            Self::Within => "common-within",
        })
    }

    pub fn detail(self) -> String {
        match self {
            Self::Before => "<",
            Self::OnOrBefore => "≤",
            Self::After => ">",
            Self::OnOrAfter => "≥",
            Self::Within => "",
        }
        .to_string()
    }
}

#[vmux_api::contract(Eq)]
#[derive(bevy_ecs::prelude::Component)]
pub struct UpdatedFilter {
    pub comparison: UpdatedComparison,
    pub threshold: i64,
    pub value_label: String,
}

impl UpdatedFilter {
    pub fn from_date(comparison: UpdatedComparison, value: &str) -> Option<Self> {
        let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
        let start = Self::day_start(date)?;
        let next = Self::day_start(date.succ_opt()?)?;
        let threshold = match comparison {
            UpdatedComparison::Before | UpdatedComparison::OnOrAfter => start,
            UpdatedComparison::OnOrBefore | UpdatedComparison::After => next,
            UpdatedComparison::Within => return None,
        };
        Some(Self {
            comparison,
            threshold,
            value_label: value.to_string(),
        })
    }

    pub fn within(days: u64) -> Option<Self> {
        if days == 0 {
            return None;
        }
        let duration = Duration::try_days(days.try_into().ok()?)?;
        Some(Self {
            comparison: UpdatedComparison::Within,
            threshold: vmux_ecs::UnixMillis::now().0 - duration.num_milliseconds(),
            value_label: days.to_string(),
        })
    }

    fn day_start(date: NaiveDate) -> Option<i64> {
        let local = Local.from_local_datetime(&date.and_hms_opt(0, 0, 0)?);
        local.earliest().map(|date| date.timestamp_millis())
    }

    pub(crate) fn matches(&self, timestamp: i64) -> bool {
        match self.comparison {
            UpdatedComparison::Before | UpdatedComparison::OnOrBefore => timestamp < self.threshold,
            UpdatedComparison::After | UpdatedComparison::OnOrAfter | UpdatedComparison::Within => {
                timestamp >= self.threshold
            }
        }
    }
}

#[vmux_api::contract(Eq)]
pub struct StageGroup {
    pub stage: StageSummary,
    pub sessions: Vec<SessionSummary>,
}

impl StageGroup {
    pub fn dot(id: &str) -> &'static str {
        match id {
            "in_progress" => "bg-amber-400",
            "stale" => "bg-muted-foreground/55",
            "in_review" => "bg-violet-400",
            "done" => "bg-success",
            "canceled" => "bg-muted-foreground/45",
            _ => "bg-primary/75",
        }
    }
}
