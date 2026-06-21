#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_core::event::team::{TEAM_EVENT, TeamCommandEvent, TeamEvent, TeamMemberRow};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut team = use_signal(TeamEvent::default);
    let _listener = use_bin_event_listener::<TeamEvent, _>(TEAM_EVENT, move |data| team.set(data));
    let members = team().members;

    rsx! {
        div { class: "min-h-screen bg-background text-foreground p-8",
            h1 { class: "mb-1 text-2xl font-semibold", "Team" }
            p { class: "mb-6 text-ui text-muted-foreground",
                "Who is active in this space — you and the agents working here."
            }
            div { class: "flex max-w-xl flex-col gap-1",
                for member in members.iter() {
                    TeamRow { key: "{member.id}", member: member.clone() }
                }
                if members.is_empty() {
                    span { class: "text-ui text-muted-foreground", "No one here yet." }
                }
            }
        }
    }
}

#[component]
fn TeamRow(member: TeamMemberRow) -> Element {
    let id = member.id.clone();
    let row_class = if member.is_active {
        "flex cursor-pointer items-center gap-3 rounded-lg px-3 py-2 text-left ring-2 ring-primary"
    } else {
        "flex cursor-pointer items-center gap-3 rounded-lg px-3 py-2 text-left hover:bg-glass-hover"
    };
    let status = if member.is_user {
        "You"
    } else if member.is_running {
        "Running"
    } else {
        "Agent"
    };

    rsx! {
        button {
            r#type: "button",
            class: "{row_class}",
            onclick: move |_| {
                let _ = try_cef_bin_emit_rkyv(&TeamCommandEvent {
                    command: "activate".to_string(),
                    member_id: Some(id.clone()),
                });
            },
            div {
                class: if member.is_running {
                    "inline-flex size-9 items-center justify-center rounded-full text-sm font-semibold text-white animate-pulse"
                } else {
                    "inline-flex size-9 items-center justify-center rounded-full text-sm font-semibold text-white"
                },
                style: "background:{member.color}",
                "{member.initials}"
            }
            div { class: "flex flex-col",
                span { class: "text-ui font-medium", "{member.name}" }
                span { class: "text-xs text-muted-foreground", "{status}" }
            }
        }
    }
}
