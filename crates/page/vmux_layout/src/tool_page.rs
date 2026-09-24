#![allow(non_snake_case)]

use std::collections::BTreeSet;

use dioxus::prelude::*;
use vmux_core::tool::{
    ToolAction, ToolItem, ToolOpenRequest, ToolProvider, ToolRequest, ToolStatus, ToolUiOperation,
    ToolsNavigateRequest, ToolsRefreshRequest, ToolsUiState,
};
use vmux_ui::components::manager::{
    ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList, ManagerPage,
    ManagerRow, ManagerSpinner, ManagerTab, ManagerTabs, ManagerThumbnail,
};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

#[vmux_native::page(
    url = "vmux://tools/",
    title = "Tools",
    component = Page,
    subtree,
    takes = vmux_core::PageMetadata
)]
pub struct ToolsPage;

#[component]
pub fn Page() -> Element {
    let initial_route = try_consume_context::<vmux_core::PageMetadata>()
        .map(|metadata| ToolsRoute::from(metadata.url.as_str()))
        .unwrap_or_default();
    let active_route = use_signal(|| initial_route);
    let route = active_route();
    if route == ToolsRoute::Extensions {
        return rsx! { crate::extensions_page::ExtensionsManager { active_route } };
    }
    rsx! { ToolManager { route, active_route } }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ToolsRoute {
    #[default]
    Acp,
    Lsp,
    Homebrew,
    Npm,
    Mcp,
    Dotfiles,
    Extensions,
}

impl From<&str> for ToolsRoute {
    fn from(url: &str) -> Self {
        let Some(route) = vmux_api::VmuxRoute::parse(url)
            .map(|route| route.canonicalized())
            .filter(|route| route.is_host("tools"))
        else {
            return Self::default();
        };
        let path = route.path_segments().next().unwrap_or_default();
        match path {
            "acp" => Self::Acp,
            "lsp" => Self::Lsp,
            "homebrew" => Self::Homebrew,
            "npm" => Self::Npm,
            "mcp" => Self::Mcp,
            "dotfiles" => Self::Dotfiles,
            "extensions" => Self::Extensions,
            _ => Self::Acp,
        }
    }
}

impl ToolsRoute {
    fn id(self) -> &'static str {
        match self {
            Self::Acp => "acp",
            Self::Lsp => "lsp",
            Self::Homebrew => "homebrew",
            Self::Npm => "npm",
            Self::Mcp => "mcp",
            Self::Dotfiles => "dotfiles",
            Self::Extensions => "extensions",
        }
    }

    fn matches(self, provider: ToolProvider) -> bool {
        match self {
            Self::Acp => provider == ToolProvider::Acp,
            Self::Lsp => provider == ToolProvider::Lsp,
            Self::Homebrew => matches!(
                provider,
                ToolProvider::HomebrewFormula | ToolProvider::HomebrewCask
            ),
            Self::Npm => provider == ToolProvider::Npm,
            Self::Mcp => provider == ToolProvider::Mcp,
            Self::Dotfiles => provider == ToolProvider::Dotfiles,
            Self::Extensions => false,
        }
    }

    fn title(self) -> String {
        match self {
            Self::Acp => translate("tools-provider-acp-agents"),
            Self::Lsp => translate("tools-provider-lsp-servers"),
            Self::Homebrew => translate("tools-homebrew"),
            Self::Npm => translate("tools-provider-npm"),
            Self::Mcp => translate("tools-provider-mcp-servers"),
            Self::Dotfiles => translate("tools-provider-dotfiles"),
            Self::Extensions => translate("extensions-title"),
        }
    }
}

#[component]
pub(crate) fn ToolsManagerTabs(mut active_route: Signal<ToolsRoute>) -> Element {
    let routes = [
        (ToolsRoute::Acp, "tools-provider-acp-agents"),
        (ToolsRoute::Lsp, "tools-provider-lsp-servers"),
        (ToolsRoute::Homebrew, "tools-homebrew"),
        (ToolsRoute::Npm, "tools-provider-npm"),
        (ToolsRoute::Mcp, "tools-provider-mcp-servers"),
        (ToolsRoute::Dotfiles, "tools-provider-dotfiles"),
        (ToolsRoute::Extensions, "extensions-title"),
    ];
    let tabs = routes
        .into_iter()
        .map(|(route, label)| ManagerTab {
            id: route.id().to_string(),
            label: translate(label),
            href: format!("vmux://tools/{}", route.id()),
        })
        .collect();
    rsx! {
        ManagerTabs {
            active: active_route().id().to_string(),
            tabs,
            onselect: move |url: String| {
                active_route.set(ToolsRoute::from(url.as_str()));
                let _ = send(&ToolsNavigateRequest { url });
            },
        }
    }
}

#[component]
fn ToolManager(route: ToolsRoute, active_route: Signal<ToolsRoute>) -> Element {
    let locale = use_theme();
    let state = use_ui_state::<ToolsUiState>();
    let mut query = use_signal(String::new);

    use_effect(move || {
        locale();
        request_snapshot(false);
    });

    let current = state();
    let pending = current
        .operations
        .iter()
        .filter(|operation| operation.is_pending())
        .map(|operation| action_key(operation.provider, operation.action, &operation.item_id))
        .collect::<BTreeSet<_>>();
    let notice = current
        .operations
        .iter()
        .rev()
        .find(|operation| operation.completion().is_some())
        .cloned();
    let snapshot = &current.snapshot;
    let search = query().trim().to_ascii_lowercase();
    let visible_count = snapshot
        .categories
        .iter()
        .filter(|category| route.matches(category.provider))
        .flat_map(|category| &category.items)
        .filter(|item| item_matches(item, &search))
        .count();
    rsx! {
        ManagerPage {
            ToolsManagerTabs { active_route }
            ManagerHeader {
                title: route.title(),
                count: visible_count,
                search_value: query(),
                search_placeholder: translate("tools-search"),
                onsearch: move |event: FormEvent| query.set(event.value()),
                onkeydown: None,
                actions: rsx! {
                    ManagerButton {
                        variant: ManagerButtonVariant::Secondary,
                        disabled: pending.contains(&action_key(
                            ToolProvider::Dotfiles,
                            ToolAction::Apply,
                            "",
                        )),
                        onclick: move |_| {
                            send_action(
                                ToolProvider::Dotfiles,
                                ToolAction::Apply,
                                String::new(),
                                String::new(),
                            );
                        },
                        {translate("tools-apply")}
                    }
                    ManagerButton {
                        variant: ManagerButtonVariant::Secondary,
                        onclick: move |_| {
                            request_snapshot(true);
                        },
                        {translate("common-refresh")}
                    }
                },
            }
            ManagerList {
                if route == ToolsRoute::Homebrew {
                    HomebrewSourceCard {
                        root: snapshot.root.clone(),
                    }
                }
                if let Some(result) = notice {
                    {
                        let (success, message) = result.completion().unwrap();
                        rsx! {
                    div {
                        class: if success {
                            "rounded-xl bg-success/10 px-4 py-3 text-xs text-success ring-1 ring-inset ring-success/20"
                        } else {
                            "rounded-xl bg-ansi-1/10 px-4 py-3 text-xs text-ansi-1 ring-1 ring-inset ring-ansi-1/20"
                        },
                        if success {
                            {action_result_message(&result)}
                        } else {
                            "{message}"
                        }
                    }
                        }
                    }
                }
                if !snapshot.error.is_empty() {
                    div { class: "whitespace-pre-wrap rounded-xl bg-amber-400/10 px-4 py-3 text-xs text-amber-700 ring-1 ring-inset ring-amber-400/20 dark:text-amber-300",
                        "{snapshot.error}"
                    }
                }
                if !snapshot.loaded {
                    ManagerSpinner { detail: translate("tools-scanning") }
                } else if visible_count == 0 {
                    ManagerEmpty {
                        title: translate("tools-empty"),
                        detail: translate("tools-empty-detail"),
                    }
                } else {
                    for category in snapshot.categories.iter() {
                        if route.matches(category.provider) && category.items.iter().any(|item| item_matches(item, &search)) {
                            div { class: "mt-3 flex items-center gap-2 px-1 first:mt-0",
                                h2 { class: "text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground", {provider_title(category.provider)} }
                                span { class: "text-[10px] text-muted-foreground/60",
                                    "{category.items.iter().filter(|item| item_matches(item, &search)).count()}"
                                }
                            }
                            for item in category.items.iter().filter(|item| item_matches(item, &search)) {
                                ToolRow { key: "{category.provider.id()}:{item.id}", item: item.clone(), pending: pending.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HomebrewSourceCard(root: String) -> Element {
    let brewfile = format!("{root}/Brewfile");
    let open_brewfile = brewfile.clone();
    rsx! {
        div { class: "flex items-center gap-3 rounded-2xl bg-foreground/[0.035] p-4 ring-1 ring-inset ring-foreground/10",
            div { class: "grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-amber-500/10 text-amber-700 ring-1 ring-inset ring-amber-500/20 dark:text-amber-300",
                svg { class: "h-5 w-5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "M17 11h1a4 4 0 0 1 0 8h-1" }
                    path { d: "M9 12v6" }
                    path { d: "M13 12v6" }
                    path { d: "M14 7.5c0-1.5-2.5-1.5-2.5 0 0-1.5-2.5-1.5-2.5 0 0-1.5-2.5-1.5-2.5 0" }
                    path { d: "M5 8h12v10a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2Z" }
                }
            }
            div { class: "min-w-0 flex-1",
                div { class: "font-medium text-foreground/95", {translate("tools-homebrew")} }
                div { class: "truncate text-xs text-muted-foreground/70", "{brewfile}" }
                div { class: "mt-1 text-[10px] text-muted-foreground/60",
                    {translate("tools-homebrew-sync")}
                }
            }
            ManagerButton {
                variant: ManagerButtonVariant::Secondary,
                disabled: root.is_empty(),
                onclick: move |_| open_tool_file(open_brewfile.clone()),
                {translate("tools-open-brewfile")}
            }
        }
    }
}

#[component]
fn ToolRow(item: ToolItem, pending: BTreeSet<String>) -> Element {
    let version = item.version.clone().unwrap_or_default();
    let provider = item.provider;
    let id = item.id.clone();
    let show_icon = provider == ToolProvider::Acp;
    rsx! {
        ManagerRow {
            show_icon,
            icon: rsx! {
                ManagerThumbnail { src: item.icon.clone(), fallback: "ACP".to_string() }
            },
            title: item.name.clone(),
            subtitle: version,
            meta: rsx! {
                span { class: "shrink-0 text-[10px] text-muted-foreground/60", {provider_short_label(provider)} }
                if item.managed {
                    span { class: "shrink-0 text-[10px] text-muted-foreground/60", {format!("· {}", translate("tools-managed"))} }
                }
                span { class: "flex shrink-0 items-center gap-1 text-[10px] text-muted-foreground/70",
                    span { class: "size-1.5 rounded-full {status_dot_class(item.status)}" }
                    {status_label(item.status)}
                }
            },
            actions: rsx! {
                for action in item.actions.iter().copied() {
                    {
                        let action_id = id.clone();
                        let key = action_key(provider, action, &action_id);
                        rsx! {
                            ManagerButton {
                                key: "{key}",
                                variant: action_variant(action),
                                disabled: pending.contains(&key),
                                onclick: move |_| {
                                    send_action(
                                        provider,
                                        action,
                                        action_id.clone(),
                                        String::new(),
                                    );
                                },
                                {action_label(action)}
                            }
                        }
                    }
                }
            },
        }
    }
}

fn request_snapshot(refresh: bool) {
    let _ = send(&ToolsRefreshRequest { refresh });
}

fn send_action(provider: ToolProvider, action: ToolAction, id: String, value: String) {
    let _ = send(&ToolRequest {
        provider,
        action,
        id,
        value,
    });
}

fn item_matches(item: &ToolItem, query: &str) -> bool {
    query.is_empty()
        || item.name.to_ascii_lowercase().contains(query)
        || item.id.to_ascii_lowercase().contains(query)
        || item.detail.to_ascii_lowercase().contains(query)
        || provider_title(item.provider)
            .to_ascii_lowercase()
            .contains(query)
}

fn provider_title(provider: ToolProvider) -> String {
    translate(match provider {
        ToolProvider::HomebrewFormula => "tools-provider-homebrew-formulae",
        ToolProvider::HomebrewCask => "tools-provider-homebrew-casks",
        ToolProvider::Npm => "tools-provider-npm",
        ToolProvider::Acp => "tools-provider-acp-agents",
        ToolProvider::Lsp => "tools-provider-lsp-servers",
        ToolProvider::Mcp => "tools-provider-mcp-servers",
        ToolProvider::Dotfiles => "tools-provider-dotfiles",
    })
}

fn provider_short_label(provider: ToolProvider) -> String {
    match provider {
        ToolProvider::HomebrewFormula | ToolProvider::HomebrewCask => "brew".to_string(),
        ToolProvider::Npm => "NPM".to_string(),
        ToolProvider::Acp => "acp".to_string(),
        ToolProvider::Lsp => "lsp".to_string(),
        ToolProvider::Mcp => "mcp".to_string(),
        ToolProvider::Dotfiles => translate("tools-provider-dotfiles").to_lowercase(),
    }
}

fn status_label(status: ToolStatus) -> String {
    translate(match status {
        ToolStatus::Available => "tools-status-available",
        ToolStatus::Installed => "common-installed",
        ToolStatus::Outdated => "lsp-status-outdated",
        ToolStatus::Missing => "tools-status-missing",
        ToolStatus::Conflict => "tools-status-conflict",
        ToolStatus::Failed => "common-failed",
    })
}

fn status_dot_class(status: ToolStatus) -> &'static str {
    match status {
        ToolStatus::Installed => "bg-success",
        ToolStatus::Outdated => "bg-amber-500",
        ToolStatus::Conflict | ToolStatus::Failed => "bg-rose-500",
        ToolStatus::Missing => "bg-muted-foreground/40",
        ToolStatus::Available => "bg-primary/70",
    }
}

fn action_label(action: ToolAction) -> String {
    translate(match action {
        ToolAction::Install => "common-install",
        ToolAction::Update => "common-update",
        ToolAction::Uninstall => "common-uninstall",
        ToolAction::Forget => "tools-forget",
        ToolAction::Adopt => "tools-manage",
        ToolAction::Link => "tools-link",
        ToolAction::Unlink => "tools-unlink",
        ToolAction::Apply => "tools-apply",
        ToolAction::Import => "tools-import",
    })
}

fn action_result_message(result: &ToolUiOperation) -> String {
    let id = result.item_id.as_str();
    match result.action {
        ToolAction::Apply => translate("tools-result-applied"),
        ToolAction::Import => translate("tools-result-imported"),
        action => translate_with(
            match action {
                ToolAction::Install => "tools-result-installed",
                ToolAction::Update => "tools-result-updated",
                ToolAction::Uninstall => "tools-result-uninstalled",
                ToolAction::Forget => "tools-result-forgotten",
                ToolAction::Adopt => "tools-result-managed",
                ToolAction::Link => "tools-result-linked",
                ToolAction::Unlink => "tools-result-unlinked",
                ToolAction::Apply | ToolAction::Import => unreachable!(),
            },
            &[("name", TranslationValue::String(id))],
        ),
    }
}

fn action_variant(action: ToolAction) -> ManagerButtonVariant {
    match action {
        ToolAction::Install | ToolAction::Link => ManagerButtonVariant::Primary,
        ToolAction::Uninstall | ToolAction::Forget | ToolAction::Unlink => {
            ManagerButtonVariant::Danger
        }
        _ => ManagerButtonVariant::Secondary,
    }
}

fn action_key(provider: ToolProvider, action: ToolAction, id: &str) -> String {
    format!("{}:{action:?}:{id}", provider.id())
}

fn open_tool_file(path: String) {
    let _ = send(&ToolOpenRequest { path });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_routes_select_their_provider() {
        assert_eq!(ToolsRoute::from("vmux://tools/"), ToolsRoute::Acp);
        assert_eq!(ToolsRoute::from("vmux://tools/acp"), ToolsRoute::Acp);
        assert_eq!(ToolsRoute::from("vmux://tools/lsp/"), ToolsRoute::Lsp);
        assert!(ToolsRoute::Homebrew.matches(ToolProvider::HomebrewFormula));
        assert!(ToolsRoute::Homebrew.matches(ToolProvider::HomebrewCask));
        assert!(!ToolsRoute::Homebrew.matches(ToolProvider::Npm));
    }
}
