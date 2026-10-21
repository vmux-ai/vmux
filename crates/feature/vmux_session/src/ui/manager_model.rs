use std::collections::HashMap;

use chrono::{Duration, Local, NaiveDate, TimeZone};
use vmux_session::{CatalogSnapshot, SessionSummary, StageSummary};
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

    pub fn new(
        mut snapshot: CatalogSnapshot,
        repositories: SessionRepositories,
        query: &str,
        filters: &SessionFilters,
        mut stage_for: impl FnMut(&String, &String) -> String,
    ) -> Self {
        for session in &mut snapshot.sessions {
            session.stage = stage_for(&session.id, &session.stage);
        }
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
        let filtered_groups =
            StageGroup::filtered(&category_groups, filters.as_slice(), &repositories);
        let groups = if filters.stage().is_none() {
            StageGroup::inbox(&filtered_groups)
        } else {
            filtered_groups
        };
        let visible_sessions = groups.iter().any(|group| !group.sessions.is_empty());
        let selected_filters = filters.presentations(&snapshot.stages);
        let selected_stage = filters.stage().map(str::to_string);
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

#[derive(Clone, Default, PartialEq)]
pub(super) struct SessionFilters(Vec<SessionFilter>);

impl SessionFilters {
    pub fn as_slice(&self) -> &[SessionFilter] {
        &self.0
    }

    pub fn presentations(&self, stages: &[StageSummary]) -> Vec<SessionFilterPresentation> {
        self.0
            .iter()
            .map(|filter| filter.presentation(stages))
            .collect()
    }

    pub fn stage(&self) -> Option<&str> {
        self.0.iter().find_map(|filter| match filter {
            SessionFilter::Stage(id) => Some(id.as_str()),
            SessionFilter::Agent(_) | SessionFilter::Project(_) | SessionFilter::Updated(_) => None,
        })
    }

    pub fn set_stage(&mut self, stage: Option<String>) {
        self.0
            .retain(|filter| !matches!(filter, SessionFilter::Stage(_)));
        if let Some(stage) = stage {
            self.0.insert(0, SessionFilter::Stage(stage));
        }
    }

    pub fn upsert(&mut self, selected: SessionFilter) {
        match selected {
            SessionFilter::Stage(stage) => self.set_stage(Some(stage)),
            SessionFilter::Agent(agent) => {
                self.0
                    .retain(|filter| !matches!(filter, SessionFilter::Agent(_)));
                self.0.push(SessionFilter::Agent(agent));
            }
            SessionFilter::Project(project) => {
                self.0
                    .retain(|filter| !matches!(filter, SessionFilter::Project(_)));
                self.0.push(SessionFilter::Project(project));
            }
            SessionFilter::Updated(updated) => {
                self.0
                    .retain(|filter| !matches!(filter, SessionFilter::Updated(_)));
                self.0.push(SessionFilter::Updated(updated));
            }
        }
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.0.len() {
            self.0.remove(index);
        }
    }

    pub fn pop(&mut self) {
        self.0.pop();
    }
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

pub(super) struct SessionFilterMenuPresentation {
    pub agents: Vec<(String, usize)>,
    pub projects: Vec<(String, usize)>,
    pub date_filter: Option<UpdatedFilter>,
    pub within_filter: Option<UpdatedFilter>,
}

impl SessionFilterMenuPresentation {
    pub fn new(
        sessions: &[SessionSummary],
        repositories: &SessionRepositories,
        step: SessionFilterStep,
        value: &str,
    ) -> Self {
        let mut agents = HashMap::<String, usize>::new();
        let mut projects = HashMap::<String, usize>::new();
        for session in sessions {
            let agent = SessionFilterValue::agent(session);
            if !agent.is_empty() {
                *agents.entry(agent).or_default() += 1;
            }
            if let Some(repository) = repositories.get(&session.id)
                && !repository.project.is_empty()
            {
                *projects.entry(repository.project).or_default() += 1;
            }
        }
        let mut agents = agents.into_iter().collect::<Vec<_>>();
        agents.sort_by_key(|left| left.0.to_lowercase());
        let mut projects = projects.into_iter().collect::<Vec<_>>();
        projects.sort_by_key(|left| left.0.to_lowercase());
        let date_filter = match step {
            SessionFilterStep::Date(comparison) => UpdatedFilter::from_date(comparison, value),
            _ => None,
        };
        let within_filter = if step == SessionFilterStep::Within {
            value.parse().ok().and_then(UpdatedFilter::within)
        } else {
            None
        };
        Self {
            agents,
            projects,
            date_filter,
            within_filter,
        }
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
        let mut groups = stages
            .iter()
            .cloned()
            .map(|stage| Self {
                stage,
                sessions: Vec::new(),
            })
            .collect::<Vec<_>>();
        for session in sessions {
            if !SessionSummaryPresentation::matches(session, search) {
                continue;
            }
            let terminal = stages
                .iter()
                .find(|stage| stage.id == session.stage)
                .is_some_and(|stage| stage.terminal);
            if SessionFreshness::is_stale(session, terminal) {
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
    pub archivable: Vec<StageTarget>,
}

impl StageGroupPresentation {
    pub fn new(stage: &StageSummary, sessions: &[SessionSummary]) -> Self {
        Self {
            dot: StageVisual::for_id(&stage.id).dot,
            archivable: sessions
                .iter()
                .filter(|session| session.stage != "done")
                .map(StageTarget::from_session)
                .collect(),
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
        stage: &str,
    ) -> Self {
        Self {
            context: SessionContext::from(repository),
            stage_target: StageTarget::from_session(session),
            archived: session.stage == "done",
            runtime: RuntimeVisual::for_id(&session.runtime),
            agent: AgentPresentation::from_session(session),
            stage_dot: StageVisual::for_id(stage).dot,
            title: SessionSummaryPresentation::display_title(session),
            preview: SessionSummaryPresentation::preview(session),
            compact_id: SessionIdentity::new(&session.sid).compact(),
            updated: RelativeTime::from_millis(session.last_activated_at).label(),
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

pub(super) struct SessionFreshness;

impl SessionFreshness {
    pub const STALE_AFTER: u64 = 3 * RelativeTime::DAY;

    fn is_stale(session: &SessionSummary, terminal: bool) -> bool {
        let age = RelativeTime::from_millis(session.last_activated_at);
        Self::from_age(age, &session.runtime, terminal)
    }

    pub fn from_age(age: RelativeTime, runtime: &str, terminal: bool) -> bool {
        !terminal && !RuntimeVisual::for_id(runtime).visible && age.0 >= Self::STALE_AFTER
    }
}

pub(super) struct RelativeTime(pub u64);

impl RelativeTime {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * Self::MINUTE;
    pub const DAY: u64 = 24 * Self::HOUR;
    const WEEK: u64 = 7 * Self::DAY;
    const MONTH: u64 = 30 * Self::DAY;
    const YEAR: u64 = 365 * Self::DAY;

    pub fn from_millis(timestamp: i64) -> Self {
        if timestamp <= 0 {
            return Self(0);
        }
        let now = vmux_ecs::UnixMillis::now().0;
        Self(now.saturating_sub(timestamp).max(0) as u64 / 1_000)
    }

    pub fn label(self) -> String {
        let (id, count) = if self.0 < Self::MINUTE {
            ("resume-age-now", 0)
        } else if self.0 < Self::HOUR {
            ("resume-age-minutes", self.0 / Self::MINUTE)
        } else if self.0 < Self::DAY {
            ("resume-age-hours", self.0 / Self::HOUR)
        } else if self.0 < Self::WEEK {
            ("resume-age-days", self.0 / Self::DAY)
        } else if self.0 < Self::MONTH {
            ("resume-age-weeks", self.0 / Self::WEEK)
        } else if self.0 < Self::YEAR {
            ("resume-age-months", self.0 / Self::MONTH)
        } else {
            ("resume-age-years", self.0 / Self::YEAR)
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

struct SessionSummaryPresentation;

impl SessionSummaryPresentation {
    fn matches(session: &SessionSummary, query: &str) -> bool {
        query.is_empty()
            || session.name.to_ascii_lowercase().contains(query)
            || session.description.to_ascii_lowercase().contains(query)
            || session.cwd.to_ascii_lowercase().contains(query)
            || session.agent.to_ascii_lowercase().contains(query)
            || session.agent_name.to_ascii_lowercase().contains(query)
            || session
                .conversation_title
                .to_ascii_lowercase()
                .contains(query)
            || session.last_prompt.to_ascii_lowercase().contains(query)
            || session.last_message.to_ascii_lowercase().contains(query)
    }

    fn display_title(session: &SessionSummary) -> String {
        if session.conversation_title.is_empty() {
            return session.name.clone();
        }
        session.conversation_title.clone()
    }

    fn preview(session: &SessionSummary) -> String {
        let title = Self::display_title(session);
        if !session.last_prompt.is_empty() && session.last_prompt != title {
            return session.last_prompt.clone();
        }
        if !session.last_message.is_empty() && session.last_message != title {
            return session.last_message.clone();
        }
        String::new()
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
