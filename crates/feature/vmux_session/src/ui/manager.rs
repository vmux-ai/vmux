use dioxus::prelude::*;
use vmux_session::{Route, SessionSummary, StageSummary};
use vmux_ui::components::manager::{
    ManagerBadge, ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList,
    ManagerPage, ManagerRow, ManagerSelect, ManagerSelectItem,
};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::{translate, translate_humanized};

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
                            let _ = send(&ChatOpenPage {
                                url: "vmux://start/".to_string(),
                            });
                        },
                        {translate("sessions-new")}
                    }
                },
            }
            ManagerList {
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
                } else {
                    for group in groups {
                        StageSection {
                            key: "{group.stage.id}",
                            stage: group.stage,
                            stages: stages.clone(),
                            repositories: repositories.clone(),
                            sessions: group.sessions,
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
}

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
                div { class: "flex justify-end gap-2",
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
                    {translate_humanized(&stage.name)}
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
            label: translate_humanized(&stage.name),
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
