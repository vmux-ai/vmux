#![allow(non_snake_case)]

use crate::event::{AgentEntry, AgentsInstall, AgentsOpen, AgentsUninstall};
use crate::vibe::setup::event::AgentInstallRunRequest;
use dioxus::prelude::*;
use vmux_ui::components::manager::{
    ManagerBadge, ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList,
    ManagerPage, ManagerRow, ManagerSkeleton, ManagerSpinner, ManagerTone,
};
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::{send, use_listener, use_theme};
use vmux_ui::i18n::translate;

#[component]
pub fn Page() -> Element {
    let catalog = use_catalog();
    let mut query = catalog.query;
    let all_agents = catalog.all();
    let filtered = catalog.matching();

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
                if !(catalog.loaded)() {
                    ManagerSkeleton {}
                } else if filtered.is_empty() {
                    ManagerEmpty {
                        title: translate("agents-empty"),
                        detail: translate("agents-empty-detail"),
                    }
                }
                for agent in filtered.iter() {
                    AgentRow { agent: agent.clone(), catalog }
                }
            }
        }
    }
}

/// One installed agent, with its version and install controls.
#[component]
fn AgentRow(agent: AgentEntry, catalog: Catalog) -> Element {
    let agent = &agent;
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
                    ManagerBadge { tone: ManagerTone::for_runtime(&agent.runtime), "{agent.runtime}" }
                }
            },
            actions: rsx! { AgentActions { agent: agent.clone(), catalog } },
        }
    }
}

/// A version-pin control, shown only for npx/uvx agents (native binaries can't be pinned). Renders
/// a dropdown of published versions when they've been fetched, else a free-text fallback (so it
/// still works before the fetch lands or when the registry can't be queried).
/// The pinned-version field, for runtimes that support pinning.
#[component]
fn AgentVersionInput(agent: AgentEntry, catalog: Catalog) -> Element {
    let agent = &agent;
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
                oninput: move |event: FormEvent| catalog.set_pinned_version(&id, &event.value()),
            }
        };
    }
    // "latest" tracks npm's latest dist-tag: the newest *released* version. Prereleases (semver
    // build suffix, e.g. `-prerelease.5`) are not what `@latest` installs, so skip them here.
    let latest = agent
        .available_versions
        .iter()
        .find(|version| !version.contains('-'))
        .or_else(|| agent.available_versions.first())
        .cloned()
        .unwrap_or_default();
    let latest_label = if latest.is_empty() {
        translate("agents-version-latest")
    } else {
        format!("{} ({latest})", translate("agents-version-latest"))
    };
    rsx! {
        select {
            class: "w-32 truncate rounded-md bg-white/[0.04] px-2 py-1 text-xs text-foreground ring-1 ring-white/[0.06] focus:outline-none",
            title: translate("agents-version-hint"),
            onchange: move |event: FormEvent| catalog.set_pinned_version(&id, &event.value()),
            option { value: "", selected: agent.pinned_version.is_empty(), "{latest_label}" }
            for version in agent.available_versions.iter().filter(|version| *version != &latest) {
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

/// The controls on the right of an agent row.
#[component]
fn AgentActions(agent: AgentEntry, catalog: Catalog) -> Element {
    let agent = &agent;
    let pinnable = agent.source == "acp" && matches!(agent.runtime.as_str(), "node" | "python");
    rsx! {
        if pinnable {
            AgentVersionInput { agent: agent.clone(), catalog }
        }
        AgentStatusButtons { agent: agent.clone(), catalog }
    }
}

/// Install, apply and uninstall, according to what the agent needs.
#[component]
fn AgentStatusButtons(agent: AgentEntry, catalog: Catalog) -> Element {
    let agent = &agent;
    let id = agent.id.clone();
    let install_id = agent.id.clone();
    let uninstall_id = agent.id.clone();
    let apply_id = agent.id.clone();
    let launch_url = agent.launch_url.clone();
    let source = agent.source.clone();
    let update_version = agent.pinned_version.clone();
    let install_version = agent.pinned_version.clone();
    let apply_version = agent.pinned_version.clone();
    // Agents that render a version selector don't need a redundant "Installed" label next to it,
    // but they do need an explicit way to apply a version change after picking one.
    let has_version_selector =
        agent.source == "acp" && matches!(agent.runtime.as_str(), "node" | "python");
    match agent.status.as_str() {
        "installing" => rsx! { ManagerSpinner { detail: agent.detail.clone() } },
        "installed" => rsx! {
            if !has_version_selector {
                span { class: "text-xs font-medium text-success", {translate("common-installed")} }
            }
            if has_version_selector && agent.pinned_version != agent.installed_version {
                ManagerButton {
                    variant: ManagerButtonVariant::Primary,
                    onclick: move |_| {
                        catalog.set_status(&apply_id, "installing", &translate("agents-updating"));
                        let _ = send(&AgentsInstall { id: apply_id.clone(), version: apply_version.clone() });
                    },
                    {translate("agents-apply-version")}
                }
            }
            ManagerButton {
                variant: ManagerButtonVariant::Secondary,
                onclick: move |_| {
                    let _ = send(&AgentsOpen { url: launch_url.clone() });
                },
                {translate("common-open")}
            }
            if agent.uninstallable {
                ManagerButton {
                    variant: ManagerButtonVariant::Danger,
                    onclick: move |_| {
                        catalog.set_status(&uninstall_id, "available", "");
                        let _ = send(&AgentsUninstall { id: uninstall_id.clone() });
                    },
                    {translate("common-uninstall")}
                }
            }
        },
        "update" => rsx! {
            ManagerButton {
                variant: ManagerButtonVariant::Primary,
                onclick: move |_| {
                    catalog.set_status(&id, "installing", &translate("agents-updating"));
                    let _ = send(&AgentsInstall { id: id.clone(), version: update_version.clone() });
                },
                {translate("common-update")}
            }
        },
        "error" => rsx! {
            span { class: "max-w-36 truncate text-xs text-red-500", title: "{agent.detail}", {translate("common-failed")} }
            ManagerButton {
                variant: ManagerButtonVariant::Secondary,
                onclick: move |_| {
                    catalog.set_status(&install_id, "installing", &translate("agents-retrying"));
                    if source == "cli" {
                        let segment = install_id.trim_start_matches("cli:").to_string();
                        let _ = send(&AgentInstallRunRequest { agent: segment });
                    } else {
                        let _ = send(&AgentsInstall { id: install_id.clone(), version: install_version.clone() });
                    }
                },
                {translate("common-retry")}
            }
        },
        _ => rsx! {
            ManagerButton {
                variant: ManagerButtonVariant::Primary,
                onclick: move |_| {
                    catalog.set_status(&install_id, "installing", &translate("agents-preparing"));
                    if source == "cli" {
                        let segment = install_id.trim_start_matches("cli:").to_string();
                        let _ = send(&AgentInstallRunRequest { agent: segment });
                    } else {
                        let _ = send(&AgentsInstall { id: install_id.clone(), version: install_version.clone() });
                    }
                },
                {translate("common-install")}
            }
        },
    }
}

use crate::event::{AGENTS_CATALOG_EVENT, AgentsCatalog, AgentsCatalogRequest};
use crate::vibe::setup::event::{AGENT_SETUP_RESULT_EVENT, AgentSetupResult};

/// Every installed agent, the search narrowing them, and whether the first fetch has landed.
#[derive(Clone, Copy, PartialEq)]
pub struct Catalog {
    agents: Signal<Vec<AgentEntry>>,
    pub query: Signal<String>,
    pub loaded: Signal<bool>,
}

/// Fetch the catalog, keep it in step with install results, and title the document.
pub fn use_catalog() -> Catalog {
    let locale = use_theme();
    let catalog = Catalog {
        agents: use_signal(Vec::new),
        query: use_signal(String::new),
        loaded: use_signal(|| false),
    };

    let mut agents = catalog.agents;
    let mut loaded = catalog.loaded;
    let _fetched = use_listener::<AgentsCatalog, _>(AGENTS_CATALOG_EVENT, move |incoming| {
        agents.set(incoming.agents);
        loaded.set(true);
    });

    // An install reports its own result, so the row is moved before the refetch lands rather than
    // sitting on "installing" for a round trip.
    let _installed = use_listener::<AgentSetupResult, _>(AGENT_SETUP_RESULT_EVENT, move |result| {
        let id = format!("cli:{}", result.agent);
        if result.ok {
            catalog.set_status(&id, "installed", "");
            Catalog::request();
        } else {
            catalog.set_status(&id, "error", &translate("agents-install-failed"));
        }
    });

    use_effect(move || {
        locale();
        set_document_title(&translate("agents-title"));
        Catalog::request();
    });

    catalog
}

impl Catalog {
    /// Ask the host to send the catalog.
    pub fn request() {
        let _ = send(&AgentsCatalogRequest {});
    }

    pub fn all(&self) -> Vec<AgentEntry> {
        (self.agents)()
    }

    /// The entries the current search leaves visible.
    pub fn matching(&self) -> Vec<AgentEntry> {
        let query = (self.query)();
        (self.agents)()
            .into_iter()
            .filter(|agent| agent.matches(&query))
            .collect()
    }

    /// Move one row to a new state, without waiting for a refetch to confirm it.
    pub fn set_status(&self, id: &str, status: &str, detail: &str) {
        let mut agents = self.agents;
        agents.with_mut(|list| {
            if let Some(agent) = list.iter_mut().find(|agent| agent.id == id) {
                agent.status = status.to_string();
                agent.detail = detail.to_string();
            }
        });
    }

    pub fn set_pinned_version(&self, id: &str, version: &str) {
        let mut agents = self.agents;
        agents.with_mut(|list| {
            if let Some(agent) = list.iter_mut().find(|agent| agent.id == id) {
                agent.pinned_version = version.to_string();
            }
        });
    }
}

/// The browser tab's title. A phone has no tab, so this is where that difference stops.
#[cfg(web)]
fn set_document_title(title: &str) {
    if let Some(doc) = web_sys::window().and_then(|window| window.document()) {
        doc.set_title(title);
    }
}

#[cfg(not(web))]
fn set_document_title(_title: &str) {}
