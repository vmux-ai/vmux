#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_core::event::team::{TEAM_EVENT, TeamEvent, TeamMemberRow};
use vmux_ui::favicon::favicon_src_for_url;
use vmux_ui::hooks::{use_bin_event_listener, use_theme};

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut team = use_signal(TeamEvent::default);
    let _listener = use_bin_event_listener::<TeamEvent, _>(TEAM_EVENT, move |data| team.set(data));

    let members = team().members;
    let count = members.len();
    let user = members.iter().find(|m| m.is_user).cloned();
    let agents: Vec<TeamMemberRow> = members.iter().filter(|m| !m.is_user).cloned().collect();
    let agent_count = agents.len();
    let subtitle = match agent_count {
        0 => "Just you in this space".to_string(),
        1 => "You and 1 agent".to_string(),
        n => format!("You and {n} agents"),
    };

    rsx! {
        div {
            class: "flex h-full min-h-0 flex-col bg-background text-foreground",
            header { class: "flex items-center justify-between border-b border-border px-5 py-4",
                div { class: "min-w-0",
                    h1 { class: "text-lg font-semibold tracking-tight", "Team" }
                    p { class: "mt-0.5 truncate text-xs text-muted-foreground", "{subtitle}" }
                }
                if count > 0 {
                    span { class: "shrink-0 rounded-full border border-border bg-card px-2.5 py-1 text-xs font-medium text-muted-foreground",
                        "{count}"
                    }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-3 py-3",
                if members.is_empty() {
                    div { class: "flex h-full flex-col items-center justify-center gap-2 text-muted-foreground",
                        div { class: "flex size-12 items-center justify-center rounded-full border border-dashed border-border",
                            svg {
                                class: "size-5",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.5",
                                path { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" }
                                circle { cx: "9", cy: "7", r: "4" }
                                path { d: "M22 21v-2a4 4 0 0 0-3-3.87" }
                            }
                        }
                        span { class: "text-sm", "No one here yet" }
                    }
                } else {
                    div { class: "flex flex-col gap-0.5",
                        if let Some(user) = user.clone() {
                            TeamRow { member: user }
                        }
                        if !agents.is_empty() {
                            div { class: "ml-6 flex flex-col gap-0.5 border-l border-border/60 pl-3",
                                for agent in agents.iter() {
                                    TeamRow { key: "{agent.id}", member: agent.clone() }
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
fn TeamRow(member: TeamMemberRow) -> Element {
    // Secondary line: a real page title (skip the default "Kind (sid)" one,
    // since the session id is shown on its own line), else a role label.
    let default_title = format!("{} (", member.name);
    let subtitle = if member.is_user {
        Some("You".to_string())
    } else if !member.title.is_empty()
        && member.title != member.name
        && !member.title.starts_with(&default_title)
    {
        Some(member.title.clone())
    } else if member.sid.is_empty() {
        Some("Agent".to_string())
    } else {
        None
    };

    rsx! {
        div {
            class: "flex items-start gap-3 rounded-lg px-2 py-2 hover:bg-muted/40",
            TeamAvatar { member: member.clone(), size: 32 }
            div { class: "flex min-w-0 flex-1 flex-col gap-0.5 pt-0.5",
                div { class: "flex min-w-0 items-center gap-2",
                    span {
                        class: if member.is_user {
                            "text-sm font-semibold text-foreground"
                        } else {
                            "truncate text-sm font-semibold text-foreground"
                        },
                        "{member.name}"
                    }
                    if member.is_running {
                        span { class: "flex shrink-0 items-center gap-1.5 rounded-full bg-emerald-500/15 px-2 py-0.5 text-[11px] font-medium text-emerald-400",
                            span { class: "size-1.5 rounded-full bg-emerald-400 animate-pulse" }
                            "running"
                        }
                    } else if member.is_done_unseen {
                        span { class: "flex shrink-0 items-center gap-1.5 rounded-full bg-amber-500/15 px-2 py-0.5 text-[11px] font-medium text-amber-400",
                            span { class: "size-1.5 rounded-full bg-amber-400 animate-pulse" }
                            "done"
                        }
                    }
                }
                if let Some(subtitle) = subtitle {
                    span { class: "truncate text-xs text-muted-foreground", "{subtitle}" }
                }
                if !member.is_user && !member.sid.is_empty() {
                    span { class: "truncate font-mono text-[11px] text-muted-foreground/50", "{member.sid}" }
                }
            }
        }
    }
}

#[component]
fn TeamAvatar(member: TeamMemberRow, size: u32) -> Element {
    let src = favicon_src_for_url(&member.icon, &member.url);
    let bg = if src.is_some() {
        String::new()
    } else {
        format!("background:{}", member.color)
    };
    let dim = format!("height:{size}px;width:{size}px;{bg}");

    rsx! {
        div { class: "relative shrink-0",
            div {
                class: "inline-flex items-center justify-center overflow-hidden rounded-full text-sm font-semibold text-white",
                style: "{dim}",
                if let Some(src) = src.as_ref() {
                    img { class: "size-full object-cover", src: "{src}" }
                } else {
                    "{member.initials}"
                }
            }
            if member.is_running {
                span { class: "absolute -bottom-0.5 -right-0.5 size-3 rounded-full bg-emerald-400 ring-2 ring-background animate-pulse" }
            } else if member.is_done_unseen {
                span { class: "absolute -bottom-0.5 -right-0.5 size-3 rounded-full bg-amber-400 ring-2 ring-background animate-pulse" }
            }
        }
    }
}
