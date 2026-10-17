use dioxus::prelude::*;
use vmux_session::{Route, SessionSummary, StageSummary};
use vmux_ui::components::avatar::AgentThumbnail;
use vmux_ui::components::manager::{
    ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerPage,
};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_humanized, translate_with};
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
    let mut selected = use_signal(|| None::<String>);
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
    let selected_session = selected().and_then(|id| {
        snapshot
            .sessions
            .iter()
            .find(|session| session.id == id)
            .cloned()
    });

    rsx! {
        ManagerPage {
            header { class: "flex h-11 shrink-0 items-center gap-2.5 border-b border-foreground/[0.07] bg-background px-3",
                div { class: "flex min-w-0 items-baseline gap-2",
                    h1 { class: "truncate text-sm font-semibold tracking-tight", {translate("sessions-title")} }
                    span { class: "text-[11px] tabular-nums text-muted-foreground", "{visible_count}" }
                }
                div { class: "flex-1" }
                div { class: "relative w-44 max-w-[30vw] sm:w-60",
                    LineIconView {
                        icon: LineIcon::Search,
                        class: "pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground".to_string(),
                    }
                    input {
                        r#type: "search",
                        class: "h-7 w-full rounded-md bg-foreground/[0.025] pl-8 pr-3 text-[11px] text-foreground outline-none ring-1 ring-inset ring-foreground/[0.08] placeholder:text-muted-foreground/55 focus:bg-foreground/[0.05] focus:ring-foreground/[0.16]",
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
                    variant: ManagerButtonVariant::Secondary,
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
            div { class: "flex min-h-0 flex-1 overflow-hidden bg-background",
                if snapshot.sessions.is_empty() {
                    ManagerEmpty {
                        title: translate("sessions-empty"),
                        detail: translate("sessions-empty-detail"),
                    }
                } else if view() == SessionsView::List {
                    SessionsList {
                        groups: groups.clone(),
                        repositories: repositories.clone(),
                        selected: selected(),
                        onselect: move |id| selected.set(Some(id)),
                        onedit: move |session: SessionSummary| {
                            editing.set(Some(session.id));
                            name.set(session.name);
                            description.set(session.description);
                            editor_open.set(true);
                        },
                    }
                } else {
                    SessionsBoard {
                        groups: groups.clone(),
                        repositories: repositories.clone(),
                        selected: selected(),
                        onselect: move |id| selected.set(Some(id)),
                        onedit: move |session: SessionSummary| {
                            editing.set(Some(session.id));
                            name.set(session.name);
                            description.set(session.description);
                            editor_open.set(true);
                        },
                    }
                }
                if let Some(session) = selected_session {
                    SessionDetails {
                        repository: repositories.get(&session.id),
                        session,
                        stages: stages.clone(),
                        onclose: move |_| selected.set(None),
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
        div { class: "flex h-7 items-center gap-0.5 rounded-md bg-foreground/[0.025] p-0.5 ring-1 ring-inset ring-foreground/[0.07]",
            button {
                r#type: "button",
                class: if list_selected {
                    "flex h-6 items-center gap-1.5 rounded px-2 text-[11px] font-medium text-foreground bg-foreground/[0.08]"
                } else {
                    "flex h-6 items-center gap-1.5 rounded px-2 text-[11px] font-medium text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground"
                },
                aria_pressed: list_selected,
                onclick: move |_| onselect.call(SessionsView::List),
                BuiltinIconView { icon: vmux_api::BuiltinIcon::ListTodo, class: "size-3.5".to_string() }
                span { class: "hidden sm:inline", {translate("sessions-view-list")} }
            }
            button {
                r#type: "button",
                class: if board_selected {
                    "flex h-6 items-center gap-1.5 rounded px-2 text-[11px] font-medium text-foreground bg-foreground/[0.08]"
                } else {
                    "flex h-6 items-center gap-1.5 rounded px-2 text-[11px] font-medium text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground"
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
    repositories: SessionRepositories,
    selected: Option<String>,
    onselect: EventHandler<String>,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    rsx! {
        div { class: "min-h-0 flex-1 overflow-auto",
            div { class: "w-full min-w-[44rem] max-w-[56rem] space-y-2 p-2",
                for group in groups {
                    if !group.sessions.is_empty() {
                        SessionListGroup {
                            key: "{group.stage.id}",
                            stage: group.stage,
                            sessions: group.sessions,
                            repositories: repositories.clone(),
                            selected: selected.clone(),
                            onselect,
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
    repositories: SessionRepositories,
    selected: Option<String>,
    onselect: EventHandler<String>,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    let stage_dot = StageVisual::for_id(&stage.id).dot;
    rsx! {
        section {
            div { class: "sticky top-0 z-10 flex h-8 items-center gap-2 rounded-md bg-foreground/[0.045] px-2.5 backdrop-blur",
                LineIconView { icon: LineIcon::ChevronDown, class: "size-3 text-muted-foreground/65".to_string() }
                span { class: "size-2 rounded-full {stage_dot}" }
                h2 { class: "text-[11px] font-medium text-foreground/85", {translate_humanized(&stage.name)} }
                span { class: "text-[10px] tabular-nums text-muted-foreground/75", "{sessions.len()}" }
            }
            div { class: "px-1",
                for session in sessions {
                    SessionListRow {
                        key: "{session.id}",
                        repository: repositories.get(&session.id),
                        selected: selected.as_deref() == Some(&session.id),
                        session,
                        onselect,
                        onedit,
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
    selected: bool,
    onselect: EventHandler<String>,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    let context = SessionContext::from(repository);
    let edit_session = session.clone();
    let cleanup_id = session.id.clone();
    let select_id = session.id.clone();
    let runtime = RuntimeVisual::for_id(&session.runtime);
    let agent = AgentPresentation::from_session(&session);
    let title = session.display_title();
    let compact_id = SessionIdentity::new(&session.id).compact();
    let git_url = context.git_url.clone();
    let updated = RelativeTime::from_millis(session.last_activated_at).label();
    let row_class = if selected {
        "bg-foreground/[0.055]"
    } else {
        "hover:bg-foreground/[0.03]"
    };
    rsx! {
        div {
            class: "group relative flex h-9 cursor-pointer items-center gap-2 border-b border-foreground/[0.045] px-2.5 text-xs transition-colors last:border-b-0 {row_class}",
            onclick: move |_| onselect.call(select_id.clone()),
            AgentThumbnail {
                src: agent.icon.clone(),
                seed: agent.id.clone(),
                background: agent.color.clone(),
                alt: agent.name.clone(),
                class: "size-4 shrink-0 text-[5px]".to_string(),
            }
            button {
                r#type: "button",
                class: "min-w-0 flex-1 truncate text-left font-medium text-foreground/90",
                tabindex: "-1",
                "{title}"
            }
            MetadataPill {
                label: compact_id,
                title: session.id.clone(),
                mono: true,
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
                session: edit_session,
                git_url,
                cleanup_id,
                onedit,
            }
        }
    }
}

#[component]
fn SessionsBoard(
    groups: Vec<StageGroup>,
    repositories: SessionRepositories,
    selected: Option<String>,
    onselect: EventHandler<String>,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    rsx! {
        div { class: "flex min-h-0 flex-1 gap-2 overflow-x-auto p-2",
            for group in groups {
                SessionBoardColumn {
                    key: "{group.stage.id}",
                    stage: group.stage,
                    sessions: group.sessions,
                    repositories: repositories.clone(),
                    selected: selected.clone(),
                    onselect,
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
    repositories: SessionRepositories,
    selected: Option<String>,
    onselect: EventHandler<String>,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    let stage_dot = StageVisual::for_id(&stage.id).dot;
    rsx! {
        section { class: "flex min-h-0 w-[18rem] shrink-0 flex-col",
            header { class: "flex h-8 shrink-0 items-center gap-2 rounded-md bg-foreground/[0.045] px-2.5",
                span { class: "size-2 rounded-full {stage_dot}" }
                h2 { class: "truncate text-[11px] font-medium text-foreground/85", {translate_humanized(&stage.name)} }
                span { class: "ml-auto text-[10px] tabular-nums text-muted-foreground/75", "{sessions.len()}" }
            }
            div { class: "mt-1 min-h-0 flex-1 overflow-y-auto",
                for session in sessions {
                    SessionBoardCard {
                        key: "{session.id}",
                        repository: repositories.get(&session.id),
                        selected: selected.as_deref() == Some(session.id.as_str()),
                        session,
                        onselect,
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
    selected: bool,
    onselect: EventHandler<String>,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    let context = SessionContext::from(repository);
    let edit_session = session.clone();
    let cleanup_id = session.id.clone();
    let select_id = session.id.clone();
    let runtime = RuntimeVisual::for_id(&session.runtime);
    let git_url = context.git_url.clone();
    let agent = AgentPresentation::from_session(&session);
    let title = session.display_title();
    let preview = session.preview();
    let updated = RelativeTime::from_millis(session.last_activated_at).label();
    let card_class = if selected {
        "bg-foreground/[0.055]"
    } else {
        "hover:bg-foreground/[0.03]"
    };
    rsx! {
        article {
            class: "group relative cursor-pointer border-b border-foreground/[0.045] px-2.5 py-2.5 transition-colors {card_class}",
            onclick: move |_| onselect.call(select_id.clone()),
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
                    session: edit_session,
                    git_url: git_url.clone(),
                    cleanup_id,
                    onedit,
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
            div { class: "mt-3 flex items-center gap-1.5 border-t border-foreground/[0.06] pt-2 text-[10px] text-muted-foreground/70",
                time { class: "tabular-nums", "{updated}" }
                div { class: "flex-1" }
                if !git_url.is_empty() {
                    LineIconView { icon: LineIcon::GitBranch, class: "size-3".to_string() }
                }
            }
        }
    }
}

#[component]
fn SessionDetails(
    session: SessionSummary,
    repository: Option<SessionRepository>,
    stages: Vec<StageSummary>,
    onclose: EventHandler<MouseEvent>,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    let context = SessionContext::from(repository);
    let open_url = Route::Session(vmux_session::SessionId(session.id.clone())).url();
    let runtime_dot = RuntimeVisual::for_id(&session.runtime).dot;
    let updated = RelativeTime::from_millis(session.last_activated_at).label();
    let title = session.display_title();
    let preview = session.preview();
    let agent = AgentPresentation::from_session(&session);
    let edit_session = session.clone();
    let git_url = context.git_url.clone();
    rsx! {
        aside { class: "flex min-h-0 w-[21rem] shrink-0 flex-col border-l border-foreground/[0.08] bg-background/80",
            header { class: "flex shrink-0 items-start gap-3 border-b border-foreground/[0.08] p-4",
                AgentThumbnail {
                    src: agent.icon.clone(),
                    seed: agent.id.clone(),
                    background: agent.color.clone(),
                    alt: agent.name.clone(),
                    class: "size-8 text-[9px]".to_string(),
                }
                div { class: "min-w-0 flex-1",
                    h2 { class: "break-words text-sm font-semibold text-foreground", "{title}" }
                    div { class: "mt-1 flex items-center gap-1.5 text-[11px] text-muted-foreground",
                        span { class: "size-1.5 rounded-full {runtime_dot}" }
                        span { {runtime_label(&session.runtime)} }
                        if !session.agent.is_empty() {
                            span { class: "truncate", "· {session.agent}" }
                        }
                    }
                }
                button {
                    r#type: "button",
                    class: "flex size-7 shrink-0 items-center justify-center rounded text-lg leading-none text-muted-foreground hover:bg-foreground/[0.07] hover:text-foreground",
                    title: translate("common-close"),
                    aria_label: translate("common-close"),
                    onclick: move |event| onclose.call(event),
                    "×"
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto p-4",
                if !preview.is_empty() {
                    p { class: "mb-4 rounded-lg bg-foreground/[0.025] px-3 py-2.5 text-xs leading-5 text-foreground/70 ring-1 ring-inset ring-foreground/[0.07]", "{preview}" }
                }
                if !session.description.is_empty() {
                    p { class: "mb-4 whitespace-pre-wrap text-xs leading-5 text-foreground/65", "{session.description}" }
                }
                section { class: "rounded-lg bg-foreground/[0.018] ring-1 ring-inset ring-foreground/[0.08]",
                    h3 { class: "border-b border-foreground/[0.07] px-3 py-2 text-[10px] font-medium text-muted-foreground", {translate("editor-properties")} }
                    div { class: "grid gap-3 p-3",
                        SessionDetailField { label: translate("sessions-stage"),
                            StagePicker {
                                session_id: session.id.clone(),
                                selected: session.stage.clone(),
                                stages,
                            }
                        }
                        if !agent.name.is_empty() {
                            SessionDetailField { label: translate("resume-column-agent"),
                                MetadataPill { label: agent.name.clone() }
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
                        if !session.cwd.is_empty() {
                            SessionDetailText { label: translate("sessions-directory"), value: session.cwd.clone(), mono: true }
                        }
                        SessionDetailField { label: translate("resume-column-updated"),
                            MetadataPill { dot: runtime_dot.to_string(), label: updated }
                        }
                    }
                }
            }
            footer { class: "flex shrink-0 items-center gap-2 border-t border-foreground/[0.08] p-3",
                ManagerButton {
                    onclick: move |_| {
                        let _ = send(&ChatOpenPage { url: open_url.clone() });
                    },
                    {translate("sessions-open-chat")}
                }
                ManagerButton {
                    variant: ManagerButtonVariant::Secondary,
                    onclick: move |_| onedit.call(edit_session.clone()),
                    {translate("sessions-edit")}
                }
                if !git_url.is_empty() {
                    ManagerButton {
                        variant: ManagerButtonVariant::Ghost,
                        onclick: move |_| {
                            let _ = send(&ChatOpenPage { url: git_url.clone() });
                        },
                        {translate("git-title")}
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
fn SessionActions(
    session: SessionSummary,
    git_url: String,
    cleanup_id: String,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    let mut open = use_signal(|| false);
    let edit_session = session.clone();
    rsx! {
        div { class: "relative flex justify-end",
            button {
                r#type: "button",
                class: "flex size-7 items-center justify-center rounded text-sm tracking-[1px] text-muted-foreground opacity-0 transition-all hover:bg-foreground/[0.08] hover:text-foreground group-hover:opacity-100 group-focus-within:opacity-100",
                title: translate("sessions-actions"),
                aria_label: translate("sessions-actions"),
                onclick: move |event| {
                    event.stop_propagation();
                    open.set(!open());
                },
                "•••"
            }
            if open() {
                div {
                    class: "absolute right-0 top-7 z-30 w-36 rounded-md bg-background p-1 shadow-xl ring-1 ring-inset ring-foreground/[0.12]",
                    onclick: move |event| event.stop_propagation(),
                    SessionActionItem {
                        label: translate("sessions-edit"),
                        onclick: move |_| {
                            open.set(false);
                            onedit.call(edit_session.clone());
                        },
                    }
                    if !git_url.is_empty() {
                        SessionActionItem {
                            label: translate("git-title"),
                            onclick: move |_| {
                                open.set(false);
                                let _ = send(&ChatOpenPage { url: git_url.clone() });
                            },
                        }
                    }
                    SessionActionItem {
                        label: translate("sessions-cleanup"),
                        danger: true,
                        onclick: move |_| {
                            open.set(false);
                            let _ = send(&SessionsCleanup { id: cleanup_id.clone() });
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn SessionActionItem(
    label: String,
    #[props(default)] danger: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if danger {
        "text-ansi-1 hover:bg-ansi-1/10"
    } else {
        "text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground"
    };
    rsx! {
        button {
            r#type: "button",
            class: "flex w-full items-center rounded px-2 py-1.5 text-left text-[11px] transition-colors {class}",
            onclick: move |event| onclick.call(event),
            "{label}"
        }
    }
}

#[component]
fn StagePicker(session_id: String, selected: String, stages: Vec<StageSummary>) -> Element {
    let stage_dot = StageVisual::for_id(&selected).dot;
    rsx! {
        div { class: "relative inline-flex min-w-0 items-center",
            span { class: "pointer-events-none absolute left-2 size-1.5 rounded-full {stage_dot}" }
            select {
                class: "h-6 max-w-full min-w-0 appearance-none rounded-full bg-foreground/[0.035] py-0.5 pl-5 pr-7 text-[10px] text-foreground/75 outline-none ring-1 ring-inset ring-foreground/[0.08] hover:bg-foreground/[0.06] focus-visible:ring-primary/35",
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
                class: "pointer-events-none absolute right-2 top-1/2 size-2.5 -translate-y-1/2 text-muted-foreground".to_string(),
            }
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
        span { class: "inline-flex h-[18px] max-w-full min-w-0 items-center gap-1 rounded-full bg-foreground/[0.045] px-1.5 text-[10px] text-muted-foreground", title,
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
        let start = characters[..7].iter().collect::<String>();
        let end = characters[characters.len() - 5..]
            .iter()
            .collect::<String>();
        format!("{start}…{end}")
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
            || self.last_message.to_ascii_lowercase().contains(query)
    }

    fn display_title(&self) -> String {
        if self.conversation_title.is_empty() {
            return self.name.clone();
        }
        self.conversation_title.clone()
    }

    fn preview(&self) -> String {
        if !self.last_message.is_empty() && self.last_message != self.display_title() {
            return self.last_message.clone();
        }
        if !self.description.is_empty() {
            return self.description.clone();
        }
        self.cwd.clone()
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

    #[test]
    fn session_identity_preserves_both_ends() {
        let id = SessionIdentity::new("01a0fc43-68f9-7121-a388-59e1891fdc41");

        assert_eq!(id.compact(), "01a0fc4…fdc41");
    }
}
