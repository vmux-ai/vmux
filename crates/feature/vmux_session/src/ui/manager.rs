use std::collections::{HashMap, HashSet};

use chrono::{Duration, Local, NaiveDate, TimeZone};
use dioxus::prelude::*;
use vmux_session::{SessionSummary, StageSummary};
use vmux_ui::components::avatar::AgentThumbnail;
use vmux_ui::components::dropdown::Dropdown;
use vmux_ui::components::filter::{FilterMenu, FilterOption, FilterPill, FilterTrigger};
use vmux_ui::components::manager::{
    ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerPage, ManagerSearch, ManagerToolbar,
    ManagerView, ManagerViewToggle,
};
use vmux_ui::components::motion::Collapse;
use vmux_ui::components::sheet::{SheetHeaderAction, SheetHeaderChevron};
use vmux_ui::hooks::{OptimisticMap, send, use_optimistic_map, use_theme, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_humanized, translate_with};
use vmux_ui::icon::{GitSymbol, GitSymbolView, LineIcon, LineIconView};

use crate::event::{
    ChatOpenPage, ResumeSession, SessionRepositories, SessionRepository, SessionsCleanup,
    SessionsDescriptionUpdate, SessionsStageChange,
};
use crate::state::ChatUiState;

#[component]
pub fn SessionsManager() -> Element {
    use_theme();
    let ui = use_ui_state::<ChatUiState>();
    let mut query = use_signal(String::new);
    let mut view = use_signal(ManagerView::default);
    let mut filters = use_signal(SessionFilters::default);
    let mut filter_step = use_signal(|| SessionFilterStep::Field);
    let mut filter_value = use_signal(String::new);
    let state = ui.state.read();
    let mut snapshot = state.sessions.clone();
    let repositories = state.session_repositories.clone();
    drop(state);
    let observed_stages = snapshot
        .sessions
        .iter()
        .map(|session| (session.id.clone(), session.stage.clone()))
        .collect::<HashMap<_, _>>();
    let optimistic_stages = use_optimistic_map(observed_stages);
    for session in &mut snapshot.sessions {
        session.stage = optimistic_stages.value(&session.id, &session.stage);
    }
    let mutations = SessionStageMutations {
        optimistic: optimistic_stages,
        leaving: use_signal(HashSet::new),
    };
    use_context_provider(|| mutations);
    let filter_requested = query().starts_with('#');
    let filter_needle = query()
        .strip_prefix('#')
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let search = if filter_requested {
        String::new()
    } else {
        query().trim().to_ascii_lowercase()
    };
    let category_groups = StageGroup::from_catalog(&snapshot.stages, &snapshot.sessions, &search);
    let applied_filters = filters();
    let filtered_groups =
        StageGroup::filtered(&category_groups, applied_filters.as_slice(), &repositories);
    let groups = if applied_filters.stage().is_none() {
        StageGroup::inbox(&filtered_groups)
    } else {
        filtered_groups
    };
    let visible_sessions = groups.iter().any(|group| !group.sessions.is_empty());
    let stages = snapshot.stages.clone();
    let selected_filters = applied_filters.presentations(&stages);
    rsx! {
        ManagerPage {
            ManagerToolbar {
                Dropdown {
                    open: filter_requested,
                    class: "min-w-0 flex-1".to_string(),
                    keyboard_navigation: true,
                    on_dismiss: move |_| {
                        query.set(String::new());
                        filter_step.set(SessionFilterStep::Field);
                        filter_value.set(String::new());
                    },
                    ManagerSearch {
                        value: query(),
                        placeholder: translate("sessions-search"),
                        onsearch: move |event: FormEvent| query.set(event.value()),
                        onkeydown: Some(EventHandler::new(move |event: KeyboardEvent| {
                            if event.key() == Key::Backspace && query().is_empty() {
                                filters.write().pop();
                            }
                        })),
                        prefix: rsx! {
                            for (index, presentation) in selected_filters.iter().cloned().enumerate() {
                                FilterPill {
                                    key: "filter-pill-{index}",
                                    label: presentation.label,
                                    dot: presentation.dot,
                                    clear_label: translate("common-remove-filter"),
                                    onclear: move |_| filters.write().remove(index),
                                }
                            }
                        },
                        suffix: rsx! {
                            FilterTrigger {
                                active: filter_requested,
                                label: translate("common-filter"),
                                onclick: move |_| {
                                    if filter_requested {
                                        query.set(String::new());
                                    } else {
                                        query.set("#".to_string());
                                    }
                                    filter_step.set(SessionFilterStep::Field);
                                    filter_value.set(String::new());
                                },
                            }
                        },
                    }
                    if filter_requested {
                        SessionFilterMenu {
                            groups: category_groups.clone(),
                            sessions: snapshot.sessions.clone(),
                            repositories: repositories.clone(),
                            needle: filter_needle,
                            step: filter_step(),
                            value: filter_value(),
                            onstep: move |step| {
                                filter_step.set(step);
                                filter_value.set(String::new());
                                query.set("#".to_string());
                            },
                            onvalue: move |value| filter_value.set(value),
                            onselect: move |selected| {
                                filters.write().upsert(selected);
                                query.set(String::new());
                                filter_step.set(SessionFilterStep::Field);
                                filter_value.set(String::new());
                            },
                        }
                    }
                }
                ManagerViewToggle {
                    view: view(),
                    list_label: translate("sessions-view-list"),
                    grid_label: translate("sessions-view-board"),
                    grid_icon: LineIcon::Columns3,
                    onchange: move |selected| view.set(selected),
                }
                ManagerButton {
                    variant: ManagerButtonVariant::Primary,
                    onclick: move |_| {
                        let _ = send(&ChatOpenPage {
                            url: "vmux://start/".to_string(),
                        });
                    },
                    span { class: "inline-flex items-center gap-1.5",
                        LineIconView { icon: LineIcon::Plus, class: "size-3.5".to_string() }
                        span { {translate("sessions-start")} }
                    }
                }
            }
            div { class: "flex min-h-0 flex-1 overflow-hidden bg-background/45",
                SessionCategories {
                    groups: category_groups.clone(),
                    selected: applied_filters.stage().map(str::to_string),
                    onselect: move |selected| {
                        filters.write().set_stage(selected);
                        query.set(String::new());
                    },
                }
                div { class: "flex min-w-0 flex-1 overflow-hidden",
                    if snapshot.sessions.is_empty() {
                        ManagerEmpty {
                            title: translate("sessions-empty"),
                            detail: translate("sessions-empty-detail"),
                            actions: rsx! { StartSessionButton {} },
                        }
                    } else if !visible_sessions {
                        ManagerEmpty {
                            title: translate("sessions-inbox-empty"),
                            detail: translate("sessions-inbox-empty-detail"),
                            actions: rsx! { StartSessionButton {} },
                        }
                    } else if view() == ManagerView::List {
                        SessionsList {
                            groups: groups.clone(),
                            repositories: repositories.clone(),
                        }
                    } else {
                        SessionsBoard {
                            groups: groups.clone(),
                            repositories: repositories.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StartSessionButton() -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "vmux-primary-action mt-2 inline-flex h-8 items-center gap-1.5 rounded-md px-3 text-[11px] font-medium",
            onclick: move |_| {
                let _ = send(&ChatOpenPage {
                    url: "vmux://start/".to_string(),
                });
            },
            LineIconView { icon: LineIcon::Plus, class: "size-3.5".to_string() }
            span { {translate("sessions-start")} }
        }
    }
}

#[component]
fn SessionCategories(
    groups: Vec<StageGroup>,
    selected: Option<String>,
    onselect: EventHandler<Option<String>>,
) -> Element {
    let inbox_count = groups
        .iter()
        .filter(|group| !group.stage.terminal)
        .map(|group| group.sessions.len())
        .sum::<usize>();
    rsx! {
        aside { class: "hidden w-44 shrink-0 flex-col bg-foreground/[0.018] p-2 sm:flex",
            h2 { class: "px-2 pb-2 pt-1 text-xs font-semibold text-foreground/90", {translate("sessions-title")} }
            SessionCategory {
                label: translate("sessions-inbox"),
                count: inbox_count,
                selected: selected.is_none(),
                icon: Some(LineIcon::Inbox),
                onclick: move |_| onselect.call(None),
            }
            div { class: "h-2" }
            for group in groups {
                if !group.sessions.is_empty() {
                    SessionCategory {
                        key: "{group.stage.id}",
                        label: translate_humanized(&group.stage.name),
                        count: group.sessions.len(),
                        selected: selected.as_ref() == Some(&group.stage.id),
                        dot: StageVisual::for_id(&group.stage.id).dot.to_string(),
                        onclick: move |_| onselect.call(Some(group.stage.id.clone())),
                    }
                }
            }
        }
    }
}

#[component]
fn SessionCategory(
    label: String,
    count: usize,
    selected: bool,
    #[props(default)] icon: Option<LineIcon>,
    #[props(default)] dot: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if selected {
        "bg-foreground/[0.065] text-foreground"
    } else {
        "text-muted-foreground hover:bg-foreground/[0.035] hover:text-foreground"
    };
    rsx! {
        button {
            r#type: "button",
            class: "flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[11px] transition-colors {class}",
            onclick: move |event| onclick.call(event),
            if let Some(icon) = icon {
                LineIconView { icon, class: "size-3.5 shrink-0".to_string() }
            } else {
                span { class: "size-2 shrink-0 rounded-full {dot}" }
            }
            span { class: "min-w-0 flex-1 truncate", "{label}" }
            span { class: "text-[10px] tabular-nums text-muted-foreground/65", "{count}" }
        }
    }
}

#[derive(Clone, PartialEq)]
enum SessionFilter {
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
struct SessionFilterPresentation {
    label: String,
    dot: String,
}

#[derive(Clone, Default, PartialEq)]
struct SessionFilters(Vec<SessionFilter>);

impl SessionFilters {
    fn as_slice(&self) -> &[SessionFilter] {
        &self.0
    }

    fn presentations(&self, stages: &[StageSummary]) -> Vec<SessionFilterPresentation> {
        self.0
            .iter()
            .map(|filter| filter.presentation(stages))
            .collect()
    }

    fn stage(&self) -> Option<&str> {
        self.0.iter().find_map(|filter| match filter {
            SessionFilter::Stage(id) => Some(id.as_str()),
            SessionFilter::Agent(_) | SessionFilter::Project(_) | SessionFilter::Updated(_) => None,
        })
    }

    fn set_stage(&mut self, stage: Option<String>) {
        self.0
            .retain(|filter| !matches!(filter, SessionFilter::Stage(_)));
        if let Some(stage) = stage {
            self.0.insert(0, SessionFilter::Stage(stage));
        }
    }

    fn upsert(&mut self, selected: SessionFilter) {
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

    fn remove(&mut self, index: usize) {
        if index < self.0.len() {
            self.0.remove(index);
        }
    }

    fn pop(&mut self) {
        self.0.pop();
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SessionFilterStep {
    Field,
    Status,
    Agent,
    Project,
    Updated,
    Date(UpdatedComparison),
    Within,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum UpdatedComparison {
    Before,
    OnOrBefore,
    After,
    OnOrAfter,
    Within,
}

impl UpdatedComparison {
    fn label(self) -> String {
        translate(match self {
            Self::Before => "common-before",
            Self::OnOrBefore => "common-on-or-before",
            Self::After => "common-after",
            Self::OnOrAfter => "common-on-or-after",
            Self::Within => "common-within",
        })
    }

    fn detail(self) -> String {
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
struct UpdatedFilter {
    comparison: UpdatedComparison,
    threshold: i64,
    value_label: String,
}

impl UpdatedFilter {
    fn from_date(comparison: UpdatedComparison, value: &str) -> Option<Self> {
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

    fn within(days: u64) -> Option<Self> {
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

#[component]
fn SessionFilterMenu(
    groups: Vec<StageGroup>,
    sessions: Vec<SessionSummary>,
    repositories: SessionRepositories,
    needle: String,
    step: SessionFilterStep,
    value: String,
    onstep: EventHandler<SessionFilterStep>,
    onvalue: EventHandler<String>,
    onselect: EventHandler<SessionFilter>,
) -> Element {
    let mut agents = HashMap::<String, usize>::new();
    let mut projects = HashMap::<String, usize>::new();
    for session in &sessions {
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
        SessionFilterStep::Date(comparison) => UpdatedFilter::from_date(comparison, &value),
        _ => None,
    };
    let within_filter = if step == SessionFilterStep::Within {
        value.parse().ok().and_then(UpdatedFilter::within)
    } else {
        None
    };
    match step {
        SessionFilterStep::Field => rsx! {
            FilterMenu { title: translate("common-filter"),
                if needle.is_empty() || translate("common-status").to_ascii_lowercase().contains(&needle) {
                    FilterOption {
                        label: translate("common-status"),
                        prefix: "#".to_string(),
                        next: true,
                        onclick: move |_| onstep.call(SessionFilterStep::Status),
                    }
                }
                if needle.is_empty() || translate("resume-column-agent").to_ascii_lowercase().contains(&needle) {
                    FilterOption {
                        label: translate("resume-column-agent"),
                        prefix: "#".to_string(),
                        next: true,
                        onclick: move |_| onstep.call(SessionFilterStep::Agent),
                    }
                }
                if needle.is_empty() || translate("resume-column-project").to_ascii_lowercase().contains(&needle) {
                    FilterOption {
                        label: translate("resume-column-project"),
                        prefix: "#".to_string(),
                        next: true,
                        onclick: move |_| onstep.call(SessionFilterStep::Project),
                    }
                }
                if needle.is_empty() || translate("common-updated").to_ascii_lowercase().contains(&needle) {
                    FilterOption {
                        label: translate("common-updated"),
                        prefix: "#".to_string(),
                        next: true,
                        onclick: move |_| onstep.call(SessionFilterStep::Updated),
                    }
                }
            }
        },
        SessionFilterStep::Status => rsx! {
            FilterMenu {
                title: translate("common-status"),
                onback: move |_| onstep.call(SessionFilterStep::Field),
                for group in groups {
                    if needle.is_empty() || translate_humanized(&group.stage.name).to_ascii_lowercase().contains(&needle) {
                        FilterOption {
                            key: "filter-{group.stage.id}",
                            label: translate_humanized(&group.stage.name),
                            dot: StageVisual::for_id(&group.stage.id).dot.to_string(),
                            trailing: group.sessions.len().to_string(),
                            onclick: move |_| onselect.call(SessionFilter::Stage(group.stage.id.clone())),
                        }
                    }
                }
            }
        },
        SessionFilterStep::Agent => rsx! {
            FilterMenu {
                title: translate("resume-column-agent"),
                onback: move |_| onstep.call(SessionFilterStep::Field),
                for (agent, count) in agents {
                    if needle.is_empty() || agent.to_ascii_lowercase().contains(&needle) {
                        FilterOption {
                            key: "agent-{agent}",
                            label: agent.clone(),
                            trailing: count.to_string(),
                            onclick: move |_| onselect.call(SessionFilter::Agent(agent.clone())),
                        }
                    }
                }
            }
        },
        SessionFilterStep::Project => rsx! {
            FilterMenu {
                title: translate("resume-column-project"),
                onback: move |_| onstep.call(SessionFilterStep::Field),
                for (project, count) in projects {
                    if needle.is_empty() || project.to_ascii_lowercase().contains(&needle) {
                        FilterOption {
                            key: "project-{project}",
                            label: project.clone(),
                            trailing: count.to_string(),
                            onclick: move |_| onselect.call(SessionFilter::Project(project.clone())),
                        }
                    }
                }
            }
        },
        SessionFilterStep::Updated => rsx! {
            FilterMenu {
                title: translate("common-updated"),
                onback: move |_| onstep.call(SessionFilterStep::Field),
                for comparison in [
                    UpdatedComparison::Before,
                    UpdatedComparison::OnOrBefore,
                    UpdatedComparison::After,
                    UpdatedComparison::OnOrAfter,
                ] {
                    FilterOption {
                        key: "updated-{comparison:?}",
                        label: comparison.label(),
                        detail: comparison.detail(),
                        next: true,
                        onclick: move |_| onstep.call(SessionFilterStep::Date(comparison)),
                    }
                }
                FilterOption {
                    label: UpdatedComparison::Within.label(),
                    detail: UpdatedComparison::Within.detail(),
                    next: true,
                    onclick: move |_| onstep.call(SessionFilterStep::Within),
                }
            }
        },
        SessionFilterStep::Date(comparison) => rsx! {
            FilterMenu {
                title: format!("{} · {}", translate("common-updated"), comparison.label()),
                onback: move |_| onstep.call(SessionFilterStep::Updated),
                div { class: "flex items-center gap-2 p-1",
                    input {
                        r#type: "date",
                        class: "h-8 min-w-0 flex-1 rounded-md bg-foreground/[0.035] px-2 text-[11px] text-foreground outline-none ring-1 ring-inset ring-foreground/[0.09] focus:ring-foreground/[0.18]",
                        value: value,
                        oninput: move |event| onvalue.call(event.value()),
                    }
                    button {
                        r#type: "button",
                        class: "vmux-primary-action h-8 shrink-0 rounded-md px-3 text-[11px] font-medium disabled:cursor-default disabled:opacity-35",
                        disabled: date_filter.is_none(),
                        onclick: move |_| {
                            if let Some(updated) = date_filter.clone() {
                                onselect.call(SessionFilter::Updated(updated));
                            }
                        },
                        {translate("common-apply")}
                    }
                }
            }
        },
        SessionFilterStep::Within => rsx! {
            FilterMenu {
                title: format!("{} · {}", translate("common-updated"), UpdatedComparison::Within.label()),
                onback: move |_| onstep.call(SessionFilterStep::Updated),
                div { class: "flex items-center gap-2 p-1",
                    div { class: "flex h-8 min-w-0 flex-1 items-center rounded-md bg-foreground/[0.035] ring-1 ring-inset ring-foreground/[0.09] focus-within:ring-foreground/[0.18]",
                        input {
                            r#type: "number",
                            min: "1",
                            class: "h-full min-w-0 flex-1 bg-transparent px-2 text-[11px] text-foreground outline-none",
                            value,
                            oninput: move |event| onvalue.call(event.value()),
                        }
                        span { class: "shrink-0 pr-2 text-[10px] text-muted-foreground", {translate("common-days")} }
                    }
                    button {
                        r#type: "button",
                        class: "vmux-primary-action h-8 shrink-0 rounded-md px-3 text-[11px] font-medium disabled:cursor-default disabled:opacity-35",
                        disabled: within_filter.is_none(),
                        onclick: move |_| {
                            if let Some(updated) = within_filter.clone() {
                                onselect.call(SessionFilter::Updated(updated));
                            }
                        },
                        {translate("common-apply")}
                    }
                }
            }
        },
    }
}

#[component]
fn SessionsList(groups: Vec<StageGroup>, repositories: SessionRepositories) -> Element {
    rsx! {
        div { class: "min-h-0 flex-1 overflow-auto",
            div { class: "w-full min-w-[44rem] space-y-2 p-2",
                for group in groups {
                    if !group.sessions.is_empty() {
                        SessionListGroup {
                            key: "{group.stage.id}",
                            stage: group.stage,
                            sessions: group.sessions,
                            repositories: repositories.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionListGroup(
    stage: StageSummary,
    sessions: Vec<SessionSummary>,
    repositories: SessionRepositories,
) -> Element {
    let mut collapsed = use_signal(|| false);
    let pager = SessionPager::use_state();
    let total = sessions.len();
    let visible = pager.visible(total);
    let stage_dot = StageVisual::for_id(&stage.id).dot;
    let archivable = sessions
        .iter()
        .filter(|session| session.stage != "done")
        .map(StageTarget::from_session)
        .collect::<Vec<_>>();
    rsx! {
        section { class: "rounded-lg bg-foreground/[0.014] p-1",
            div {
                class: "glass sticky top-0 z-10 flex h-8 w-full items-center gap-2 rounded-md px-2.5 text-left backdrop-blur-xl transition-colors hover:bg-glass-hover",
                SheetHeaderChevron {
                    expanded: !collapsed(),
                    label: if collapsed() { translate("common-expand") } else { translate("common-collapse") },
                    onclick: move |event: MouseEvent| {
                        event.stop_propagation();
                        collapsed.set(!collapsed());
                    },
                }
                button {
                    r#type: "button",
                    class: "flex min-w-0 flex-1 items-center gap-2 text-left",
                    aria_expanded: !collapsed(),
                    onclick: move |_| collapsed.set(!collapsed()),
                    span { class: "flex size-4 shrink-0 items-center justify-center",
                        span { class: "size-2 rounded-full {stage_dot}" }
                    }
                    h2 { class: "truncate text-[11px] font-medium text-foreground/85", {translate_humanized(&stage.name)} }
                    span { class: "text-[10px] tabular-nums text-muted-foreground/75", "{sessions.len()}" }
                }
                if stage.id == "stale" && !archivable.is_empty() {
                    SessionArchiveAllButton { targets: archivable }
                }
            }
            if !collapsed() {
                div { class: "space-y-px pt-0.5",
                    for session in sessions.into_iter().take(visible) {
                        SessionListRow {
                            key: "{session.id}",
                            repository: repositories.get(&session.id),
                            stage: stage.id.clone(),
                            session,
                        }
                    }
                    if visible < total {
                        div {
                            class: "h-1",
                            onvisible: move |event| pager.load_more(event, total),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionListRow(
    session: SessionSummary,
    repository: Option<SessionRepository>,
    stage: String,
) -> Element {
    let leaving = use_context::<SessionStageMutations>().is_leaving(&session.id);
    let context = SessionContext::from(repository);
    let cleanup_id = session.id.clone();
    let stage_target = StageTarget::from_session(&session);
    let archived = session.stage == "done";
    let runtime = RuntimeVisual::for_id(&session.runtime);
    let agent = AgentPresentation::from_session(&session);
    let stage_dot = StageVisual::for_id(&stage).dot;
    let title = session.display_title();
    let preview = session.preview();
    let compact_id = SessionIdentity::new(&session.sid).compact();
    let git_url = context.git_url.clone();
    let resume = ResumeTarget::from(&session);
    let open_session = resume.clone();
    let updated = RelativeTime::from_millis(session.last_activated_at).label();
    rsx! {
        Collapse { leaving,
          div {
            class: "group relative flex min-h-11 cursor-pointer items-center gap-2 rounded-md px-2.5 py-1 text-xs transition-colors hover:bg-foreground/[0.03]",
            onclick: move |_| open_session.open(),
            span { class: "size-3 shrink-0" }
            AgentThumbnail {
                src: agent.icon.clone(),
                seed: agent.id.clone(),
                background: agent.color.clone(),
                alt: agent.name.clone(),
                class: "size-4 shrink-0 text-[5px]".to_string(),
            }
            SessionIdPill { label: compact_id, title: session.sid.clone() }
            span { class: "flex size-4 shrink-0 items-center justify-center",
                span { class: "size-2 rounded-full {stage_dot}" }
            }
            div { class: "min-w-0 flex-1",
                div { class: "truncate font-medium text-foreground/90", "{title}" }
                if !preview.is_empty() {
                    div { class: "truncate text-[10px] leading-4 text-muted-foreground/60", "{preview}" }
                }
            }
            div { class: "flex min-w-0 shrink-0 items-center gap-1.5",
                SessionContextView { context: context.clone() }
                if runtime.visible {
                    MetadataPill {
                        dot: runtime.dot.to_string(),
                        label: runtime_label(&session.runtime),
                    }
                }
                time { class: "w-10 text-right text-[10px] tabular-nums text-muted-foreground/65", "{updated}" }
            }
            SessionActions {
                resume,
                git_url,
                cleanup_id,
                stage_target,
                archived,
                managed: session.managed,
            }
          }
        }
    }
}

#[component]
fn SessionsBoard(groups: Vec<StageGroup>, repositories: SessionRepositories) -> Element {
    rsx! {
        div { class: "min-h-0 flex-1 overflow-auto p-2",
            div { class: "flex min-h-full w-max items-start gap-2",
                for group in groups {
                    if !group.sessions.is_empty() {
                        SessionBoardColumn {
                            key: "{group.stage.id}",
                            stage: group.stage,
                            sessions: group.sessions,
                            repositories: repositories.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionBoardColumn(
    stage: StageSummary,
    sessions: Vec<SessionSummary>,
    repositories: SessionRepositories,
) -> Element {
    let mut collapsed = use_signal(|| false);
    let pager = SessionPager::use_state();
    let total = sessions.len();
    let visible = pager.visible(total);
    let stage_dot = StageVisual::for_id(&stage.id).dot;
    let archivable = sessions
        .iter()
        .filter(|session| session.stage != "done")
        .map(StageTarget::from_session)
        .collect::<Vec<_>>();
    rsx! {
        section { class: "glass w-[18rem] shrink-0 overflow-hidden rounded-lg p-1 backdrop-blur-xl",
            header { class: "glass sticky top-0 z-10 flex h-8 items-center gap-2 rounded-md px-1.5 backdrop-blur-xl",
                SheetHeaderChevron {
                    expanded: !collapsed(),
                    label: if collapsed() { translate("common-expand") } else { translate("common-collapse") },
                    onclick: move |_| collapsed.set(!collapsed()),
                }
                span { class: "size-2 rounded-full {stage_dot}" }
                h2 { class: "truncate text-[11px] font-medium text-foreground/85", {translate_humanized(&stage.name)} }
                span { class: "text-[10px] tabular-nums text-muted-foreground/75", "{sessions.len()}" }
                if stage.id == "stale" && !archivable.is_empty() {
                    div { class: "ml-auto",
                        SessionArchiveAllButton { targets: archivable }
                    }
                }
            }
            div { class: if collapsed() {
                    "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
                } else {
                    "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
                },
                div { class: "min-h-0 overflow-hidden",
                    div { class: "mt-1 space-y-1",
                        for session in sessions.into_iter().take(visible) {
                            SessionBoardCard {
                                key: "{session.id}",
                                repository: repositories.get(&session.id),
                                session,
                            }
                        }
                        if visible < total {
                            div {
                                class: "h-1",
                                onvisible: move |event| pager.load_more(event, total),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionBoardCard(session: SessionSummary, repository: Option<SessionRepository>) -> Element {
    let leaving = use_context::<SessionStageMutations>().is_leaving(&session.id);
    let context = SessionContext::from(repository);
    let cleanup_id = session.id.clone();
    let stage_target = StageTarget::from_session(&session);
    let archived = session.stage == "done";
    let runtime = RuntimeVisual::for_id(&session.runtime);
    let git_url = context.git_url.clone();
    let resume = ResumeTarget::from(&session);
    let open_session = resume.clone();
    let agent = AgentPresentation::from_session(&session);
    let title = session.display_title();
    let preview = session.preview();
    let updated = RelativeTime::from_millis(session.last_activated_at).label();
    rsx! {
        Collapse { leaving,
          article {
            class: "glass group relative cursor-pointer rounded-md px-2.5 py-2.5 transition-colors hover:bg-glass-hover",
            onclick: move |_| open_session.open(),
            div { class: "flex items-start gap-2",
                AgentThumbnail {
                    src: agent.icon.clone(),
                    seed: agent.id.clone(),
                    background: agent.color.clone(),
                    alt: agent.name.clone(),
                    class: "size-5 text-[6px]".to_string(),
                }
                div { class: "min-w-0 flex-1",
                    div { class: "truncate text-xs font-medium text-foreground/90", "{title}" }
                    if !agent.name.is_empty() {
                        div { class: "truncate text-[10px] text-muted-foreground/65", "{agent.name}" }
                    }
                }
                SessionActions {
                    resume,
                    git_url: git_url.clone(),
                    cleanup_id,
                    stage_target,
                    archived,
                    managed: session.managed,
                }
            }
            if !preview.is_empty() {
                p { class: "mt-1 max-h-8 overflow-hidden text-[10px] leading-4 text-muted-foreground/65", "{preview}" }
            }
            div { class: "mt-3 flex min-w-0 flex-wrap items-center gap-1.5",
                if runtime.visible {
                    MetadataPill { dot: runtime.dot.to_string(), label: runtime_label(&session.runtime) }
                }
                SessionContextView { context: context.clone() }
            }
            div { class: "mt-3 flex items-center gap-1.5 pt-1 text-[10px] text-muted-foreground/70",
                time { class: "tabular-nums", "{updated}" }
                div { class: "flex-1" }
            }
          }
        }
    }
}

#[component]
pub(super) fn SessionMetaPanel(
    session: SessionSummary,
    repository: Option<SessionRepository>,
    stages: Vec<StageSummary>,
) -> Element {
    let context = SessionContext::from(repository);
    let runtime_dot = RuntimeVisual::for_id(&session.runtime).dot;
    let updated = RelativeTime::from_millis(session.last_activated_at).label();
    let agent = AgentPresentation::from_session(&session);
    let compact_id = SessionIdentity::new(&session.sid).compact();
    rsx! {
        aside { class: "hidden min-h-0 w-[19rem] shrink-0 flex-col bg-foreground/[0.018] lg:flex",
            header { class: "flex h-11 shrink-0 items-center px-4",
                h2 { class: "text-xs font-semibold text-foreground/90", {translate("editor-properties")} }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-4 pb-4",
                if session.managed {
                    EditableDescription {
                        session_id: session.id.clone(),
                        value: session.description.clone(),
                    }
                } else if !session.description.trim().is_empty() {
                    p { class: "mb-4 px-1 text-xs leading-5 text-muted-foreground", "{session.description}" }
                }
                div { class: "grid gap-3",
                    SessionDetailField { label: translate("sessions-stage"),
                        if session.managed {
                            StagePicker {
                                target: StageTarget::from_session(&session),
                                selected: session.stage.clone(),
                                stages,
                            }
                        } else {
                            MetadataPill {
                                dot: StageVisual::for_id(&session.stage).dot.to_string(),
                                label: translate_humanized(&session.stage),
                            }
                        }
                    }
                    if !agent.name.is_empty() {
                        SessionDetailField { label: translate("resume-column-agent"),
                            div { class: "flex min-w-0 items-center gap-1.5",
                                AgentThumbnail {
                                    src: agent.icon.clone(),
                                    seed: agent.id.clone(),
                                    background: agent.color.clone(),
                                    alt: agent.name.clone(),
                                    class: "size-4 shrink-0 text-[5px]".to_string(),
                                }
                                span { class: "truncate text-[10px] text-foreground/75", "{agent.name}" }
                            }
                        }
                    }
                    SessionDetailField { label: translate("common-status"),
                        MetadataPill {
                            dot: runtime_dot.to_string(),
                            label: runtime_label(&session.runtime),
                        }
                    }
                    if !context.project.is_empty() {
                        SessionDetailField { label: translate("resume-column-project"),
                            MetadataPill { label: context.project.clone(), icon: Some(LineIcon::Package) }
                        }
                    }
                    if !context.branch.is_empty() {
                        SessionDetailField { label: translate("resume-column-branch"),
                            MetadataPill { label: context.branch.clone(), icon: Some(LineIcon::GitBranch), mono: true }
                        }
                    }
                    SessionDetailField { label: translate("resume-column-updated"),
                        MetadataPill { label: updated }
                    }
                    SessionDetailField { label: translate("schema-id"),
                        MetadataPill {
                            label: compact_id,
                            title: session.sid.clone(),
                            mono: true,
                        }
                    }
                    if !session.cwd.is_empty() {
                        SessionDetailText {
                            label: translate("sessions-directory"),
                            value: session.cwd.clone(),
                            mono: true,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionDetailField(label: String, children: Element) -> Element {
    rsx! {
        div { class: "grid grid-cols-[5.5rem_minmax(0,1fr)] items-center gap-3",
            span { class: "text-[10px] font-medium text-muted-foreground/70", "{label}" }
            {children}
        }
    }
}

#[component]
fn SessionDetailText(label: String, value: String, #[props(default)] mono: bool) -> Element {
    let class = if mono {
        "break-all font-mono text-[10px] text-foreground/70"
    } else {
        "break-words text-xs text-foreground/75"
    };
    rsx! {
        SessionDetailField { label,
            span { class: "{class}", "{value}" }
        }
    }
}

#[component]
fn EditableDescription(session_id: String, value: String) -> Element {
    let mut editing = use_signal(|| false);
    let mut draft = use_signal(|| value.clone());
    let mut canceled = use_signal(|| false);
    let display = if value.is_empty() {
        translate("sessions-description-placeholder")
    } else {
        value.clone()
    };
    if editing() {
        let current = value.clone();
        return rsx! {
            textarea {
                class: "mb-4 min-h-20 w-full resize-y rounded-lg bg-foreground/[0.035] px-3 py-2.5 text-xs leading-5 text-foreground outline-none ring-1 ring-inset ring-primary/35",
                value: draft(),
                placeholder: translate("sessions-description-placeholder"),
                aria_label: translate("sessions-description"),
                autofocus: true,
                oninput: move |event| draft.set(event.value()),
                onkeydown: move |event| {
                    if event.key() != Key::Escape {
                        return;
                    }
                    event.prevent_default();
                    canceled.set(true);
                    draft.set(current.clone());
                    editing.set(false);
                },
                onblur: move |_| {
                    if canceled() {
                        canceled.set(false);
                        return;
                    }
                    editing.set(false);
                    let description = draft();
                    if description == value {
                        return;
                    }
                    let _ = send(&SessionsDescriptionUpdate {
                        id: session_id.clone(),
                        description,
                    });
                },
            }
        };
    }
    let edit_value = value.clone();
    rsx! {
        button {
            r#type: "button",
            class: if value.is_empty() {
                "mb-4 block w-full rounded-lg px-3 py-2.5 text-left text-xs leading-5 text-muted-foreground/50 hover:bg-foreground/[0.035]"
            } else {
                "mb-4 block w-full whitespace-pre-wrap rounded-lg px-3 py-2.5 text-left text-xs leading-5 text-foreground/65 hover:bg-foreground/[0.035]"
            },
            title: translate("sessions-description"),
            onclick: move |_| {
                canceled.set(false);
                draft.set(edit_value.clone());
                editing.set(true);
            },
            "{display}"
        }
    }
}

#[derive(Clone, PartialEq)]
struct StageTarget {
    id: String,
    kind: String,
    sid: String,
    stage: String,
}

impl StageTarget {
    fn from_session(session: &SessionSummary) -> Self {
        Self {
            id: session.id.clone(),
            kind: session.kind.clone(),
            sid: session.sid.clone(),
            stage: session.stage.clone(),
        }
    }
}

#[derive(Clone, Copy)]
struct SessionStageMutations {
    optimistic: OptimisticMap<String, String>,
    leaving: Signal<HashSet<String>>,
}

impl SessionStageMutations {
    fn change(self, target: StageTarget, stage: String) {
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

    fn archive(mut self, targets: Vec<StageTarget>) {
        let leaving = self.leaving.read();
        let targets = targets
            .into_iter()
            .filter(|target| target.stage != "done" && !leaving.contains(&target.id))
            .collect::<Vec<_>>();
        drop(leaving);
        if targets.is_empty() {
            return;
        }
        self.leaving
            .write()
            .extend(targets.iter().map(|target| target.id.clone()));
        for target in &targets {
            self.optimistic
                .set(target.id.clone(), target.stage.clone(), "done".to_string());
            if send(&SessionsStageChange {
                id: target.id.clone(),
                kind: target.kind.clone(),
                sid: target.sid.clone(),
                stage: "done".to_string(),
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

    fn is_leaving(self, id: &str) -> bool {
        self.leaving.read().contains(id)
    }

    fn cleanup(mut self, id: String) {
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

#[component]
fn SessionActions(
    resume: ResumeTarget,
    git_url: String,
    cleanup_id: String,
    stage_target: StageTarget,
    archived: bool,
    managed: bool,
) -> Element {
    let mut open = use_signal(|| false);
    let archive = stage_target.clone();
    let mutations = use_context::<SessionStageMutations>();
    let visibility = if open() {
        "opacity-100"
    } else {
        "opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
    };
    rsx! {
        Dropdown {
            open: open(),
            class: "flex w-[5.5rem] shrink-0 justify-end gap-0.5".to_string(),
            on_dismiss: move |_| open.set(false),
            button {
                r#type: "button",
                class: "flex size-7 items-center justify-center rounded-md text-muted-foreground transition-all hover:bg-foreground/[0.08] hover:text-foreground {visibility}",
                title: translate("sessions-open-chat"),
                aria_label: translate("sessions-open-chat"),
                onclick: move |event| {
                    event.stop_propagation();
                    resume.open();
                },
                LineIconView { icon: LineIcon::MessagesRound, class: "size-3.5".to_string() }
            }
            if !git_url.is_empty() {
                button {
                    r#type: "button",
                    class: "flex size-7 items-center justify-center rounded-md text-muted-foreground transition-all hover:bg-foreground/[0.08] hover:text-foreground {visibility}",
                    title: translate("git-title"),
                    aria_label: translate("git-title"),
                    onclick: move |event| {
                        event.stop_propagation();
                        let _ = send(&ChatOpenPage { url: git_url.clone() });
                    },
                    GitSymbolView {
                        symbol: GitSymbol::Repository,
                        class: "size-3.5".to_string(),
                    }
                }
            }
            if !archived {
                button {
                    r#type: "button",
                    class: "flex size-7 items-center justify-center rounded-md text-muted-foreground transition-all hover:bg-foreground/[0.08] hover:text-foreground {visibility}",
                    title: translate("sessions-archive"),
                    aria_label: translate("sessions-archive"),
                    onclick: move |event| {
                        event.stop_propagation();
                        mutations.archive(vec![archive.clone()]);
                    },
                    LineIconView { icon: LineIcon::Archive, class: "size-3.5".to_string() }
                }
            }
            if managed {
                button {
                    r#type: "button",
                    class: "flex size-7 items-center justify-center rounded-md text-muted-foreground transition-all hover:bg-foreground/[0.08] hover:text-foreground {visibility}",
                    title: translate("sessions-actions"),
                    aria_label: translate("sessions-actions"),
                    onclick: move |event| {
                        event.stop_propagation();
                        open.set(!open());
                    },
                    LineIconView { icon: LineIcon::Ellipsis, class: "size-3.5".to_string() }
                }
            }
            if managed && open() {
                div {
                    class: "absolute right-0 top-7 z-30 w-36 rounded-md bg-background p-1 shadow-xl ring-1 ring-inset ring-foreground/[0.12]",
                    onclick: move |event| event.stop_propagation(),
                    SessionDeleteButton {
                        onclick: move |_| {
                            open.set(false);
                            mutations.cleanup(cleanup_id.clone());
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn SessionArchiveAllButton(targets: Vec<StageTarget>) -> Element {
    let label = format!("{} {}", translate("sessions-archive"), targets.len());
    let mutations = use_context::<SessionStageMutations>();
    rsx! {
        SheetHeaderAction {
            label,
            onclick: move |event: MouseEvent| {
                event.stop_propagation();
                mutations.archive(targets.clone());
            },
            LineIconView { icon: LineIcon::Archive, class: "size-3.5 shrink-0".to_string() }
        }
    }
}

#[component]
fn SessionDeleteButton(onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11px] text-ansi-1 transition-colors hover:bg-ansi-1/10",
            onclick: move |event| onclick.call(event),
            LineIconView { icon: LineIcon::Trash, class: "size-3.5 shrink-0".to_string() }
            span { {translate("sessions-delete")} }
        }
    }
}

#[component]
fn StagePicker(target: StageTarget, selected: String, stages: Vec<StageSummary>) -> Element {
    let mut open = use_signal(|| false);
    let stage_dot = StageVisual::for_id(&selected).dot;
    let selected_name = stages
        .iter()
        .find(|stage| stage.id == selected)
        .map(|stage| translate_humanized(&stage.name))
        .unwrap_or_else(|| translate_humanized(&selected));
    rsx! {
        Dropdown {
            open: open(),
            class: "inline-flex w-fit max-w-full min-w-0 justify-self-start".to_string(),
            on_dismiss: move |_| open.set(false),
            button {
                r#type: "button",
                class: "inline-flex h-6 max-w-full min-w-0 cursor-pointer items-center gap-1.5 rounded-md bg-foreground/[0.035] py-0.5 pl-2 pr-2 text-[10px] text-foreground/75 outline-none ring-1 ring-inset ring-foreground/[0.08] hover:bg-foreground/[0.06] focus-visible:ring-primary/35",
                title: translate("sessions-stage"),
                aria_expanded: open(),
                onclick: move |_| open.set(!open()),
                span { class: "size-1.5 shrink-0 rounded-full {stage_dot}" }
                span { class: "min-w-0 truncate", "{selected_name}" }
                LineIconView {
                    icon: LineIcon::ChevronDown,
                    class: if open() {
                        "size-2.5 shrink-0 rotate-180 text-muted-foreground".to_string()
                    } else {
                        "size-2.5 shrink-0 text-muted-foreground".to_string()
                    },
                }
            }
            if open() {
                div { class: "absolute left-0 top-7 z-30 min-w-full rounded-md bg-background p-1 shadow-xl ring-1 ring-inset ring-foreground/[0.12]",
                    for stage in stages {
                        StagePickerOption {
                            target: target.clone(),
                            selected_id: selected.clone(),
                            stage,
                            open,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StagePickerOption(
    target: StageTarget,
    selected_id: String,
    stage: StageSummary,
    mut open: Signal<bool>,
) -> Element {
    let selected = stage.id == selected_id;
    let stage_id = stage.id.clone();
    let dot = StageVisual::for_id(&stage.id).dot;
    let name = translate_humanized(&stage.name);
    let mutations = try_consume_context::<SessionStageMutations>();
    rsx! {
        button {
            r#type: "button",
            class: if selected {
                "flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-[10px] text-foreground bg-foreground/[0.07]"
            } else {
                "flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-[10px] text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground"
            },
            onclick: move |_| {
                open.set(false);
                if let Some(mutations) = mutations {
                    mutations.change(target.clone(), stage_id.clone());
                } else {
                    let _ = send(&SessionsStageChange {
                        id: target.id.clone(),
                        kind: target.kind.clone(),
                        sid: target.sid.clone(),
                        stage: stage_id.clone(),
                    });
                }
            },
            span { class: "size-1.5 shrink-0 rounded-full {dot}" }
            span { class: "whitespace-nowrap", "{name}" }
        }
    }
}

#[component]
fn SessionContextView(context: SessionContext) -> Element {
    rsx! {
        div { class: "flex min-w-0 items-center gap-1.5",
            if !context.project.is_empty() {
                MetadataPill { label: context.project, icon: Some(LineIcon::Package) }
            }
            if !context.branch.is_empty() {
                MetadataPill { label: context.branch, icon: Some(LineIcon::GitBranch), mono: true }
            }
        }
    }
}

#[component]
fn MetadataPill(
    label: String,
    #[props(default)] title: String,
    #[props(default)] dot: String,
    #[props(default)] icon: Option<LineIcon>,
    #[props(default)] mono: bool,
) -> Element {
    let label_class = if mono { "font-mono" } else { "" };
    let title = if title.is_empty() {
        label.clone()
    } else {
        title
    };
    rsx! {
        span { class: "inline-flex h-[18px] max-w-full min-w-0 items-center gap-1 rounded-md bg-foreground/[0.045] px-1.5 text-[10px] text-muted-foreground", title,
            if !dot.is_empty() {
                span { class: "size-1.5 shrink-0 rounded-full {dot}" }
            }
            if let Some(icon) = icon {
                LineIconView { icon, class: "size-2.5 shrink-0 opacity-65".to_string() }
            }
            span { class: "max-w-32 truncate {label_class}", "{label}" }
        }
    }
}

#[component]
fn SessionIdPill(label: String, title: String) -> Element {
    rsx! {
        span {
            class: "inline-flex h-[18px] w-[6.75rem] shrink-0 items-center justify-center rounded-md bg-foreground/[0.025] px-1.5 font-mono text-[9px] text-muted-foreground/55",
            title,
            "{label}"
        }
    }
}

struct SessionIdentity<'a>(&'a str);

impl<'a> SessionIdentity<'a> {
    fn new(id: &'a str) -> Self {
        Self(id)
    }

    fn compact(&self) -> String {
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

#[derive(Clone, PartialEq)]
struct ResumeTarget {
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
    fn open(&self) {
        let _ = send(&ResumeSession {
            kind: self.kind.clone(),
            sid: self.sid.clone(),
            cwd: self.cwd.clone(),
        });
    }
}

#[derive(Clone, PartialEq)]
struct SessionContext {
    project: String,
    branch: String,
    git_url: String,
}

#[derive(Clone, PartialEq)]
struct AgentPresentation {
    id: String,
    name: String,
    icon: Option<String>,
    color: String,
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

struct StageVisual {
    dot: &'static str,
}

impl StageVisual {
    fn for_id(id: &str) -> Self {
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

struct RuntimeVisual {
    dot: &'static str,
    visible: bool,
}

struct SessionFreshness;

impl SessionFreshness {
    const STALE_AFTER: u64 = 3 * RelativeTime::DAY;

    fn is_stale(session: &SessionSummary, terminal: bool) -> bool {
        let age = RelativeTime::from_millis(session.last_activated_at);
        Self::from_age(age, &session.runtime, terminal)
    }

    fn from_age(age: RelativeTime, runtime: &str, terminal: bool) -> bool {
        !terminal && !RuntimeVisual::for_id(runtime).visible && age.0 >= Self::STALE_AFTER
    }
}

impl RuntimeVisual {
    fn for_id(id: &str) -> Self {
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

struct RelativeTime(u64);

impl RelativeTime {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * Self::MINUTE;
    const DAY: u64 = 24 * Self::HOUR;
    const WEEK: u64 = 7 * Self::DAY;
    const MONTH: u64 = 30 * Self::DAY;
    const YEAR: u64 = 365 * Self::DAY;

    fn from_millis(timestamp: i64) -> Self {
        if timestamp <= 0 {
            return Self(0);
        }
        let now = vmux_ecs::UnixMillis::now().0;
        Self(now.saturating_sub(timestamp).max(0) as u64 / 1_000)
    }

    fn label(self) -> String {
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

#[derive(Clone, Copy)]
struct SessionPager(Signal<usize>);

impl SessionPager {
    const PAGE: usize = 40;

    fn use_state() -> Self {
        Self(use_signal(|| Self::PAGE))
    }

    fn visible(self, total: usize) -> usize {
        self.0().min(total)
    }

    fn load_more(self, event: Event<VisibleData>, total: usize) {
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

#[derive(Clone, PartialEq)]
struct StageGroup {
    stage: StageSummary,
    sessions: Vec<SessionSummary>,
}

impl StageGroup {
    fn inbox(groups: &[Self]) -> Vec<Self> {
        groups
            .iter()
            .filter(|group| !group.stage.terminal)
            .cloned()
            .collect()
    }

    fn filtered(
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

    fn from_catalog(
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
            if !session.matches(search) {
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
                if let Some(group) = groups.get_mut(stale_index) {
                    group.sessions.push(session.clone());
                }
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

trait SessionSummaryExt {
    fn matches(&self, query: &str) -> bool;
    fn display_title(&self) -> String;
    fn preview(&self) -> String;
}

impl SessionSummaryExt for SessionSummary {
    fn matches(&self, query: &str) -> bool {
        query.is_empty()
            || self.name.to_ascii_lowercase().contains(query)
            || self.description.to_ascii_lowercase().contains(query)
            || self.cwd.to_ascii_lowercase().contains(query)
            || self.agent.to_ascii_lowercase().contains(query)
            || self.agent_name.to_ascii_lowercase().contains(query)
            || self.conversation_title.to_ascii_lowercase().contains(query)
            || self.last_prompt.to_ascii_lowercase().contains(query)
            || self.last_message.to_ascii_lowercase().contains(query)
    }

    fn display_title(&self) -> String {
        if self.conversation_title.is_empty() {
            return self.name.clone();
        }
        self.conversation_title.clone()
    }

    fn preview(&self) -> String {
        if !self.last_prompt.is_empty() && self.last_prompt != self.display_title() {
            return self.last_prompt.clone();
        }
        if !self.last_message.is_empty() && self.last_message != self.display_title() {
            return self.last_message.clone();
        }
        String::new()
    }
}

fn runtime_label(runtime: &str) -> String {
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
    fn session_without_stage_definition_remains_visible() {
        let session = SessionSummary {
            id: "session-1".into(),
            name: "Task".into(),
            stage: "active".into(),
            ..Default::default()
        };

        let groups = StageGroup::from_catalog(&[], std::slice::from_ref(&session), "");

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].stage.id, "active");
        assert_eq!(groups[0].sessions, vec![session]);
    }

    #[test]
    fn session_identity_preserves_both_ends() {
        let id = SessionIdentity::new("01a0fc43-68f9-7121-a388-59e1891fdc41");

        assert_eq!(id.compact(), "01a0fc…dc41");
    }

    #[test]
    fn stale_requires_three_days_without_active_work() {
        let threshold = SessionFreshness::STALE_AFTER;

        assert!(!SessionFreshness::from_age(
            RelativeTime(threshold - 1),
            "idle",
            false,
        ));
        assert!(SessionFreshness::from_age(
            RelativeTime(threshold),
            "idle",
            false,
        ));
        assert!(!SessionFreshness::from_age(
            RelativeTime(threshold),
            "streaming",
            false,
        ));
        assert!(!SessionFreshness::from_age(
            RelativeTime(threshold),
            "idle",
            true,
        ));
    }

    #[test]
    fn stale_session_moves_out_of_its_persisted_stage() {
        let stages = vec![StageSummary {
            id: "in_progress".into(),
            name: "session-stage-active".into(),
            order: 10,
            terminal: false,
        }];
        let session = SessionSummary {
            id: "session-1".into(),
            stage: "in_progress".into(),
            runtime: "idle".into(),
            last_activated_at: vmux_ecs::UnixMillis::now().0
                - (SessionFreshness::STALE_AFTER as i64 * 1_000),
            ..Default::default()
        };

        let groups = StageGroup::from_catalog(&stages, std::slice::from_ref(&session), "");
        let active = groups
            .iter()
            .find(|group| group.stage.id == "in_progress")
            .unwrap();
        let stale = groups
            .iter()
            .find(|group| group.stage.id == "stale")
            .unwrap();

        assert!(active.sessions.is_empty());
        assert_eq!(stale.sessions, vec![session]);
    }

    #[test]
    fn stage_and_updated_filters_compose() {
        let recent = SessionSummary {
            id: "recent".into(),
            stage: "in_progress".into(),
            last_activated_at: 200,
            ..Default::default()
        };
        let old = SessionSummary {
            id: "old".into(),
            stage: "in_progress".into(),
            last_activated_at: 50,
            ..Default::default()
        };
        let done = SessionSummary {
            id: "done".into(),
            stage: "done".into(),
            last_activated_at: 200,
            ..Default::default()
        };
        let groups = vec![
            StageGroup {
                stage: StageSummary {
                    id: "in_progress".into(),
                    ..Default::default()
                },
                sessions: vec![recent.clone(), old],
            },
            StageGroup {
                stage: StageSummary {
                    id: "done".into(),
                    ..Default::default()
                },
                sessions: vec![done],
            },
        ];
        let filters = vec![
            SessionFilter::Stage("in_progress".into()),
            SessionFilter::Updated(UpdatedFilter {
                comparison: UpdatedComparison::OnOrAfter,
                threshold: 100,
                value_label: String::new(),
            }),
        ];

        let filtered = StageGroup::filtered(&groups, &filters, &SessionRepositories::default());

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].sessions, vec![recent]);
    }

    #[test]
    fn agent_and_project_filters_compose() {
        let codex = SessionSummary {
            id: "codex".into(),
            stage: "in_progress".into(),
            agent_name: "Codex".into(),
            ..Default::default()
        };
        let claude = SessionSummary {
            id: "claude".into(),
            stage: "in_progress".into(),
            agent_name: "Claude".into(),
            ..Default::default()
        };
        let groups = vec![StageGroup {
            stage: StageSummary {
                id: "in_progress".into(),
                ..Default::default()
            },
            sessions: vec![codex.clone(), claude],
        }];
        let filters = vec![
            SessionFilter::Agent("Codex".into()),
            SessionFilter::Project("vmux".into()),
        ];
        let repositories = SessionRepositories {
            sessions: vec![SessionRepository {
                id: "codex".into(),
                project: "vmux".into(),
                ..Default::default()
            }],
        };

        let filtered = StageGroup::filtered(&groups, &filters, &repositories);

        assert_eq!(filtered[0].sessions, vec![codex]);
    }

    #[test]
    fn inbox_excludes_terminal_stages() {
        let active = StageGroup {
            stage: StageSummary {
                id: "in_progress".into(),
                terminal: false,
                ..Default::default()
            },
            sessions: vec![SessionSummary {
                id: "active".into(),
                ..Default::default()
            }],
        };
        let completed = StageGroup {
            stage: StageSummary {
                id: "done".into(),
                terminal: true,
                ..Default::default()
            },
            sessions: vec![SessionSummary {
                id: "completed".into(),
                ..Default::default()
            }],
        };

        let inbox = StageGroup::inbox(&[active.clone(), completed]);

        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].stage.id, active.stage.id);
        assert_eq!(inbox[0].sessions, active.sessions);
    }
}
