#![allow(non_snake_case)]

use crate::agents_page::event::{AgentEntry, AgentsInstall, AgentsOpen, AgentsUninstall};
use crate::agents_page::state::{Catalog, use_catalog};
use crate::vibe::setup::event::AgentInstallRunRequest;
use dioxus::prelude::*;
use vmux_ui::components::manager::{
    ManagerBadge, ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList,
    ManagerPage, ManagerRow, ManagerSkeleton, ManagerSpinner, ManagerTone,
};
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::try_cef_bin_emit_rkyv;
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
                        let _ = try_cef_bin_emit_rkyv(&AgentsInstall { id: apply_id.clone(), version: apply_version.clone() });
                    },
                    {translate("agents-apply-version")}
                }
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
                        catalog.set_status(&uninstall_id, "available", "");
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
                    catalog.set_status(&id, "installing", &translate("agents-updating"));
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
                    catalog.set_status(&install_id, "installing", &translate("agents-retrying"));
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
                    catalog.set_status(&install_id, "installing", &translate("agents-preparing"));
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
