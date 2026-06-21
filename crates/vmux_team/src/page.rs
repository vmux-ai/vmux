#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_core::event::team::{TEAM_EVENT, TeamCommandEvent, TeamEvent, TeamMemberRow};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::JsCast;

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut team = use_signal(TeamEvent::default);
    let mut selected = use_signal(|| 0usize);
    let _listener = use_bin_event_listener::<TeamEvent, _>(TEAM_EVENT, move |data| team.set(data));

    use_effect(move || {
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("team-root"))
            && let Ok(html) = el.dyn_into::<web_sys::HtmlElement>()
        {
            let _ = html.focus();
        }
    });

    let members = team().members;
    let count = members.len();
    let sel = selected().min(count.saturating_sub(1));
    let selected_id = members.get(sel).map(|m| m.id.clone());
    let agent_count = members.iter().filter(|m| !m.is_user).count();
    let subtitle = match agent_count {
        0 => "Just you in this space".to_string(),
        1 => "You and 1 agent".to_string(),
        n => format!("You and {n} agents"),
    };

    rsx! {
        div {
            id: "team-root",
            tabindex: "0",
            class: "flex h-full min-h-0 flex-col bg-background text-foreground outline-none",
            onkeydown: move |e| {
                let down = e.code() == Code::KeyJ || e.key() == Key::ArrowDown;
                let up = e.code() == Code::KeyK || e.key() == Key::ArrowUp;
                if down {
                    e.prevent_default();
                    selected.set((sel + 1).min(count.saturating_sub(1)));
                } else if up {
                    e.prevent_default();
                    selected.set(sel.saturating_sub(1));
                } else if e.key() == Key::Enter
                    && let Some(id) = selected_id.clone()
                {
                    e.prevent_default();
                    activate(&id);
                }
            },
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
                    div { class: "flex flex-col gap-1.5",
                        for (index, member) in members.iter().enumerate() {
                            TeamRow {
                                key: "{member.id}",
                                member: member.clone(),
                                selected: index == sel,
                            }
                        }
                    }
                }
            }
        }
    }
}

fn activate(id: &str) {
    let _ = try_cef_bin_emit_rkyv(&TeamCommandEvent {
        command: "activate".to_string(),
        member_id: Some(id.to_string()),
    });
}

#[component]
fn TeamRow(member: TeamMemberRow, selected: bool) -> Element {
    let id = member.id.clone();
    let row_class = if selected {
        "group flex cursor-pointer items-center gap-3 rounded-xl border border-foreground/25 bg-muted px-3 py-2.5"
    } else {
        "group flex cursor-pointer items-center gap-3 rounded-xl border border-transparent px-3 py-2.5 transition-colors hover:border-border hover:bg-muted/50"
    };
    let role = if member.is_user { "You" } else { "Agent" };

    rsx! {
        div {
            class: "{row_class}",
            onclick: move |_| activate(&id),
            TeamAvatar { member: member.clone(), size: 40, ring_active: member.is_active }
            div { class: "flex min-w-0 flex-1 flex-col",
                div { class: "flex min-w-0 items-center gap-2",
                    span { class: "truncate text-sm font-semibold text-foreground", "{member.name}" }
                    if member.is_active {
                        span { class: "shrink-0 rounded-full bg-primary/15 px-2 py-0.5 text-[11px] font-medium text-primary",
                            "active"
                        }
                    }
                }
                span { class: "truncate text-xs text-muted-foreground", "{role}" }
            }
            if member.is_running {
                span { class: "flex shrink-0 items-center gap-1.5 rounded-full bg-emerald-500/15 px-2 py-0.5 text-[11px] font-medium text-emerald-400",
                    span { class: "size-1.5 rounded-full bg-emerald-400 animate-pulse" }
                    "running"
                }
            }
        }
    }
}

#[component]
fn TeamAvatar(member: TeamMemberRow, size: u32, ring_active: bool) -> Element {
    let has_icon = !member.icon.is_empty();
    let ring = if ring_active {
        "ring-2 ring-primary ring-offset-2 ring-offset-background"
    } else {
        ""
    };
    let bg = if has_icon {
        String::new()
    } else {
        format!("background:{}", member.color)
    };
    let dim = format!("height:{size}px;width:{size}px;{bg}");

    rsx! {
        div { class: "relative shrink-0",
            div {
                class: "inline-flex items-center justify-center overflow-hidden rounded-full text-sm font-semibold text-white {ring}",
                style: "{dim}",
                if has_icon {
                    img { class: "size-full object-cover", src: "{member.icon}" }
                } else {
                    "{member.initials}"
                }
            }
            if member.is_running {
                span { class: "absolute -bottom-0.5 -right-0.5 size-3 rounded-full bg-emerald-400 ring-2 ring-background animate-pulse" }
            }
        }
    }
}
