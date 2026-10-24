#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::team::{
    ProfileRow, TeamAgentPresentation, TeamAgentSubtitle, TeamMemberRow, TeamProfileCreateRequest,
    TeamProfileForm, TeamProfileFormCloseRequest, TeamProfileFormInputRequest,
    TeamProfileFormOpenRequest, TeamProfileSwitchRequest, TeamProfileUpdateRequest, UiTeam,
};
use vmux_ui::components::avatar::{AgentThumbnail, Avatar};
use vmux_ui::components::badge::Badge;
use vmux_ui::components::dropdown::{DropdownAlign, DropdownOption, DropdownSelector};
use vmux_ui::components::manager::{WorkspaceBreadcrumbHeader, WorkspacePage};
use vmux_ui::favicon::FaviconSource;
use vmux_ui::hooks::{UiState, send, use_theme};
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

#[vmux_page::page(
    component = Page,
    states = [vmux_ui::theme::UiTheme, vmux_api::team::UiTeam]
)]
pub(crate) struct TeamPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let team = UiState::<UiTeam>::use_context().state;

    let UiTeam {
        profiles,
        active_profile,
        agents,
        profile_form,
        ..
    } = team();

    rsx! {
        div {
            class: "flex h-full min-h-0 flex-col bg-background text-foreground",
            WorkspaceBreadcrumbHeader {
                current: WorkspacePage::Team.option(),
                options: WorkspacePage::options(),
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-5 py-5",
                div { class: "mx-auto w-full max-w-4xl",
                    ProfileSection { profiles, active: active_profile, form: profile_form }
                    if !agents.is_empty() {
                        section {
                            div { class: "mb-2.5 px-0.5 text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground", {translate("team-agent")} }
                            div { class: "flex flex-col gap-0.5",
                                for agent in agents.iter() {
                                    AgentRow { key: "{agent.member.id}", presentation: agent.clone() }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProfileSection(
    profiles: Vec<ProfileRow>,
    active: Option<ProfileRow>,
    form: Option<TeamProfileForm>,
) -> Element {
    let profile_count = profiles.len();
    let active_id = active
        .as_ref()
        .map(|profile| profile.id.clone())
        .unwrap_or_default();
    let switch_active_id = active_id.clone();
    let edit_active_id = active_id.clone();
    let mut profile_open = use_signal(|| false);

    rsx! {
        section { class: "mb-6",
            div { class: "mb-2.5 flex items-center justify-between px-0.5",
                div { class: "text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground", {translate("team-you")} }
            }
            div { class: "flex max-w-xl flex-col gap-2",
                if let Some(form) = form.as_ref().filter(|form| form.profile_id.is_some()) {
                    div { class: "flex h-12 items-center rounded-lg bg-foreground/[0.035] px-2",
                        ProfileFormView {
                            form: form.clone(),
                            color: active.as_ref().map(|profile| profile.color.clone()).unwrap_or_default(),
                        }
                    }
                } else if let Some(active) = active.clone() {
                    div { class: "flex items-stretch rounded-lg bg-primary/[0.055] p-1",
                        div { class: "min-w-0 flex-1",
                            DropdownSelector {
                                label: active.name.clone(),
                                title: active.id.clone(),
                                open: profile_open(),
                                active: true,
                                align: DropdownAlign::Start,
                                class: "w-full min-w-0".to_string(),
                                trigger_class: "h-10 w-full justify-start bg-transparent px-2 hover:bg-foreground/[0.055]".to_string(),
                                popup_class: "max-h-80 min-w-64 overflow-y-auto p-1".to_string(),
                                ontoggle: move |_| profile_open.set(!profile_open()),
                                on_dismiss: move |_| profile_open.set(false),
                                leading: rsx! {
                                    Avatar {
                                        src: None,
                                        seed: active.id.clone(),
                                        background: active.color.clone(),
                                        alt: active.name.clone(),
                                        class: "size-7",
                                    }
                                },
                                for profile in profiles.iter().cloned() {
                                    {
                                        let selected = profile.id == active_id;
                                        let profile_id = profile.id.clone();
                                        let selected_profile_id = profile_id.clone();
                                        let active_profile_id = switch_active_id.clone();
                                        rsx! {
                                            DropdownOption {
                                                key: "profile-{profile.id}",
                                                label: profile.name.clone(),
                                                secondary: profile.id.clone(),
                                                selected,
                                                onclick: move |_| {
                                                    profile_open.set(false);
                                                    if selected_profile_id != active_profile_id {
                                                        send(&TeamProfileSwitchRequest {
                                                            profile_id: selected_profile_id.clone(),
                                                        });
                                                    }
                                                },
                                                leading: rsx! {
                                                Avatar {
                                                    src: None,
                                                    seed: profile_id,
                                                    background: profile.color.clone(),
                                                    alt: profile.name.clone(),
                                                    class: "size-7",
                                                }
                                                },
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        button {
                            id: "edit-active-profile",
                            r#type: "button",
                            class: "flex size-9 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/[0.075] hover:text-foreground",
                            title: translate("team-edit-profile"),
                            onclick: move |_| {
                                send(&TeamProfileFormOpenRequest {
                                    profile_id: Some(edit_active_id.clone()),
                                    draft: active.name.clone(),
                                });
                            },
                            LineIconView { icon: LineIcon::Pencil, class: "size-4" }
                        }
                    }
                }
                if let Some(form) = form.as_ref().filter(|form| form.profile_id.is_none()) {
                    div { key: "create-{profile_count}", class: "flex h-12 items-center rounded-lg bg-foreground/[0.035] px-2",
                        ProfileFormView { form: form.clone(), color: String::new() }
                    }
                } else {
                    button {
                        id: "new-profile",
                        r#type: "button",
                        class: "flex h-10 items-center justify-center gap-2 rounded-lg bg-foreground/[0.03] text-sm font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.065] hover:text-foreground",
                        onclick: move |_| {
                            send(&TeamProfileFormOpenRequest {
                                profile_id: None,
                                draft: String::new(),
                            });
                        },
                        svg { class: "size-4", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8",
                            path { d: "M12 5v14M5 12h14" }
                        }
                        {translate("team-new-profile")}
                    }
                }
            }
        }
    }
}

#[component]
fn ProfileFormView(form: TeamProfileForm, color: String) -> Element {
    let mut finished = use_signal(|| false);
    let submission = ProfileFormSubmission(form.clone());
    let enter = submission.clone();
    let click = submission.clone();
    let seed = if form.draft.trim().is_empty() {
        "profile".to_string()
    } else {
        form.draft.clone()
    };
    let disabled = form.draft.trim().is_empty();
    rsx! {
        Avatar {
            src: None,
            seed,
            background: color,
            alt: form.draft.clone(),
            class: "size-8 shrink-0 text-[9px]".to_string(),
        }
        input {
            r#type: "text",
            class: "h-full min-w-0 flex-1 bg-transparent px-2 text-sm text-foreground outline-none placeholder:text-muted-foreground/60",
            placeholder: translate("team-profile-name"),
            aria_label: translate("team-profile-name"),
            value: form.draft,
            autofocus: true,
            autocomplete: "off",
            oninput: move |event: FormEvent| {
                send(&TeamProfileFormInputRequest { draft: event.value() });
            },
            onkeydown: move |event: KeyboardEvent| match event.key() {
                Key::Enter if !finished() => {
                    event.prevent_default();
                    finished.set(true);
                    enter.submit();
                }
                Key::Escape if !finished() => {
                    event.prevent_default();
                    finished.set(true);
                    send(&TeamProfileFormCloseRequest);
                }
                _ => {}
            },
        }
        button {
            r#type: "button",
            class: "flex size-8 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/[0.075] hover:text-foreground disabled:pointer-events-none disabled:opacity-30",
            title: translate("common-save"),
            aria_label: translate("common-save"),
            disabled,
            onclick: move |_| {
                if !finished() {
                    finished.set(true);
                    click.submit();
                }
            },
            LineIconView { icon: LineIcon::Check, class: "size-4" }
        }
    }
}

#[derive(Clone)]
struct ProfileFormSubmission(TeamProfileForm);

impl ProfileFormSubmission {
    fn submit(&self) {
        let name = self.0.draft.trim().to_string();
        if name.is_empty() {
            send(&TeamProfileFormCloseRequest);
            return;
        }
        match &self.0.profile_id {
            Some(profile_id) => {
                send(&TeamProfileUpdateRequest {
                    profile_id: profile_id.clone(),
                    name,
                });
            }
            None => {
                send(&TeamProfileCreateRequest { name });
            }
        }
        send(&TeamProfileFormCloseRequest);
    }
}

#[component]
fn AgentRow(presentation: TeamAgentPresentation) -> Element {
    let subtitle = match presentation.subtitle {
        TeamAgentSubtitle::None => None,
        TeamAgentSubtitle::Role => Some(translate("team-agent")),
        TeamAgentSubtitle::Title(title) => Some(title),
    };
    let member = presentation.member;

    rsx! {
        div {
            class: "flex items-start gap-3 rounded-lg px-2 py-2 hover:bg-foreground/[0.04]",
            AgentAvatar { member: member.clone() }
            div { class: "flex min-w-0 flex-1 flex-col gap-0.5 pt-0.5",
                div { class: "flex min-w-0 items-center gap-2",
                    span { class: "truncate text-sm font-semibold text-foreground", "{member.name}" }
                    if member.is_running {
                        Badge { class: "gap-1.5 rounded-full bg-success/15 px-2 py-0.5 text-[11px] font-medium text-success",
                            span { class: "size-1.5 rounded-full bg-success animate-pulse" }
                            {translate("common-running")}
                        }
                    } else if member.is_done_unseen {
                        Badge { class: "gap-1.5 rounded-full bg-amber-500/15 px-2 py-0.5 text-[11px] font-medium text-amber-600 dark:text-amber-400",
                            span { class: "size-1.5 rounded-full bg-amber-400 animate-pulse" }
                            {translate("common-done")}
                        }
                    }
                }
                if let Some(subtitle) = subtitle {
                    span { class: "truncate text-xs text-muted-foreground", "{subtitle}" }
                }
                if !member.sid.is_empty() {
                    span { class: "truncate font-mono text-[11px] text-muted-foreground/50", "{member.sid}" }
                }
            }
        }
    }
}

#[component]
fn AgentAvatar(member: TeamMemberRow) -> Element {
    let src = FaviconSource::resolve(&member.icon, &member.url);

    rsx! {
        div { class: "relative shrink-0",
            AgentThumbnail {
                src,
                seed: member.name.clone(),
                background: member.color.clone(),
                alt: member.name.clone(),
                class: "size-8 text-sm",
            }
            if member.is_running {
                span { class: "absolute -bottom-0.5 -right-0.5 size-3 rounded-full bg-success ring-2 ring-background animate-pulse" }
            } else if member.is_done_unseen {
                span { class: "absolute -bottom-0.5 -right-0.5 size-3 rounded-full bg-amber-400 ring-2 ring-background animate-pulse" }
            }
        }
    }
}
