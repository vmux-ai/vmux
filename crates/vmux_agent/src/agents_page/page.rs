#![allow(non_snake_case)]

use crate::agents_page::event::{
    AGENTS_CATALOG_EVENT, AgentEntry, AgentsCatalog, AgentsCatalogRequest, AgentsInstall,
    AgentsUninstall,
};
use dioxus::prelude::*;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};

fn install(id: String) {
    let _ = try_cef_bin_emit_rkyv(&AgentsInstall { id });
}

fn uninstall(id: String) {
    let _ = try_cef_bin_emit_rkyv(&AgentsUninstall { id });
}

/// Optimistically reflect an action in the local list so the row reacts instantly, before the
/// host pushes the authoritative catalog.
fn set_status(mut agents: Signal<Vec<AgentEntry>>, id: &str, status: &str, detail: &str) {
    agents.with_mut(|list| {
        if let Some(a) = list.iter_mut().find(|a| a.id == id) {
            a.status = status.to_string();
            a.detail = detail.to_string();
        }
    });
}

fn runtime_pill(runtime: &str) -> &'static str {
    match runtime {
        "native" => "text-emerald-300 bg-emerald-400/10 ring-emerald-400/20",
        "node" => "text-lime-300 bg-lime-400/10 ring-lime-400/20",
        "python" => "text-amber-300 bg-amber-400/10 ring-amber-400/20",
        _ => "text-muted-foreground bg-foreground/10 ring-foreground/15",
    }
}

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut agents = use_signal(Vec::<AgentEntry>::new);
    let mut mounted = use_signal(|| false);

    let _listener = use_bin_event_listener::<AgentsCatalog, _>(AGENTS_CATALOG_EVENT, move |snap| {
        agents.set(snap.agents);
    });

    use_effect(move || {
        let _ = try_cef_bin_emit_rkyv(&AgentsCatalogRequest {});
        mounted.set(true);
    });

    let reveal = if mounted() {
        "opacity-100 blur-0 translate-y-0"
    } else {
        "opacity-0 blur-sm translate-y-4"
    };
    let count = agents.read().len();

    rsx! {
        main {
            class: "relative isolate flex h-screen flex-col overflow-hidden bg-background text-foreground",
            style: "background-image:radial-gradient(120% 90% at 50% -10%, rgba(129,140,248,0.06), transparent 55%);",
            div { class: "pointer-events-none absolute inset-0 -z-10 overflow-hidden",
                div { class: "absolute left-1/2 top-[-8%] h-[34rem] w-[34rem] -translate-x-1/2 rounded-full blur-[150px] dark:bg-indigo-500/12" }
                div { class: "absolute right-[8%] top-1/4 h-72 w-72 rounded-full blur-[130px] dark:bg-violet-500/10" }
            }
            header {
                class: "relative shrink-0 px-8 pt-10 pb-6 transition-all duration-700 ease-out motion-reduce:transition-none {reveal}",
                h1 { class: "bg-gradient-to-b from-foreground to-foreground/55 bg-clip-text text-3xl font-semibold tracking-tight text-transparent",
                    "Agents"
                }
                p { class: "mt-1.5 text-sm text-muted-foreground",
                    "Install a coding agent — vmux brings the runtime. "
                    span { class: "text-foreground/70", "{count} available." }
                }
            }
            div { class: "relative flex-1 overflow-y-auto px-8 pb-10",
                div { class: "mx-auto flex max-w-3xl flex-col gap-2.5 transition-all duration-700 ease-out motion-reduce:transition-none {reveal}",
                    for a in agents.read().iter() {
                        {render_agent(a, agents)}
                    }
                }
            }
        }
    }
}

fn render_agent(a: &AgentEntry, agents: Signal<Vec<AgentEntry>>) -> Element {
    rsx! {
        div {
            key: "{a.id}",
            class: "group flex items-center gap-4 rounded-2xl bg-foreground/[0.035] px-5 py-4 ring-1 ring-inset ring-foreground/10 backdrop-blur-xl transition-colors hover:bg-foreground/[0.07]",
            div { class: "flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-foreground/[0.06] ring-1 ring-inset ring-foreground/10",
                if !a.icon.is_empty() {
                    img { class: "h-6 w-6 object-contain", src: "{a.icon}" }
                }
            }
            div { class: "min-w-0 flex-1",
                div { class: "flex items-center gap-2",
                    span { class: "truncate text-sm font-medium", "{a.name}" }
                    span { class: "shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide ring-1 ring-inset {runtime_pill(&a.runtime)}",
                        "{a.runtime}"
                    }
                }
                if !a.description.is_empty() {
                    p { class: "mt-0.5 truncate text-xs leading-relaxed text-muted-foreground",
                        "{a.description}"
                    }
                }
            }
            div { class: "shrink-0", {render_action(a, agents)} }
        }
    }
}

fn render_action(a: &AgentEntry, agents: Signal<Vec<AgentEntry>>) -> Element {
    let id = a.id.clone();
    match a.status.as_str() {
        "installing" => rsx! {
            div { class: "flex items-center gap-2 text-xs text-muted-foreground",
                span { class: "h-3.5 w-3.5 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-foreground" }
                span { class: "max-w-[11rem] truncate", "{a.detail}" }
            }
        },
        "installed" => rsx! {
            div { class: "flex items-center gap-3",
                span { class: "text-xs font-medium text-emerald-400", "Installed" }
                button {
                    class: "rounded-full px-3 py-1.5 text-xs text-muted-foreground opacity-0 transition hover:bg-foreground/10 hover:text-foreground group-hover:opacity-100",
                    onclick: move |_| {
                        set_status(agents, &id, "available", "");
                        uninstall(id.clone());
                    },
                    "Uninstall"
                }
            }
        },
        "update" => rsx! {
            button {
                class: "rounded-full bg-amber-400/15 px-3.5 py-1.5 text-xs font-medium text-amber-300 ring-1 ring-inset ring-amber-400/25 transition hover:bg-amber-400/25",
                onclick: move |_| {
                    set_status(agents, &id, "installing", "Updating…");
                    install(id.clone());
                },
                "Update"
            }
        },
        "error" => rsx! {
            div { class: "flex items-center gap-2",
                span { class: "max-w-[9rem] truncate text-xs text-red-400", title: "{a.detail}", "Failed" }
                button {
                    class: "rounded-full bg-foreground/10 px-3.5 py-1.5 text-xs transition hover:bg-foreground/20",
                    onclick: move |_| {
                        set_status(agents, &id, "installing", "Retrying…");
                        install(id.clone());
                    },
                    "Retry"
                }
            }
        },
        _ => rsx! {
            button {
                class: "rounded-full bg-foreground px-4 py-1.5 text-xs font-medium text-background shadow-sm transition hover:brightness-110 active:scale-[0.98]",
                onclick: move |_| {
                    set_status(agents, &id, "installing", "Preparing…");
                    install(id.clone());
                },
                "Install"
            }
        },
    }
}
