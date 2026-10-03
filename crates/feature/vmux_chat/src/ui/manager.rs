use dioxus::prelude::*;
use vmux_session::{CatalogSnapshot, Route, SessionSummary, StageSummary};
use vmux_ui::components::manager::{
    ManagerBadge, ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList,
    ManagerPage, ManagerRow, ManagerSelect, ManagerSelectItem, ManagerSelectItemKind,
};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::translate;

use crate::event::{
    ChatOpenPage, SessionsCleanup, SessionsCreate, SessionsDescriptionUpdate, SessionsRename,
    SessionsStageChange,
};
use crate::state::ChatUiState;

#[component]
pub fn SessionsManager() -> Element {
    use_theme();
    let ui = use_ui_state::<ChatUiState>();
    let catalog = ui.use_value::<CatalogSnapshot>().value;
    let mut query = use_signal(String::new);
    let mut editing = use_signal(|| None::<String>);
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut editor_open = use_signal(|| false);
    let snapshot = catalog();
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
}

impl SessionSummaryExt for SessionSummary {
    fn matches(&self, query: &str) -> bool {
        query.is_empty()
            || self.name.to_ascii_lowercase().contains(query)
            || self.description.to_ascii_lowercase().contains(query)
            || self.cwd.to_ascii_lowercase().contains(query)
            || self.agent.to_ascii_lowercase().contains(query)
    }
}

#[component]
fn Editor(
    id: Option<String>,
    name: Signal<String>,
    description: Signal<String>,
    onclose: EventHandler<MouseEvent>,
) -> Element {
    let editing = id.is_some();
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
                                    id: uuid::Uuid::new_v4().to_string(),
                                    name: value,
                                    description: description(),
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
    stages: Vec<StageSummary>,
    onedit: EventHandler<SessionSummary>,
) -> Element {
    let stage_id = session.id.clone();
    let open_id = session.id.clone();
    let cleanup_id = session.id.clone();
    let edit_session = session.clone();
    let stage_items = stages
        .iter()
        .map(|stage| ManagerSelectItem {
            value: stage.id.clone(),
            label: translate(&stage.name),
            kind: ManagerSelectItemKind::Default,
        })
        .collect();
    let subtitle = if session.description.is_empty() {
        session.cwd.clone()
    } else {
        session.description.clone()
    };
    rsx! {
        ManagerRow {
            show_icon: false,
            icon: rsx! {},
            title: session.name.clone(),
            subtitle,
            meta: rsx! {
                ManagerBadge { {runtime_label(&session.runtime)} }
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
