#![allow(non_snake_case)]

use crate::agents_page::event::{
    AGENTS_CATALOG_EVENT, AgentEntry, AgentsCatalog, AgentsCatalogRequest, AgentsInstall,
    AgentsOpen, AgentsUninstall,
};
use crate::vibe::setup::event::{
    AGENT_SETUP_RESULT_EVENT, AgentInstallRunRequest, AgentSetupResult,
};
use dioxus::prelude::*;
use vmux_ui::components::manager::{
    ManagerBadge, ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList,
    ManagerPage, ManagerRow, ManagerSkeleton, ManagerSpinner, ManagerTone,
};
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};

fn request_catalog() {
    let _ = try_cef_bin_emit_rkyv(&AgentsCatalogRequest {});
}

fn set_status(mut agents: Signal<Vec<AgentEntry>>, id: &str, status: &str, detail: &str) {
    agents.with_mut(|list| {
        if let Some(agent) = list.iter_mut().find(|agent| agent.id == id) {
            agent.status = status.to_string();
            agent.detail = detail.to_string();
        }
    });
}

fn runtime_tone(runtime: &str) -> ManagerTone {
    match runtime {
        "native" => ManagerTone::Green,
        "node" => ManagerTone::Cyan,
        "python" => ManagerTone::Amber,
        _ => ManagerTone::Neutral,
    }
}

fn matches_search(agent: &AgentEntry, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || agent.name.to_lowercase().contains(&query)
        || agent.id.to_lowercase().contains(&query)
        || agent.description.to_lowercase().contains(&query)
        || agent.runtime.to_lowercase().contains(&query)
        || agent.source.to_lowercase().contains(&query)
}

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut agents = use_signal(Vec::<AgentEntry>::new);
    let mut query = use_signal(String::new);
    let mut loaded = use_signal(|| false);

    let _catalog =
        use_bin_event_listener::<AgentsCatalog, _>(AGENTS_CATALOG_EVENT, move |catalog| {
            agents.set(catalog.agents);
            loaded.set(true);
        });
    let _setup =
        use_bin_event_listener::<AgentSetupResult, _>(AGENT_SETUP_RESULT_EVENT, move |result| {
            let id = format!("cli:{}", result.agent);
            if result.ok {
                set_status(agents, &id, "installed", "");
                request_catalog();
            } else {
                set_status(agents, &id, "error", "Install failed");
            }
        });

    use_effect(move || {
        if let Some(doc) = web_sys::window().and_then(|window| window.document()) {
            doc.set_title("Agents");
        }
        request_catalog();
    });

    let all_agents = agents();
    let filtered: Vec<AgentEntry> = all_agents
        .iter()
        .filter(|agent| matches_search(agent, &query()))
        .cloned()
        .collect();

    rsx! {
        ManagerPage {
            ManagerHeader {
                title: "Agents",
                count: all_agents.len(),
                search_value: query(),
                search_placeholder: "Search ACP and CLI agents…",
                onsearch: move |event: FormEvent| query.set(event.value()),
                onkeydown: None,
                actions: rsx! {},
            }
            ManagerList {
                if !loaded() {
                    ManagerSkeleton {}
                } else if filtered.is_empty() {
                    ManagerEmpty {
                        title: "No matching agents",
                        detail: "Try a name, runtime, or ACP/CLI.",
                    }
                }
                for agent in filtered.iter() {
                    {render_agent(agent, agents)}
                }
            }
        }
    }
}

fn render_agent(agent: &AgentEntry, agents: Signal<Vec<AgentEntry>>) -> Element {
    let icon_url = agent.icon.clone();
    let launch_url = agent.launch_url.clone();
    rsx! {
        ManagerRow {
            icon: rsx! {
                Favicon {
                    favicon_url: icon_url,
                    url: launch_url,
                    class: "h-6 w-6 rounded-md object-contain".to_string(),
                    globe_class: "h-5 w-5 text-muted-foreground".to_string(),
                }
            },
            title: agent.name.clone(),
            subtitle: agent.description.clone(),
            meta: rsx! {
                ManagerBadge { tone: ManagerTone::Neutral, "{agent.source}" }
                if agent.runtime != agent.source {
                    ManagerBadge { tone: runtime_tone(&agent.runtime), "{agent.runtime}" }
                }
            },
            actions: render_action(agent, agents),
        }
    }
}

fn render_action(agent: &AgentEntry, agents: Signal<Vec<AgentEntry>>) -> Element {
    let id = agent.id.clone();
    let install_id = agent.id.clone();
    let uninstall_id = agent.id.clone();
    let launch_url = agent.launch_url.clone();
    let source = agent.source.clone();
    match agent.status.as_str() {
        "installing" => rsx! { ManagerSpinner { detail: agent.detail.clone() } },
        "installed" => rsx! {
            span { class: "text-xs font-medium text-emerald-600 dark:text-emerald-400", "Installed" }
            ManagerButton {
                variant: ManagerButtonVariant::Secondary,
                onclick: move |_| {
                    let _ = try_cef_bin_emit_rkyv(&AgentsOpen { url: launch_url.clone() });
                },
                "Open"
            }
            if agent.uninstallable {
                ManagerButton {
                    variant: ManagerButtonVariant::Danger,
                    onclick: move |_| {
                        set_status(agents, &uninstall_id, "available", "");
                        let _ = try_cef_bin_emit_rkyv(&AgentsUninstall { id: uninstall_id.clone() });
                    },
                    "Uninstall"
                }
            }
        },
        "update" => rsx! {
            ManagerButton {
                variant: ManagerButtonVariant::Primary,
                onclick: move |_| {
                    set_status(agents, &id, "installing", "Updating…");
                    let _ = try_cef_bin_emit_rkyv(&AgentsInstall { id: id.clone() });
                },
                "Update"
            }
        },
        "error" => rsx! {
            span { class: "max-w-36 truncate text-xs text-red-500", title: "{agent.detail}", "Failed" }
            ManagerButton {
                variant: ManagerButtonVariant::Secondary,
                onclick: move |_| {
                    set_status(agents, &install_id, "installing", "Retrying…");
                    if source == "cli" {
                        let segment = install_id.trim_start_matches("cli:").to_string();
                        let _ = try_cef_bin_emit_rkyv(&AgentInstallRunRequest { agent: segment });
                    } else {
                        let _ = try_cef_bin_emit_rkyv(&AgentsInstall { id: install_id.clone() });
                    }
                },
                "Retry"
            }
        },
        _ => rsx! {
            ManagerButton {
                variant: ManagerButtonVariant::Primary,
                onclick: move |_| {
                    set_status(agents, &install_id, "installing", "Preparing…");
                    if source == "cli" {
                        let segment = install_id.trim_start_matches("cli:").to_string();
                        let _ = try_cef_bin_emit_rkyv(&AgentInstallRunRequest { agent: segment });
                    } else {
                        let _ = try_cef_bin_emit_rkyv(&AgentsInstall { id: install_id.clone() });
                    }
                },
                "Install"
            }
        },
    }
}
