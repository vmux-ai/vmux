use std::collections::HashMap;

use chrono::{Duration, Local, NaiveDate, TimeZone};
use vmux_session::{
    CatalogSnapshot, SessionAge, SessionSummary, SessionSummaryProjection, StageSummary,
};
use vmux_ui::i18n::{TranslationValue, translate, translate_humanized, translate_with};

use crate::event::{SessionRepositories, SessionRepository};

#[derive(Clone, PartialEq)]
pub(super) struct ManagerPresentation {
    pub snapshot: CatalogSnapshot,
    pub repositories: SessionRepositories,
    pub category_groups: Vec<StageGroup>,
    pub groups: Vec<StageGroup>,
    pub selected_filters: Vec<SessionFilterPresentation>,
    pub selected_stage: Option<String>,
    pub filter_requested: bool,
    pub filter_needle: String,
    pub visible_sessions: bool,
}

impl ManagerPresentation {
    pub fn stage_observations(snapshot: &CatalogSnapshot) -> HashMap<String, String> {
        snapshot
            .sessions
            .iter()
            .map(|session| (session.id.clone(), session.stage.clone()))
            .collect()
    }

    pub fn archive_observations(snapshot: &CatalogSnapshot) -> HashMap<String, bool> {
        snapshot
            .sessions
            .iter()
            .map(|session| (session.id.clone(), session.archived_at > 0))
            .collect()
    }

    pub fn new(
        mut snapshot: CatalogSnapshot,
        repositories: SessionRepositories,
        query: &str,
        filters: &[SessionFilter],
        mut stage_for: impl FnMut(&String, &String) -> String,
        mut archived_for: impl FnMut(&String, bool) -> bool,
    ) -> Self {
        for session in &mut snapshot.sessions {
            session.stage = stage_for(&session.id, &session.stage);
            session.archived_at = if archived_for(&session.id, session.archived_at > 0) {
                session.archived_at.max(1)
            } else {
                0
            };
        }
        snapshot.sessions.retain(|session| session.archived_at == 0);
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
        let category_groups =
            StageGroup::from_catalog(&snapshot.stages, &snapshot.sessions, &search);
        let filtered_groups = StageGroup::filtered(&category_groups, filters, &repositories);
        let groups = if SessionFilter::stage(filters).is_none() {
            StageGroup::inbox(&filtered_groups)
        } else {
            filtered_groups
        };
        let visible_sessions = groups.iter().any(|group| !group.sessions.is_empty());
        let selected_filters = SessionFilter::presentations(filters, &snapshot.stages);
        let selected_stage = SessionFilter::stage(filters).map(str::to_string);
        Self {
            snapshot,
            repositories,
            category_groups,
            groups,
            selected_filters,
            selected_stage,
            filter_requested,
            filter_needle,
            visible_sessions,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) enum SessionFilter {
    Stage(String),
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

    pub fn stage(filters: &[Self]) -> Option<&str> {
        filters.iter().find_map(|filter| match filter {
            Self::Stage(id) => Some(id.as_str()),
            Self::Agent(_) | Self::Project(_) | Self::Updated(_) => None,
        })
    }

    pub fn set_stage(filters: &mut Vec<Self>, stage: Option<String>) {
        filters.retain(|filter| !matches!(filter, Self::Stage(_)));
        if let Some(stage) = stage {
            filters.insert(0, Self::Stage(stage));
        }
    }

    pub fn upsert(filters: &mut Vec<Self>, selected: Self) {
        match selected {
            Self::Stage(stage) => Self::set_stage(filters, Some(stage)),
            Self::Agent(agent) => {
                filters.retain(|filter| !matches!(filter, Self::Agent(_)));
                filters.push(Self::Agent(agent));
            }
            Self::Project(project) => {
                filters.retain(|filter| !matches!(filter, Self::Project(_)));
                filters.push(Self::Project(project));
            }
            Self::Updated(updated) => {
                filters.retain(|filter| !matches!(filter, Self::Updated(_)));
                filters.push(Self::Updated(updated));
            }
        }
    }

    fn presentation(&self, stages: &[StageSummary]) -> SessionFilterPresentation {
        match self {
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
                    dot: StageVisual::for_id(id).dot.to_string(),
                }
            }
            Self::Agent(agent) => SessionFilterPresentation {
                label: format!("{} · {agent}", translate("resume-column-agent")),
                dot: String::new(),
            },
            Self::Project(project) => SessionFilterPresentation {
                label: format!("{} · {project}", translate("resume-column-project")),
                dot: String::new(),
            },
            Self::Updated(updated) => SessionFilterPresentation {
                label: format!(
                    "{} · {} · {}",
                    translate("common-updated"),
                    updated.comparison.label(),
                    updated.value_label
                ),
                dot: String::new(),
            },
        }
    }

    fn matches(&self, session: &SessionSummary, repositories: &SessionRepositories) -> bool {
        match self {
            Self::Stage(_) => true,
            Self::Agent(agent) => SessionFilterValue::agent(session) == *agent,
            Self::Project(project) => repositories
                .get(&session.id)
                .is_some_and(|repository| repository.project == *project),
            Self::Updated(updated) => updated.matches(session.last_activated_at),
        }
    }
}

struct SessionFilterValue;

impl SessionFilterValue {
    fn agent(session: &SessionSummary) -> String {
        if session.agent_name.is_empty() {
            session.agent.clone()
        } else {
            session.agent_name.clone()
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct SessionFilterPresentation {
    pub label: String,
    pub dot: String,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum SessionFilterStep {
    Field,
    Status,
    Agent,
    Project,
    Updated,
    Date(UpdatedComparison),
    Within,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum UpdatedComparison {
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

#[derive(Clone, PartialEq)]
pub(super) struct UpdatedFilter {
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
            value_label: translate_with(
                "common-days-value",
                &[("count", TranslationValue::Number(days as i64))],
            ),
        })
    }

    fn day_start(date: NaiveDate) -> Option<i64> {
        let local = Local.from_local_datetime(&date.and_hms_opt(0, 0, 0)?);
        local.earliest().map(|date| date.timestamp_millis())
    }

    fn matches(&self, timestamp: i64) -> bool {
        match self.comparison {
            UpdatedComparison::Before | UpdatedComparison::OnOrBefore => timestamp < self.threshold,
            UpdatedComparison::After | UpdatedComparison::OnOrAfter | UpdatedComparison::Within => {
                timestamp >= self.threshold
            }
        }
    }
}

pub(super) struct SessionFilterMenuPresentation;

impl SessionFilterMenuPresentation {
    pub fn agents(sessions: &[SessionSummary]) -> Vec<(String, usize)> {
        let mut agents = HashMap::<String, usize>::new();
        for session in sessions {
            let agent = SessionFilterValue::agent(session);
            if !agent.is_empty() {
                *agents.entry(agent).or_default() += 1;
            }
        }
        let mut agents = agents.into_iter().collect::<Vec<_>>();
        agents.sort_by_key(|left| left.0.to_lowercase());
        agents
    }

    pub fn projects(
        sessions: &[SessionSummary],
        repositories: &SessionRepositories,
    ) -> Vec<(String, usize)> {
        let mut projects = HashMap::<String, usize>::new();
        for session in sessions {
            if let Some(repository) = repositories.get(&session.id)
                && !repository.project.is_empty()
            {
                *projects.entry(repository.project).or_default() += 1;
            }
        }
        let mut projects = projects.into_iter().collect::<Vec<_>>();
        projects.sort_by_key(|left| left.0.to_lowercase());
        projects
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct StageGroup {
    pub stage: StageSummary,
    pub sessions: Vec<SessionSummary>,
}

impl StageGroup {
    pub fn inbox(groups: &[Self]) -> Vec<Self> {
        groups
            .iter()
            .filter(|group| !group.stage.terminal)
            .cloned()
            .collect()
    }

    pub fn filtered(
        groups: &[Self],
        filters: &[SessionFilter],
        repositories: &SessionRepositories,
    ) -> Vec<Self> {
        let selected_stage = filters.iter().find_map(|filter| match filter {
            SessionFilter::Stage(id) => Some(id.as_str()),
            SessionFilter::Agent(_) | SessionFilter::Project(_) | SessionFilter::Updated(_) => None,
        });
        let mut filtered = Vec::new();
        for group in groups {
            if selected_stage.is_some_and(|stage| stage != group.stage.id.as_str()) {
                continue;
            }
            let mut group = group.clone();
            group.sessions.retain(|session| {
                filters
                    .iter()
                    .all(|filter| filter.matches(session, repositories))
            });
            filtered.push(group);
        }
        filtered
    }

    pub fn from_catalog(
        stages: &[StageSummary],
        sessions: &[SessionSummary],
        search: &str,
    ) -> Vec<Self> {
        let now = vmux_ecs::UnixMillis::now().0;
        let mut groups = stages
            .iter()
            .cloned()
            .map(|stage| Self {
                stage,
                sessions: Vec::new(),
            })
            .collect::<Vec<_>>();
        for session in sessions {
            if !SessionSummaryProjection::matches(session, search) {
                continue;
            }
            if SessionSummaryProjection::is_stale(session, stages, now) {
                let stale_index = match groups.iter().position(|group| group.stage.id == "stale") {
                    Some(index) => index,
                    None => {
                        let index = groups
                            .iter()
                            .position(|group| !group.stage.terminal)
                            .map(|index| index + 1)
                            .unwrap_or(groups.len());
                        groups.insert(
                            index,
                            Self {
                                stage: StageSummary {
                                    id: "stale".into(),
                                    name: "sessions-stale".into(),
                                    order: 0,
                                    terminal: false,
                                },
                                sessions: Vec::new(),
                            },
                        );
                        index
                    }
                };
                groups[stale_index].sessions.push(session.clone());
                continue;
            }
            if let Some(group) = groups
                .iter_mut()
                .find(|group| group.stage.id == session.stage)
            {
                group.sessions.push(session.clone());
                continue;
            }
            groups.push(Self {
                stage: StageSummary {
                    id: session.stage.clone(),
                    name: session.stage.clone(),
                    order: u32::MAX,
                    terminal: false,
                },
                sessions: vec![session.clone()],
            });
        }
        groups
    }
}

pub(super) struct StageGroupPresentation {
    pub dot: &'static str,
    pub archivable: Vec<ArchiveTarget>,
}

impl StageGroupPresentation {
    pub fn new(stage: &StageSummary, sessions: &[SessionSummary]) -> Self {
        Self {
            dot: StageVisual::for_id(&stage.id).dot,
            archivable: sessions
                .iter()
                .filter(|session| session.archived_at == 0)
                .map(ArchiveTarget::from_session)
                .collect(),
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct ArchiveTarget {
    pub id: String,
    pub archived: bool,
}

impl ArchiveTarget {
    pub fn from_session(session: &SessionSummary) -> Self {
        Self {
            id: session.id.clone(),
            archived: session.archived_at > 0,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct StageTarget {
    pub id: String,
    pub kind: String,
    pub sid: String,
    pub stage: String,
}

impl StageTarget {
    pub fn from_session(session: &SessionSummary) -> Self {
        Self {
            id: session.id.clone(),
            kind: session.kind.clone(),
            sid: session.sid.clone(),
            stage: session.stage.clone(),
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct SessionContext {
    pub project: String,
    pub branch: String,
    pub git_url: String,
}

impl SessionContext {
    fn from(repository: Option<SessionRepository>) -> Self {
        match repository {
            Some(repository) => Self {
                project: repository.project,
                branch: repository.branch,
                git_url: repository.url,
            },
            None => Self {
                project: String::new(),
                branch: String::new(),
                git_url: String::new(),
            },
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct AgentPresentation {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub color: String,
}

impl AgentPresentation {
    fn from_session(session: &SessionSummary) -> Self {
        Self {
            id: session.agent.clone(),
            name: if session.agent_name.is_empty() {
                session.agent.clone()
            } else {
                session.agent_name.clone()
            },
            icon: (!session.agent_icon.is_empty()).then(|| session.agent_icon.clone()),
            color: session.agent_color.clone(),
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct SessionPresentation {
    pub context: SessionContext,
    pub stage_target: StageTarget,
    pub archived: bool,
    pub runtime: RuntimeVisual,
    pub agent: AgentPresentation,
    pub stage_dot: &'static str,
    pub title: String,
    pub preview: String,
    pub compact_id: String,
    pub updated: String,
}

impl SessionPresentation {
    pub fn new(
        session: &SessionSummary,
        repository: Option<SessionRepository>,
        stages: &[StageSummary],
        stage: &str,
    ) -> Self {
        let summary = SessionSummaryProjection::new(session, stages, vmux_ecs::UnixMillis::now().0);
        Self {
            context: SessionContext::from(repository),
            stage_target: StageTarget::from_session(session),
            archived: session.archived_at > 0,
            runtime: RuntimeVisual::for_id(&session.runtime),
            agent: AgentPresentation::from_session(session),
            stage_dot: StageVisual::for_id(stage).dot,
            title: summary.title,
            preview: summary.preview,
            compact_id: SessionIdentity::new(&session.sid).compact(),
            updated: RelativeTime(summary.age).label(),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct StageVisual {
    pub dot: &'static str,
}

impl StageVisual {
    pub fn for_id(id: &str) -> Self {
        let dot = match id {
            "in_progress" => "bg-amber-400",
            "stale" => "bg-muted-foreground/55",
            "in_review" => "bg-violet-400",
            "done" => "bg-success",
            "canceled" => "bg-muted-foreground/45",
            _ => "bg-primary/75",
        };
        Self { dot }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct RuntimeVisual {
    pub dot: &'static str,
    pub visible: bool,
}

impl RuntimeVisual {
    pub fn for_id(id: &str) -> Self {
        let dot = match id {
            "streaming" => "bg-success",
            "awaiting" => "bg-amber-400",
            "installing" => "bg-sky-400",
            "errored" => "bg-ansi-1",
            "idle" => "bg-primary/65",
            _ => "bg-muted-foreground/35",
        };
        let visible = matches!(id, "streaming" | "awaiting" | "installing" | "errored");
        Self { dot, visible }
    }
}

pub(super) struct RelativeTime(pub SessionAge);

impl RelativeTime {
    pub fn label(self) -> String {
        let (id, count) = if self.0.0 < SessionAge::MINUTE {
            ("resume-age-now", 0)
        } else if self.0.0 < SessionAge::HOUR {
            ("resume-age-minutes", self.0.0 / SessionAge::MINUTE)
        } else if self.0.0 < SessionAge::DAY {
            ("resume-age-hours", self.0.0 / SessionAge::HOUR)
        } else if self.0.0 < SessionAge::WEEK {
            ("resume-age-days", self.0.0 / SessionAge::DAY)
        } else if self.0.0 < SessionAge::MONTH {
            ("resume-age-weeks", self.0.0 / SessionAge::WEEK)
        } else if self.0.0 < SessionAge::YEAR {
            ("resume-age-months", self.0.0 / SessionAge::MONTH)
        } else {
            ("resume-age-years", self.0.0 / SessionAge::YEAR)
        };
        if count == 0 {
            return translate(id);
        }
        translate_with(id, &[("count", TranslationValue::Number(count as i64))])
    }
}

pub(super) struct SessionIdentity<'a>(&'a str);

impl<'a> SessionIdentity<'a> {
    pub fn new(id: &'a str) -> Self {
        Self(id)
    }

    pub fn compact(&self) -> String {
        let characters = self.0.chars().collect::<Vec<_>>();
        if characters.len() <= 13 {
            return self.0.to_string();
        }
        let start = characters[..6].iter().collect::<String>();
        let end = characters[characters.len() - 4..]
            .iter()
            .collect::<String>();
        format!("{start}…{end}")
    }
}

pub(super) fn runtime_label(runtime: &str) -> String {
    let id = match runtime {
        "idle" => "session-runtime-idle",
        "installing" => "session-runtime-installing",
        "streaming" => "session-runtime-streaming",
        "awaiting" => "session-runtime-awaiting",
        "errored" => "session-runtime-errored",
        _ => "session-runtime-inactive",
    };
    translate(id)
}
