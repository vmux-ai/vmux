use std::collections::{HashMap, HashSet};

use chrono::{Duration, Local, NaiveDate, TimeZone};
use vmux_session::{
    AgentFamily, CatalogSnapshot, SessionAge, SessionSummary, SessionSummaryProjection,
    StageSummary,
};
use vmux_ui::i18n::{TranslationValue, translate, translate_humanized, translate_with};

use crate::event::{ProviderSessionTarget, SessionRepositories, SessionRepository};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum RowIdentity {
    Provider(AgentFamily, String),
    Local(String),
}

impl RowIdentity {
    pub(super) fn of(session: &SessionSummary) -> Self {
        if !session.provider_kind.is_empty() && !session.provider_sid.is_empty() {
            return Self::Provider(
                AgentFamily::of(session.provider_kind.trim_start_matches("acp:")),
                session.provider_sid.clone(),
            );
        }
        if !session.managed && !session.kind.is_empty() && !session.sid.is_empty() {
            return Self::Provider(
                AgentFamily::of(session.kind.trim_start_matches("acp:")),
                session.sid.clone(),
            );
        }
        Self::Local(session.id.clone())
    }

    pub(super) fn key(&self) -> String {
        format!("{self:?}")
    }
}

#[derive(Default)]
pub(super) struct CatalogDisplay {
    settled: Option<CatalogSnapshot>,
}

impl CatalogDisplay {
    pub(super) fn update(
        &mut self,
        mut incoming: CatalogSnapshot,
        loading: bool,
    ) -> Option<CatalogSnapshot> {
        if loading && incoming.sessions.is_empty() {
            return self.settled.clone();
        }
        if let Some(previous) = &self.settled {
            let mut rows = HashMap::new();
            let mut order = Vec::new();
            for session in incoming.sessions {
                let key = RowIdentity::of(&session);
                order.push(key.clone());
                rows.insert(key, session);
            }
            incoming.sessions = Vec::new();
            for session in &previous.sessions {
                if let Some(updated) = rows.remove(&RowIdentity::of(session)) {
                    incoming.sessions.push(updated);
                } else if loading {
                    incoming.sessions.push(session.clone());
                }
            }
            for id in order {
                if let Some(session) = rows.remove(&id) {
                    incoming.sessions.push(session);
                }
            }
        }
        self.settled = Some(incoming.clone());
        Some(incoming)
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct ManagerPresentation {
    pub loading: bool,
    pub snapshot: CatalogSnapshot,
    pub repositories: SessionRepositories,
    pub category_groups: Vec<StageGroup>,
    pub groups: Vec<StageGroup>,
    pub selected_filters: Vec<SessionFilterPresentation>,
    pub selected_stage: Option<String>,
    pub selected_archived: bool,
    pub archived_count: usize,
    pub filter_requested: bool,
    pub filter_needle: String,
    pub visible_sessions: bool,
    pub visible_ids: HashSet<String>,
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
        mut retain_archived: impl FnMut(&String) -> bool,
    ) -> Self {
        for session in &mut snapshot.sessions {
            session.stage = stage_for(&session.id, &session.stage);
            session.archived_at = if archived_for(&session.id, session.archived_at > 0) {
                session.archived_at.max(1)
            } else {
                0
            };
        }
        let selected_archived = SessionFilter::archived(filters);
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
        let archived_count = snapshot
            .sessions
            .iter()
            .filter(|session| session.archived_at > 0)
            .count();
        let active_sessions = snapshot
            .sessions
            .iter()
            .filter(|session| {
                session.archived_at == 0 || (!selected_archived && retain_archived(&session.id))
            })
            .cloned()
            .collect::<Vec<_>>();
        let archived_sessions = snapshot
            .sessions
            .iter()
            .filter(|session| {
                session.archived_at > 0 || (selected_archived && retain_archived(&session.id))
            })
            .cloned()
            .collect::<Vec<_>>();
        let category_groups = StageGroup::from_catalog(&snapshot.stages, &active_sessions, &search);
        let groups = if selected_archived {
            StageGroup::filtered(
                &StageGroup::archived(&archived_sessions, &search),
                filters,
                &repositories,
            )
        } else {
            let filtered_groups = StageGroup::filtered(&category_groups, filters, &repositories);
            if SessionFilter::stage(filters).is_none() {
                StageGroup::inbox(&filtered_groups)
            } else {
                filtered_groups
            }
        };
        let visible_ids = groups
            .iter()
            .flat_map(|group| group.sessions.iter().map(|session| session.id.clone()))
            .collect::<HashSet<_>>();
        let visible_sessions = !visible_ids.is_empty();
        let selected_filters = SessionFilter::presentations(filters, &snapshot.stages);
        let selected_stage = SessionFilter::stage(filters).map(str::to_string);
        Self {
            loading: false,
            snapshot,
            repositories,
            category_groups,
            groups,
            selected_filters,
            selected_stage,
            selected_archived,
            archived_count,
            filter_requested,
            filter_needle,
            visible_sessions,
            visible_ids,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) enum SessionFilter {
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

    pub fn stage(filters: &[Self]) -> Option<&str> {
        filters.iter().find_map(|filter| match filter {
            Self::Stage(id) => Some(id.as_str()),
            Self::Current
            | Self::Archived
            | Self::Agent(_)
            | Self::Project(_)
            | Self::Updated(_) => None,
        })
    }

    pub fn archived(filters: &[Self]) -> bool {
        filters
            .iter()
            .any(|filter| matches!(filter, Self::Archived))
    }

    pub fn set_stage(filters: &mut Vec<Self>, stage: Option<String>) {
        filters.retain(|filter| !matches!(filter, Self::Current | Self::Stage(_) | Self::Archived));
        if let Some(stage) = stage {
            filters.insert(0, Self::Stage(stage));
        }
    }

    pub fn upsert(filters: &mut Vec<Self>, selected: Self) {
        match selected {
            Self::Current => {
                Self::set_stage(filters, None);
                filters.insert(0, Self::Current);
            }
            Self::Stage(stage) => Self::set_stage(filters, Some(stage)),
            Self::Archived => {
                Self::set_stage(filters, None);
                filters.insert(0, Self::Archived);
            }
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
                    dot: StageVisual::for_id(id).dot.to_string(),
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
                    updated.value_label
                ),
                dot: String::new(),
            },
        }
    }

    fn matches(&self, session: &SessionSummary, repositories: &SessionRepositories) -> bool {
        match self {
            Self::Current | Self::Stage(_) | Self::Archived => true,
            Self::Agent(agent) => SessionFilterValue::agent(session) == *agent,
            Self::Project(project) => {
                if project.starts_with('/')
                    && std::path::Path::new(&session.cwd).starts_with(project)
                {
                    return true;
                }
                repositories.get(&session.id).is_some_and(|repository| {
                    repository.project == *project || repository.project_url == *project
                })
            }
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
            SessionFilter::Current
            | SessionFilter::Archived
            | SessionFilter::Agent(_)
            | SessionFilter::Project(_)
            | SessionFilter::Updated(_) => None,
        });
        let mut filtered = Vec::new();
        for group in groups {
            if filters.contains(&SessionFilter::Current)
                && (group.stage.terminal || group.stage.id == "stale")
            {
                continue;
            }
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

    pub fn archived(sessions: &[SessionSummary], search: &str) -> Vec<Self> {
        vec![Self {
            stage: StageSummary {
                id: "archived".into(),
                name: "common-archived".into(),
                order: u32::MAX,
                terminal: true,
            },
            sessions: sessions
                .iter()
                .filter(|session| SessionSummaryProjection::matches(session, search))
                .cloned()
                .collect(),
        }]
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct StageGroupPresentation {
    pub dot: &'static str,
    pub archivable: Vec<ArchiveTarget>,
    pub restorable: Vec<ArchiveTarget>,
}

impl StageGroupPresentation {
    pub fn new(stage: &StageSummary, sessions: &[SessionSummary]) -> Self {
        let mut archivable = Vec::new();
        let mut restorable = Vec::new();
        for session in sessions {
            let target = ArchiveTarget::from_session(session);
            if target.archived {
                restorable.push(target);
            } else {
                archivable.push(target);
            }
        }
        Self {
            dot: StageVisual::for_id(&stage.id).dot,
            archivable,
            restorable,
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
pub(super) struct DeleteTarget {
    pub id: String,
    pub provider: Option<ProviderSessionTarget>,
}

impl DeleteTarget {
    pub fn from_session(session: &SessionSummary) -> Self {
        let provider = (!session.provider_kind.is_empty() && !session.provider_sid.is_empty())
            .then(|| ProviderSessionTarget {
                kind: session.provider_kind.clone(),
                sid: session.provider_sid.clone(),
            });
        Self {
            id: session.id.clone(),
            provider,
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
    pub project_identity: String,
    pub project: String,
    pub project_url: String,
    pub branch: String,
    pub git_url: String,
    pub editor_url: String,
}

impl SessionContext {
    fn from(repository: Option<SessionRepository>, cwd: &str) -> Self {
        let editor_url = url::Url::from_directory_path(cwd)
            .map(|mut url| {
                url.query_pairs_mut().append_pair("vmux-entry", "1");
                url.to_string()
            })
            .unwrap_or_default();
        match repository {
            Some(repository) => Self {
                project_identity: repository.project_identity,
                project: repository.project,
                project_url: repository.project_url,
                branch: repository.branch,
                git_url: repository.url,
                editor_url,
            },
            None => Self {
                project_identity: String::new(),
                project: String::new(),
                project_url: String::new(),
                branch: String::new(),
                git_url: String::new(),
                editor_url,
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
            context: SessionContext::from(repository, &session.cwd),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn working_directory_links_preserve_the_path_and_request_a_code_entry_file() {
        let path = "/tmp/a project/日本語";
        let context = SessionContext::from(None, path);
        let url = url::Url::parse(&context.editor_url).unwrap();
        assert_eq!(url.to_file_path().unwrap(), std::path::PathBuf::from(path));
        assert!(
            url.query_pairs()
                .any(|(key, value)| key == "vmux-entry" && value == "1")
        );
    }

    #[test]
    fn discovery_replaces_a_provider_row_in_place_when_it_becomes_managed() {
        let before = SessionSummary {
            id: "before".into(),
            managed: true,
            ..Default::default()
        };
        let after = SessionSummary {
            id: "after".into(),
            managed: true,
            ..Default::default()
        };
        let provider = SessionSummary {
            id: "provider-row".into(),
            kind: "acp:codex".into(),
            sid: "provider-session".into(),
            ..Default::default()
        };
        let managed = SessionSummary {
            id: "canonical".into(),
            managed: true,
            provider_kind: "acp:codex".into(),
            provider_sid: "provider-session".into(),
            ..Default::default()
        };
        let mut display = CatalogDisplay::default();
        display.update(
            CatalogSnapshot {
                sessions: vec![before.clone(), provider.clone(), after.clone()],
                ..Default::default()
            },
            true,
        );
        let incoming = CatalogSnapshot {
            sessions: vec![after, managed.clone(), before],
            ..Default::default()
        };
        for loading in [true, false] {
            let shown = display.update(incoming.clone(), loading).unwrap();
            assert_eq!(
                shown
                    .sessions
                    .iter()
                    .map(|session| session.id.as_str())
                    .collect::<Vec<_>>(),
                ["before", "canonical", "after"]
            );
            assert!(shown.sessions[1].managed);
            assert_eq!(
                RowIdentity::of(&provider).key(),
                RowIdentity::of(&shown.sessions[1]).key()
            );
        }
    }

    #[test]
    fn discovery_shows_ready_rows_and_appends_without_reordering_them() {
        let mut display = CatalogDisplay::default();
        assert!(display.update(CatalogSnapshot::default(), true).is_none());
        let mut initial = CatalogSnapshot::default();
        for id in ["a", "b"] {
            initial.sessions.push(SessionSummary {
                id: id.into(),
                ..Default::default()
            });
        }
        let initial = display.update(initial, true).unwrap();
        assert_eq!(
            initial
                .sessions
                .iter()
                .map(|session| session.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        let mut refreshed = CatalogSnapshot::default();
        for id in ["c", "b", "a"] {
            refreshed.sessions.push(SessionSummary {
                id: id.into(),
                ..Default::default()
            });
        }
        let pending = display.update(refreshed.clone(), true).unwrap();
        assert_eq!(
            pending
                .sessions
                .iter()
                .map(|session| session.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        let complete = display.update(refreshed, false).unwrap();
        assert_eq!(
            complete
                .sessions
                .iter()
                .map(|session| session.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        let partial = CatalogSnapshot {
            sessions: complete
                .sessions
                .into_iter()
                .filter(|session| session.id != "a")
                .collect(),
            ..Default::default()
        };
        let pending = display.update(partial.clone(), true).unwrap();
        assert_eq!(
            pending
                .sessions
                .iter()
                .map(|session| session.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        let complete = display.update(partial, false).unwrap();
        assert_eq!(
            complete
                .sessions
                .iter()
                .map(|session| session.id.as_str())
                .collect::<Vec<_>>(),
            ["b", "c"]
        );
    }
}
