use dioxus::prelude::*;
use vmux_session::{Route, SessionSummary, StageSummary};
use vmux_ui::components::manager::{
    ManagerBadge, ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList,
    ManagerPage, ManagerRow, ManagerSelect, ManagerSelectItem,
};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::translate;

use crate::event::{
    ChatOpenPage, SessionDirectorySelection, SessionRepositories, SessionRepository,
    SessionsChooseDirectory, SessionsCleanup, SessionsCreate, SessionsDescriptionUpdate,
    SessionsRename, SessionsStageChange,
};
use crate::state::ChatUiState;

#[component]
pub fn SessionsManager() -> Element {
    use_theme();
    let ui = use_ui_state::<ChatUiState>();
    let mut query = use_signal(String::new);
    let mut editing = use_signal(|| None::<String>);
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut cwd = use_signal(String::new);
    let mut editor_open = use_signal(|| false);
    let state = ui.state.read();
    let snapshot = state.sessions.clone();
    let repositories = state.session_repositories.clone();
    let directory_selection = state.session_directory.clone();
    drop(state);
    let search = query().trim().to_ascii_lowercase();
    let visible_count = snapshot
        .sessions
        .iter()
        .filter(|session| session.matches(&search))
        .count();

    rsx! {
        ManagerPage {
            ManagerHeader {
                title: translate("sessions-title"),
                count: visible_count,
                search_value: query(),
                search_placeholder: translate("sessions-search"),
                onsearch: move |event: FormEvent| query.set(event.value()),
                onkeydown: None,
                actions: rsx! {
                    ManagerButton {
                        onclick: move |_| {
                            editing.set(None);
                            name.set(String::new());
                            description.set(String::new());
                            cwd.set(String::new());
                            editor_open.set(true);
                        },
                        {translate("sessions-new")}
                    }
                },
            }
            ManagerList {
                if editor_open() {
                    Editor {
                        id: editing(),
                        name,
                        description,
                        cwd,
                        directory_selection,
                        onclose: move |_| editor_open.set(false),
                    }
                }
                if snapshot.sessions.is_empty() {
                    ManagerEmpty {
                        title: translate("sessions-empty"),
                        detail: translate("sessions-empty-detail"),
                    }
                } else {
                    for stage in snapshot.stages.clone() {
                        StageSection {
                            key: "{stage.id}",
                            stage: stage.clone(),
                            stages: snapshot.stages.clone(),
                            repositories: repositories.clone(),
                            sessions: snapshot
                                .sessions
                                .iter()
                                .filter(|session| session.stage == stage.id && session.matches(&search))
                                .cloned()
                                .collect(),
                            onedit: move |session: SessionSummary| {
                                editing.set(Some(session.id));
                                name.set(session.name);
                                description.set(session.description);
                                cwd.set(session.cwd);
                                editor_open.set(true);
                            },
                        }
                    }
                }
            }
        }
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
    id: Option<String>,
    name: Signal<String>,
    description: Signal<String>,
    cwd: Signal<String>,
    directory_selection: Option<SessionDirectorySelection>,
    onclose: EventHandler<MouseEvent>,
) -> Element {
    let editing = id.is_some();
    let initial_revision = directory_selection
        .as_ref()
        .map(|selection| selection.revision)
        .unwrap_or_default();
    let mut handled_revision = use_signal(|| initial_revision);
    use_effect(move || {
        let Some(selection) = directory_selection.clone() else {
            return;
        };
        if selection.revision <= handled_revision() {
            return;
        }
        handled_revision.set(selection.revision);
        cwd.set(selection.path.clone());
        if name().trim().is_empty()
            && let Some(folder) = std::path::Path::new(&selection.path).file_name()
        {
            name.set(folder.to_string_lossy().into_owned());
        }
    });
    rsx! {
        section { class: "rounded-2xl bg-foreground/[0.035] p-5 ring-1 ring-inset ring-foreground/10",
            div { class: "grid gap-3",
                label { class: "grid gap-1 text-xs font-medium text-muted-foreground",
                    {translate("sessions-name")}
                    input {
                        class: "rounded-xl bg-background/70 px-3 py-2 text-sm text-foreground outline-none ring-1 ring-inset ring-foreground/10 focus:ring-primary/30",
                        value: name(),
                        placeholder: translate("sessions-name-placeholder"),
                        oninput: move |event| name.set(event.value()),
                    }
                }
                label { class: "grid gap-1 text-xs font-medium text-muted-foreground",
                    {translate("sessions-description")}
                    textarea {
                        class: "min-h-20 resize-y rounded-xl bg-background/70 px-3 py-2 text-sm text-foreground outline-none ring-1 ring-inset ring-foreground/10 focus:ring-primary/30",
                        value: description(),
                        placeholder: translate("sessions-description-placeholder"),
                        oninput: move |event| description.set(event.value()),
                    }
                }
                label { class: "grid gap-1 text-xs font-medium text-muted-foreground",
                    {translate("sessions-directory")}
                    div { class: "flex items-center gap-2",
                        span {
                            class: "min-w-0 flex-1 truncate rounded-xl bg-background/70 px-3 py-2 text-sm font-normal text-foreground ring-1 ring-inset ring-foreground/10",
                            if cwd().is_empty() {
                                span { class: "text-muted-foreground/60", {translate("sessions-directory-placeholder")} }
                            } else {
                                "{cwd()}"
                            }
                        }
                        if !editing {
                            ManagerButton {
                                variant: ManagerButtonVariant::Secondary,
                                onclick: move |_| {
                                    let _ = send(&SessionsChooseDirectory {
                                        current_dir: cwd(),
                                    });
                                },
                                {translate("sessions-directory-choose")}
                            }
                        }
                    }
                }
                div { class: "flex justify-end gap-2",
                    ManagerButton {
                        variant: ManagerButtonVariant::Ghost,
                        onclick: move |event| onclose.call(event),
                        {translate("common-cancel")}
                    }
                    ManagerButton {
                        disabled: name().trim().is_empty() || (!editing && cwd().is_empty()),
                        onclick: move |event| {
                            let value = name().trim().to_string();
                            if value.is_empty() {
                                return;
                            }
                            if let Some(id) = id.clone() {
                                let _ = send(&SessionsRename {
                                    id: id.clone(),
                                    name: value,
                                });
                                let _ = send(&SessionsDescriptionUpdate {
                                    id,
                                    description: description(),
                                });
                            } else {
                                let _ = send(&SessionsCreate {
                                    name: value,
                                    description: description(),
                                    cwd: cwd(),
                                });
                            }
                            onclose.call(event);
                        },
                        {if editing { translate("common-save") } else { translate("sessions-create") }}
                    }
                }
            }
        }
    }
}

#[component]
fn StageSection(
    stage: StageSummary,
    stages: Vec<StageSummary>,
    repositories: SessionRepositories,
    sessions: Vec<SessionSummary>,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    if sessions.is_empty() {
        return rsx! {};
    }
    rsx! {
        section { class: "grid gap-2",
            div { class: "flex items-center gap-2 px-1 pt-2",
                h2 { class: "text-xs font-semibold uppercase tracking-wide text-muted-foreground",
                    {translate(&stage.name)}
                }
                span { class: "text-xs tabular-nums text-muted-foreground/60", "{sessions.len()}" }
            }
            for session in sessions {
                SessionRowView {
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
fn SessionRowView(
    session: SessionSummary,
    repository: Option<SessionRepository>,
    stages: Vec<StageSummary>,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    let stage_id = session.id.clone();
    let open_id = session.id.clone();
    let cleanup_id = session.id.clone();
    let edit_session = session.clone();
    let project = repository
        .as_ref()
        .map(|repository| repository.project.clone())
        .unwrap_or_default();
    let branch = repository
        .as_ref()
        .map(|repository| repository.branch.clone())
        .unwrap_or_default();
    let git_url = repository
        .map(|repository| repository.url)
        .unwrap_or_default();
    let open_git_url = git_url.clone();
    let stage_items = stages
        .iter()
        .map(|stage| ManagerSelectItem {
            value: stage.id.clone(),
            label: translate(&stage.name),
        })
        .collect();
    let subtitle = session.subtitle();
    rsx! {
        ManagerRow {
            show_icon: false,
            icon: rsx! {},
            title: session.name.clone(),
            subtitle,
            meta: rsx! {
                ManagerBadge { {runtime_label(&session.runtime)} }
                if !project.is_empty() {
                    ManagerBadge { {project} }
                }
                if !branch.is_empty() {
                    ManagerBadge { {branch} }
                }
            },
            actions: rsx! {
                div { class: "w-36",
                    ManagerSelect {
                        items: stage_items,
                        value: Some(session.stage.clone()),
                        placeholder: translate("sessions-stage"),
                        onselect: move |stage| {
                            let _ = send(&SessionsStageChange {
                                id: stage_id.clone(),
                                stage,
                            });
                        },
                    }
                }
                if !git_url.is_empty() {
                    ManagerButton {
                        variant: ManagerButtonVariant::Ghost,
                        onclick: move |_| {
                            let _ = send(&ChatOpenPage {
                                url: open_git_url.clone(),
                            });
                        },
                        {translate("git-title")}
                    }
                }
                ManagerButton {
                    variant: ManagerButtonVariant::Ghost,
                    onclick: move |_| onedit.call(edit_session.clone()),
                    {translate("sessions-edit")}
                }
                ManagerButton {
                    variant: ManagerButtonVariant::Secondary,
                    onclick: move |_| {
                        let _ = send(&ChatOpenPage {
                            url: Route::Session(vmux_session::SessionId(open_id.clone())).url(),
                        });
                    },
                    {translate("sessions-open")}
                }
                ManagerButton {
                    variant: ManagerButtonVariant::Ghost,
                    onclick: move |_| {
                        let _ = send(&SessionsCleanup {
                            id: cleanup_id.clone(),
                        });
                    },
                    {translate("sessions-cleanup")}
                }
            },
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
