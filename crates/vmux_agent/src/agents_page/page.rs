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
use vmux_ui::i18n::translate;

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
    let locale = use_theme();
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
                set_status(agents, &id, "error", &translate("agents-install-failed"));
            }
        });

    use_effect(move || {
        locale();
        if let Some(doc) = web_sys::window().and_then(|window| window.document()) {
            doc.set_title(&translate("agents-title"));
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
                title: translate("agents-title"),
                count: all_agents.len(),
                search_value: query(),
                search_placeholder: translate("agents-search"),
                onsearch: move |event: FormEvent| query.set(event.value()),
                onkeydown: None,
                actions: rsx! {},
            }
            ManagerList {
                if !loaded() {
                    ManagerSkeleton {}
                } else if filtered.is_empty() {
                    ManagerEmpty {
                        title: translate("agents-empty"),
                        detail: translate("agents-empty-detail"),
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
    let description = if agent.source == "cli" {
        translate("agents-terminal-coding-agent")
    } else {
        agent.description.clone()
    };
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
            subtitle: description,
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

fn set_pinned_version(mut agents: Signal<Vec<AgentEntry>>, id: &str, version: &str) {
    agents.with_mut(|list| {
        if let Some(agent) = list.iter_mut().find(|agent| agent.id == id) {
            agent.pinned_version = version.to_string();
        }
    });
}

/// A version-pin control, shown only for npx/uvx agents (native binaries can't be pinned). Renders
/// a dropdown of published versions when they've been fetched, else a free-text fallback (so it
/// still works before the fetch lands or when the registry can't be queried).
fn render_version_input(agent: &AgentEntry, agents: Signal<Vec<AgentEntry>>) -> Element {
    let id = agent.id.clone();
    if agent.available_versions.is_empty() {
        return rsx! {
            input {
                class: "w-20 rounded-md bg-white/[0.04] px-2 py-1 text-xs text-foreground ring-1 ring-white/[0.06] placeholder:text-muted-foreground focus:outline-none",
                r#type: "text",
                spellcheck: "false",
                autocomplete: "off",
                value: "{agent.pinned_version}",
                placeholder: translate("agents-version-latest"),
                title: translate("agents-version-hint"),
                oninput: move |event: FormEvent| set_pinned_version(agents, &id, &event.value()),
            }
        };
    }
    let status = agent.status.clone();
    let newest = agent
        .available_versions
        .first()
        .cloned()
        .unwrap_or_default();
    let latest_label = if newest.is_empty() {
        translate("agents-version-latest")
    } else {
        format!("{} ({newest})", translate("agents-version-latest"))
    };
    rsx! {
        select {
            class: "w-28 rounded-md bg-white/[0.04] px-2 py-1 text-xs text-foreground ring-1 ring-white/[0.06] focus:outline-none",
            title: translate("agents-version-hint"),
            onchange: move |event: FormEvent| {
                let version = event.value();
                set_pinned_version(agents, &id, &version);
                if matches!(status.as_str(), "installed" | "update") {
                    set_status(agents, &id, "installing", &translate("agents-updating"));
                    let _ = try_cef_bin_emit_rkyv(&AgentsInstall { id: id.clone(), version });
                }
            },
            option { value: "", selected: agent.pinned_version.is_empty(), "{latest_label}" }
            for version in agent.available_versions.iter().filter(|version| *version != &newest) {
                option {
                    key: "{version}",
                    value: "{version}",
                    selected: version == &agent.pinned_version,
                    "{version}"
                }
            }
        }
    }
}

fn render_action(agent: &AgentEntry, agents: Signal<Vec<AgentEntry>>) -> Element {
    let pinnable = agent.source == "acp" && matches!(agent.runtime.as_str(), "node" | "python");
    rsx! {
        if pinnable {
            {render_version_input(agent, agents)}
        }
        {render_status_buttons(agent, agents)}
    }
}

fn render_status_buttons(agent: &AgentEntry, agents: Signal<Vec<AgentEntry>>) -> Element {
    let id = agent.id.clone();
    let install_id = agent.id.clone();
    let uninstall_id = agent.id.clone();
    let launch_url = agent.launch_url.clone();
    let source = agent.source.clone();
    let update_version = agent.pinned_version.clone();
    let install_version = agent.pinned_version.clone();
    // Agents that render a version selector don't need a redundant "Installed" label next to it.
    let has_version_selector =
        agent.source == "acp" && matches!(agent.runtime.as_str(), "node" | "python");
    match agent.status.as_str() {
        "installing" => rsx! { ManagerSpinner { detail: agent.detail.clone() } },
        "installed" => rsx! {
            if !has_version_selector {
                span { class: "text-xs font-medium text-emerald-600 dark:text-emerald-400", {translate("common-installed")} }
            }
            ManagerButton {
                variant: ManagerButtonVariant::Secondary,
                onclick: move |_| {
                    let _ = try_cef_bin_emit_rkyv(&AgentsOpen { url: launch_url.clone() });
                },
                {translate("common-open")}
            }
            if agent.uninstallable {
                ManagerButton {
                    variant: ManagerButtonVariant::Danger,
                    onclick: move |_| {
                        set_status(agents, &uninstall_id, "available", "");
                        let _ = try_cef_bin_emit_rkyv(&AgentsUninstall { id: uninstall_id.clone() });
                    },
                    {translate("common-uninstall")}
                }
            }
        },
        "update" => rsx! {
            ManagerButton {
                variant: ManagerButtonVariant::Primary,
                onclick: move |_| {
                    set_status(agents, &id, "installing", &translate("agents-updating"));
                    let _ = try_cef_bin_emit_rkyv(&AgentsInstall { id: id.clone(), version: update_version.clone() });
                },
                {translate("common-update")}
            }
        },
        "error" => rsx! {
            span { class: "max-w-36 truncate text-xs text-red-500", title: "{agent.detail}", {translate("common-failed")} }
            ManagerButton {
                variant: ManagerButtonVariant::Secondary,
                onclick: move |_| {
                    set_status(agents, &install_id, "installing", &translate("agents-retrying"));
                    if source == "cli" {
                        let segment = install_id.trim_start_matches("cli:").to_string();
                        let _ = try_cef_bin_emit_rkyv(&AgentInstallRunRequest { agent: segment });
                    } else {
                        let _ = try_cef_bin_emit_rkyv(&AgentsInstall { id: install_id.clone(), version: install_version.clone() });
                    }
                },
                {translate("common-retry")}
            }
        },
        _ => rsx! {
            ManagerButton {
                variant: ManagerButtonVariant::Primary,
                onclick: move |_| {
                    set_status(agents, &install_id, "installing", &translate("agents-preparing"));
                    if source == "cli" {
                        let segment = install_id.trim_start_matches("cli:").to_string();
                        let _ = try_cef_bin_emit_rkyv(&AgentInstallRunRequest { agent: segment });
                    } else {
                        let _ = try_cef_bin_emit_rkyv(&AgentsInstall { id: install_id.clone(), version: install_version.clone() });
                    }
                },
                {translate("common-install")}
            }
        },
    }
}
