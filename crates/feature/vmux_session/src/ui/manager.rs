use dioxus::prelude::*;
use vmux_session::{Route, SessionSummary, StageSummary};
use vmux_ui::components::manager::{
    ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerPage,
};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::{translate, translate_humanized};
use vmux_ui::icon::{BuiltinIconView, LineIcon, LineIconView};

use crate::event::{
    ChatOpenPage, SessionRepositories, SessionRepository, SessionsCleanup,
    SessionsDescriptionUpdate, SessionsRename, SessionsStageChange,
};
use crate::state::ChatUiState;

#[component]
pub fn SessionsManager() -> Element {
    use_theme();
    let ui = use_ui_state::<ChatUiState>();
    let mut query = use_signal(String::new);
    let mut view = use_signal(|| SessionsView::List);
    let mut editing = use_signal(|| None::<String>);
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut editor_open = use_signal(|| false);
    let state = ui.state.read();
    let snapshot = state.sessions.clone();
    let repositories = state.session_repositories.clone();
    drop(state);
    let search = query().trim().to_ascii_lowercase();
    let visible_count = snapshot
        .sessions
        .iter()
        .filter(|session| session.matches(&search))
        .count();
    let groups = StageGroup::from_catalog(&snapshot.stages, &snapshot.sessions, &search);
    let stages = groups
        .iter()
        .map(|group| group.stage.clone())
        .collect::<Vec<_>>();

    rsx! {
        ManagerPage {
            header { class: "flex shrink-0 items-center gap-3 border-b border-foreground/[0.08] px-4 py-2.5",
                div { class: "flex min-w-0 items-baseline gap-2",
                    h1 { class: "truncate text-sm font-semibold tracking-tight", {translate("sessions-title")} }
                    span { class: "text-[11px] tabular-nums text-muted-foreground", "{visible_count}" }
                }
                div { class: "flex-1" }
                div { class: "relative w-48 max-w-[30vw] sm:w-64",
                    LineIconView {
                        icon: LineIcon::Search,
                        class: "pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground".to_string(),
                    }
                    input {
                        r#type: "search",
                        class: "h-8 w-full rounded-md bg-foreground/[0.035] pl-8 pr-3 text-xs text-foreground outline-none ring-1 ring-inset ring-foreground/[0.08] placeholder:text-muted-foreground/55 focus:bg-foreground/[0.055] focus:ring-primary/30",
                        placeholder: translate("sessions-search"),
                        value: query(),
                        oninput: move |event| query.set(event.value()),
                    }
                }
                SessionsViewToggle {
                    selected: view(),
                    onselect: move |selected| view.set(selected),
                }
                ManagerButton {
                    onclick: move |_| {
                        let _ = send(&ChatOpenPage {
                            url: "vmux://start/".to_string(),
                        });
                    },
                    {translate("sessions-new")}
                }
            }
            if editor_open() && let Some(id) = editing() {
                Editor {
                    id,
                    name,
                    description,
                    onclose: move |_| editor_open.set(false),
                }
            }
            if snapshot.sessions.is_empty() {
                ManagerEmpty {
                    title: translate("sessions-empty"),
                    detail: translate("sessions-empty-detail"),
                }
            } else if view() == SessionsView::List {
                SessionsList {
                    groups,
                    stages,
                    repositories,
                    onedit: move |session: SessionSummary| {
                        editing.set(Some(session.id));
                        name.set(session.name);
                        description.set(session.description);
                        editor_open.set(true);
                    },
                }
            } else {
                SessionsBoard {
                    groups,
                    stages,
                    repositories,
                    onedit: move |session: SessionSummary| {
                        editing.set(Some(session.id));
                        name.set(session.name);
                        description.set(session.description);
                        editor_open.set(true);
                    },
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SessionsView {
    List,
    Board,
}

#[component]
fn SessionsViewToggle(selected: SessionsView, onselect: EventHandler<SessionsView>) -> Element {
    let list_selected = selected == SessionsView::List;
    let board_selected = selected == SessionsView::Board;
    rsx! {
        div { class: "flex h-8 items-center rounded-md bg-foreground/[0.04] p-0.5 ring-1 ring-inset ring-foreground/[0.08]",
            button {
                r#type: "button",
                class: if list_selected {
                    "flex h-7 items-center gap-1.5 rounded px-2 text-[11px] font-medium text-foreground shadow-sm bg-background ring-1 ring-inset ring-foreground/[0.08]"
                } else {
                    "flex h-7 items-center gap-1.5 rounded px-2 text-[11px] font-medium text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground"
                },
                aria_pressed: list_selected,
                onclick: move |_| onselect.call(SessionsView::List),
                BuiltinIconView { icon: vmux_api::BuiltinIcon::ListTodo, class: "size-3.5".to_string() }
                span { class: "hidden sm:inline", {translate("sessions-view-list")} }
            }
            button {
                r#type: "button",
                class: if board_selected {
                    "flex h-7 items-center gap-1.5 rounded px-2 text-[11px] font-medium text-foreground shadow-sm bg-background ring-1 ring-inset ring-foreground/[0.08]"
                } else {
                    "flex h-7 items-center gap-1.5 rounded px-2 text-[11px] font-medium text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground"
                },
                aria_pressed: board_selected,
                onclick: move |_| onselect.call(SessionsView::Board),
                LineIconView { icon: LineIcon::Layout, class: "size-3.5".to_string() }
                span { class: "hidden sm:inline", {translate("sessions-view-board")} }
            }
        }
    }
}

#[component]
fn SessionsList(
    groups: Vec<StageGroup>,
    stages: Vec<StageSummary>,
    repositories: SessionRepositories,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    rsx! {
        div { class: "min-h-0 flex-1 overflow-auto p-4",
            div { class: "min-w-[46rem] overflow-hidden rounded-lg bg-foreground/[0.018] ring-1 ring-inset ring-foreground/[0.08]",
                for group in groups {
                    if !group.sessions.is_empty() {
                        SessionListGroup {
                            key: "{group.stage.id}",
                            stage: group.stage,
                            sessions: group.sessions,
                            stages: stages.clone(),
                            repositories: repositories.clone(),
                            onedit,
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
    stages: Vec<StageSummary>,
    repositories: SessionRepositories,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    let stage_dot = StageVisual::for_id(&stage.id).dot;
    rsx! {
        section { class: "border-b border-foreground/[0.07] last:border-b-0",
            div { class: "flex h-8 items-center gap-2 bg-foreground/[0.035] px-3",
                span { class: "size-2 rounded-full {stage_dot}" }
                h2 { class: "text-[11px] font-medium text-foreground/80", {translate_humanized(&stage.name)} }
                span { class: "text-[10px] tabular-nums text-muted-foreground", "{sessions.len()}" }
            }
            for session in sessions {
                SessionListRow {
                    key: "{session.id}",
                    repository: repositories.get(&session.id),
                    session,
                    stages: stages.clone(),
                    onedit,
                }
            }
        }
    }
}

#[component]
fn SessionListRow(
    session: SessionSummary,
    repository: Option<SessionRepository>,
    stages: Vec<StageSummary>,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    let context = SessionContext::from(repository);
    let open_url = Route::Session(vmux_session::SessionId(session.id.clone())).url();
    let edit_session = session.clone();
    let cleanup_id = session.id.clone();
    let runtime_dot = RuntimeVisual::for_id(&session.runtime).dot;
    let agent = session.agent.clone();
    let subtitle = session.subtitle();
    let git_url = context.git_url.clone();
    rsx! {
        div { class: "group grid min-h-11 grid-cols-[minmax(15rem,1fr)_7.5rem_minmax(8rem,0.7fr)_8rem_5.5rem] items-center gap-3 border-t border-foreground/[0.055] px-3 text-xs transition-colors first:border-t-0 hover:bg-foreground/[0.04]",
            button {
                r#type: "button",
                class: "flex min-w-0 flex-col items-start py-2 text-left",
                title: translate("sessions-open"),
                onclick: move |_| {
                    let _ = send(&ChatOpenPage { url: open_url.clone() });
                },
                span { class: "w-full truncate font-medium text-foreground/90", "{session.name}" }
                if !subtitle.is_empty() {
                    span { class: "w-full truncate text-[10px] text-muted-foreground/65", "{subtitle}" }
                }
            }
            div { class: "min-w-0",
                if !agent.is_empty() {
                    div { class: "truncate text-[10px] text-foreground/70", "{agent}" }
                }
                div { class: "flex items-center gap-1.5 text-[10px] text-muted-foreground",
                    span { class: "size-1.5 rounded-full {runtime_dot}" }
                    span { class: "truncate", {runtime_label(&session.runtime)} }
                }
            }
            SessionContextView { context: context.clone() }
            StagePicker { session_id: session.id.clone(), selected: session.stage.clone(), stages }
            div { class: "flex items-center justify-end opacity-55 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100",
                if !git_url.is_empty() {
                    SessionIconButton {
                        icon: LineIcon::GitBranch,
                        label: translate("git-title"),
                        onclick: move |_| {
                            let _ = send(&ChatOpenPage { url: git_url.clone() });
                        },
                    }
                }
                SessionIconButton {
                    icon: LineIcon::Pencil,
                    label: translate("sessions-edit"),
                    onclick: move |_| onedit.call(edit_session.clone()),
                }
                SessionIconButton {
                    icon: LineIcon::Trash,
                    label: translate("sessions-cleanup"),
                    onclick: move |_| {
                        let _ = send(&SessionsCleanup { id: cleanup_id.clone() });
                    },
                }
            }
        }
    }
}

#[component]
fn SessionsBoard(
    groups: Vec<StageGroup>,
    stages: Vec<StageSummary>,
    repositories: SessionRepositories,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    rsx! {
        div { class: "flex min-h-0 flex-1 gap-3 overflow-x-auto p-4",
            for group in groups {
                SessionBoardColumn {
                    key: "{group.stage.id}",
                    stage: group.stage,
                    sessions: group.sessions,
                    stages: stages.clone(),
                    repositories: repositories.clone(),
                    onedit,
                }
            }
        }
    }
}

#[component]
fn SessionBoardColumn(
    stage: StageSummary,
    sessions: Vec<SessionSummary>,
    stages: Vec<StageSummary>,
    repositories: SessionRepositories,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    let stage_dot = StageVisual::for_id(&stage.id).dot;
    rsx! {
        section { class: "flex min-h-0 w-[19rem] shrink-0 flex-col rounded-lg bg-foreground/[0.025] ring-1 ring-inset ring-foreground/[0.08]",
            header { class: "flex h-10 shrink-0 items-center gap-2 border-b border-foreground/[0.07] px-3",
                span { class: "size-2 rounded-full {stage_dot}" }
                h2 { class: "truncate text-xs font-medium text-foreground/85", {translate_humanized(&stage.name)} }
                span { class: "ml-auto text-[10px] tabular-nums text-muted-foreground", "{sessions.len()}" }
            }
            div { class: "min-h-0 flex-1 space-y-2 overflow-y-auto p-2",
                for session in sessions {
                    SessionBoardCard {
                        key: "{session.id}",
                        repository: repositories.get(&session.id),
                        session,
                        stages: stages.clone(),
                        onedit,
                    }
                }
            }
        }
    }
}

#[component]
fn SessionBoardCard(
    session: SessionSummary,
    repository: Option<SessionRepository>,
    stages: Vec<StageSummary>,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    let context = SessionContext::from(repository);
    let open_url = Route::Session(vmux_session::SessionId(session.id.clone())).url();
    let edit_session = session.clone();
    let cleanup_id = session.id.clone();
    let runtime_dot = RuntimeVisual::for_id(&session.runtime).dot;
    let git_url = context.git_url.clone();
    rsx! {
        article { class: "group rounded-md bg-background/55 p-3 ring-1 ring-inset ring-foreground/[0.08] transition-colors hover:bg-background/80 hover:ring-foreground/[0.14]",
            div { class: "flex items-start gap-2",
                button {
                    r#type: "button",
                    class: "min-w-0 flex-1 truncate text-left text-xs font-medium text-foreground/90",
                    title: translate("sessions-open"),
                    onclick: move |_| {
                        let _ = send(&ChatOpenPage { url: open_url.clone() });
                    },
                    "{session.name}"
                }
                div { class: "flex -mr-1 -mt-1 opacity-60 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100",
                    SessionIconButton {
                        icon: LineIcon::Pencil,
                        label: translate("sessions-edit"),
                        onclick: move |_| onedit.call(edit_session.clone()),
                    }
                    SessionIconButton {
                        icon: LineIcon::Trash,
                        label: translate("sessions-cleanup"),
                        onclick: move |_| {
                            let _ = send(&SessionsCleanup { id: cleanup_id.clone() });
                        },
                    }
                }
            }
            if !session.description.is_empty() {
                p { class: "mt-1 max-h-8 overflow-hidden text-[10px] leading-4 text-muted-foreground/70", "{session.description}" }
            }
            div { class: "mt-3 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[10px] text-muted-foreground",
                span { class: "flex items-center gap-1",
                    span { class: "size-1.5 rounded-full {runtime_dot}" }
                    {runtime_label(&session.runtime)}
                }
                if !session.agent.is_empty() {
                    span { class: "max-w-28 truncate", "{session.agent}" }
                }
                if !context.project.is_empty() {
                    span { class: "max-w-32 truncate", "{context.project}" }
                }
                if !context.branch.is_empty() {
                    span { class: "max-w-32 truncate font-mono", "{context.branch}" }
                }
            }
            div { class: "mt-3 flex items-center gap-1.5 border-t border-foreground/[0.06] pt-2",
                div { class: "min-w-0 flex-1",
                    StagePicker { session_id: session.id, selected: session.stage, stages }
                }
                if !git_url.is_empty() {
                    SessionIconButton {
                        icon: LineIcon::GitBranch,
                        label: translate("git-title"),
                        onclick: move |_| {
                            let _ = send(&ChatOpenPage { url: git_url.clone() });
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn StagePicker(session_id: String, selected: String, stages: Vec<StageSummary>) -> Element {
    rsx! {
        div { class: "relative min-w-0",
            select {
                class: "h-7 w-full min-w-0 appearance-none rounded-md bg-foreground/[0.035] py-1 pl-2 pr-7 text-[10px] text-foreground/80 outline-none ring-1 ring-inset ring-foreground/[0.08] hover:bg-foreground/[0.06] focus-visible:ring-primary/35",
                value: selected,
                title: translate("sessions-stage"),
                onchange: move |event| {
                    let _ = send(&SessionsStageChange {
                        id: session_id.clone(),
                        stage: event.value(),
                    });
                },
                for stage in stages {
                    option { value: stage.id, {translate_humanized(&stage.name)} }
                }
            }
            LineIconView {
                icon: LineIcon::ChevronDown,
                class: "pointer-events-none absolute right-2 top-1/2 size-3 -translate-y-1/2 text-muted-foreground".to_string(),
            }
        }
    }
}

#[component]
fn SessionIconButton(icon: LineIcon, label: String, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "flex size-7 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-foreground/[0.07] hover:text-foreground",
            title: label.clone(),
            aria_label: label,
            onclick: move |event| onclick.call(event),
            LineIconView { icon, class: "size-3.5".to_string() }
        }
    }
}

#[component]
fn SessionContextView(context: SessionContext) -> Element {
    rsx! {
        div { class: "flex min-w-0 items-center gap-2 text-[10px] text-muted-foreground",
            if !context.project.is_empty() {
                span { class: "max-w-32 truncate", "{context.project}" }
            }
            if !context.branch.is_empty() {
                span { class: "max-w-32 truncate font-mono text-muted-foreground/70", "{context.branch}" }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct SessionContext {
    project: String,
    branch: String,
    git_url: String,
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
        Self { dot }
    }
}

#[derive(Clone, PartialEq)]
struct StageGroup {
    stage: StageSummary,
    sessions: Vec<SessionSummary>,
}

impl StageGroup {
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
    fn subtitle(&self) -> String;
}

impl SessionSummaryExt for SessionSummary {
    fn matches(&self, query: &str) -> bool {
        query.is_empty()
            || self.name.to_ascii_lowercase().contains(query)
            || self.description.to_ascii_lowercase().contains(query)
            || self.cwd.to_ascii_lowercase().contains(query)
            || self.agent.to_ascii_lowercase().contains(query)
    }

    fn subtitle(&self) -> String {
        if self.description.is_empty() {
            return self.cwd.clone();
        }
        if self.cwd.is_empty() {
            return self.description.clone();
        }
        format!("{} · {}", self.description, self.cwd)
    }
}

#[component]
fn Editor(
    id: String,
    name: Signal<String>,
    description: Signal<String>,
    onclose: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "fixed inset-0 z-50 flex items-start justify-center bg-background/65 px-4 pt-[12vh] backdrop-blur-sm",
            section { class: "w-full max-w-lg rounded-xl bg-background p-5 shadow-2xl ring-1 ring-inset ring-foreground/12",
                div { class: "grid gap-3",
                    label { class: "grid gap-1 text-xs font-medium text-muted-foreground",
                        {translate("sessions-name")}
                        input {
                            class: "rounded-lg bg-foreground/[0.035] px-3 py-2 text-sm text-foreground outline-none ring-1 ring-inset ring-foreground/10 focus:ring-primary/30",
                            value: name(),
                            placeholder: translate("sessions-name-placeholder"),
                            oninput: move |event| name.set(event.value()),
                        }
                    }
                    label { class: "grid gap-1 text-xs font-medium text-muted-foreground",
                        {translate("sessions-description")}
                        textarea {
                            class: "min-h-20 resize-y rounded-lg bg-foreground/[0.035] px-3 py-2 text-sm text-foreground outline-none ring-1 ring-inset ring-foreground/10 focus:ring-primary/30",
                            value: description(),
                            placeholder: translate("sessions-description-placeholder"),
                            oninput: move |event| description.set(event.value()),
                        }
                    }
                    div { class: "flex justify-end gap-2 pt-1",
                        ManagerButton {
                            variant: ManagerButtonVariant::Ghost,
                            onclick: move |event| onclose.call(event),
                            {translate("common-cancel")}
                        }
                        ManagerButton {
                            disabled: name().trim().is_empty(),
                            onclick: move |event| {
                                let value = name().trim().to_string();
                                if value.is_empty() {
                                    return;
                                }
                                let _ = send(&SessionsRename {
                                    id: id.clone(),
                                    name: value,
                                });
                                let _ = send(&SessionsDescriptionUpdate {
                                    id: id.clone(),
                                    description: description(),
                                });
                                onclose.call(event);
                            },
                            {translate("common-save")}
                        }
                    }
                }
            }
        }
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
}
