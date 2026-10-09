#![allow(non_snake_case)]

use std::collections::BTreeSet;

use crate::state::{
    ToolAdoptRequest, ToolApplyRequest, ToolForgetRequest, ToolImportRequest, ToolInstallRequest,
    ToolItem, ToolLinkRequest, ToolOpenRequest, ToolOperationKey, ToolOperationKind,
    ToolOperationNotice, ToolProvider, ToolProviderMetadata, ToolStatus, ToolUninstallRequest,
    ToolUnlinkRequest, ToolUpdateRequest, ToolsFilterRequest, ToolsNavigateRequest,
    ToolsRefreshRequest, ToolsUiState,
};
use dioxus::prelude::*;
use vmux_ui::components::manager::{
    ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList, ManagerPage,
    ManagerRow, ManagerSpinner, ManagerTab, ManagerTabs, ManagerThumbnail,
};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

use crate::route::ToolRoute;

#[vmux_page::page(
    component = Page,
    states = ["ThemeUiState", "ToolsUiState"],
    subtree
)]
pub struct ToolsPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = use_ui_state::<ToolsUiState>().state;
    rsx! { ToolManager { state: state() } }
}

impl ToolProviderMetadata {
    fn localized_title(&self) -> String {
        if self.title_message_id.is_empty() {
            self.title.clone()
        } else {
            translate(&self.title_message_id)
        }
    }

    fn localized_route_title(&self) -> String {
        if self.route_title_message_id.is_empty() {
            self.route_title.clone()
        } else {
            translate(&self.route_title_message_id)
        }
    }
}

#[component]
fn ToolsManagerTabs(active: String, providers: Vec<ToolProviderMetadata>) -> Element {
    let mut seen = BTreeSet::new();
    let mut tabs = Vec::new();
    for provider in providers {
        if provider.route.is_empty() || !seen.insert(provider.route.clone()) {
            continue;
        }
        let route = ToolRoute::named(provider.route.clone());
        let label = provider.localized_route_title();
        tabs.push(ManagerTab {
            id: provider.route,
            label,
            href: route.url(),
        });
    }
    rsx! {
        ManagerTabs {
            active,
            tabs,
            onselect: move |url: String| {
                let _ = send(&ToolsNavigateRequest { url });
            },
        }
    }
}

#[component]
fn ToolManager(state: ToolsUiState) -> Element {
    let current = state;
    let pending = current.pending.into_iter().collect::<BTreeSet<_>>();
    let notice = current.notice;
    let snapshot = &current.snapshot;
    let view = &current.view;
    let apply_provider = view.apply_provider.clone();
    let route_title = if view.route_title_message_id.is_empty() {
        view.route_title.clone()
    } else {
        translate(&view.route_title_message_id)
    };
    rsx! {
        ManagerPage {
            ToolsManagerTabs { active: view.route.clone(), providers: snapshot.providers.clone() }
            ManagerHeader {
                title: route_title,
                count: view.visible_count as usize,
                search_value: view.query.clone(),
                search_placeholder: translate("tools-search"),
                onsearch: move |event: FormEvent| {
                    let _ = send(&ToolsFilterRequest { query: event.value() });
                },
                onkeydown: None,
                actions: rsx! {
                    if let Some(provider) = apply_provider {
                        ManagerButton {
                            variant: ManagerButtonVariant::Secondary,
                            disabled: pending.contains(&ToolOperationKey::new(
                                provider.clone(),
                                ToolOperationKind::Apply,
                                "",
                            )),
                            onclick: move |_| {
                                send_operation(
                                    provider.clone(),
                                    ToolOperationKind::Apply,
                                    String::new(),
                                    String::new(),
                                );
                            },
                            {translate("tools-apply")}
                        }
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
                if view.show_brewfile {
                    HomebrewSourceCard {
                        root: snapshot.root.clone(),
                    }
                }
                if let Some(result) = notice {
                    {
                        rsx! {
                    div {
                        class: if result.success {
                            "rounded-xl bg-success/10 px-4 py-3 text-xs text-success ring-1 ring-inset ring-success/20"
                        } else {
                            "rounded-xl bg-ansi-1/10 px-4 py-3 text-xs text-ansi-1 ring-1 ring-inset ring-ansi-1/20"
                        },
                        if result.success {
                            {operation_result_message(&result)}
                        } else {
                            "{result.message}"
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
                } else if view.visible_count == 0 {
                    ManagerEmpty {
                        title: translate("tools-empty"),
                        detail: translate("tools-empty-detail"),
                    }
                } else {
                    for category in view.categories.iter() {
                        if let Some(provider) = snapshot.provider(&category.provider) {
                            div { class: "mt-3 flex items-center gap-2 px-1 first:mt-0",
                                h2 { class: "text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground", {provider.localized_title()} }
                                span { class: "text-[10px] text-muted-foreground/60",
                                    "{category.items.len()}"
                                }
                            }
                            for item in category.items.iter() {
                                ToolRow { key: "{category.provider.id()}:{item.id}", item: item.clone(), provider: provider.clone(), pending: pending.clone() }
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
fn ToolRow(
    item: ToolItem,
    provider: ToolProviderMetadata,
    pending: BTreeSet<ToolOperationKey>,
) -> Element {
    let version = item.version.clone().unwrap_or_default();
    let provider_id = item.provider.clone();
    let id = item.id.clone();
    let show_icon = provider.thumbnails;
    rsx! {
        ManagerRow {
            show_icon,
            icon: rsx! {
                ManagerThumbnail { src: item.icon.clone(), fallback: "ACP".to_string() }
            },
            title: item.name.clone(),
            subtitle: version,
            meta: rsx! {
                span { class: "shrink-0 text-[10px] text-muted-foreground/60", "{provider.short_label}" }
                if item.managed {
                    span { class: "shrink-0 text-[10px] text-muted-foreground/60", {format!("· {}", translate("tools-managed"))} }
                }
                span { class: "flex shrink-0 items-center gap-1 text-[10px] text-muted-foreground/70",
                    span { class: "size-1.5 rounded-full {status_dot_class(item.status)}" }
                    {status_label(item.status)}
                }
            },
            actions: rsx! {
                for kind in item.operations.iter().copied() {
                    {
                        let operation_id = id.clone();
                        let operation_provider = provider_id.clone();
                        let operation = ToolOperationKey::new(operation_provider.clone(), kind, operation_id.clone());
                        let key = format!("{}:{kind:?}:{operation_id}", provider_id.id());
                        rsx! {
                            ManagerButton {
                                key: "{key}",
                                variant: operation_variant(kind),
                                disabled: pending.contains(&operation),
                                onclick: move |_| {
                                    send_operation(
                                        operation_provider.clone(),
                                        kind,
                                        operation_id.clone(),
                                        String::new(),
                                    );
                                },
                                {operation_label(kind)}
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

fn send_operation(provider: ToolProvider, kind: ToolOperationKind, id: String, value: String) {
    let _ = match kind {
        ToolOperationKind::Install => send(&ToolInstallRequest { provider, id }),
        ToolOperationKind::Update => send(&ToolUpdateRequest { provider, id }),
        ToolOperationKind::Uninstall => send(&ToolUninstallRequest { provider, id }),
        ToolOperationKind::Forget => send(&ToolForgetRequest { provider, id }),
        ToolOperationKind::Adopt => send(&ToolAdoptRequest {
            provider,
            id,
            value,
        }),
        ToolOperationKind::Link => send(&ToolLinkRequest { provider, id }),
        ToolOperationKind::Unlink => send(&ToolUnlinkRequest { provider, id }),
        ToolOperationKind::Apply => send(&ToolApplyRequest { provider }),
        ToolOperationKind::Import => send(&ToolImportRequest { provider, value }),
    };
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

fn operation_label(kind: ToolOperationKind) -> String {
    translate(match kind {
        ToolOperationKind::Install => "common-install",
        ToolOperationKind::Update => "common-update",
        ToolOperationKind::Uninstall => "common-uninstall",
        ToolOperationKind::Forget => "tools-forget",
        ToolOperationKind::Adopt => "tools-manage",
        ToolOperationKind::Link => "tools-link",
        ToolOperationKind::Unlink => "tools-unlink",
        ToolOperationKind::Apply => "tools-apply",
        ToolOperationKind::Import => "tools-import",
    })
}

fn operation_result_message(result: &ToolOperationNotice) -> String {
    let id = result.operation.item_id.as_str();
    match result.operation.kind {
        ToolOperationKind::Apply => translate("tools-result-applied"),
        ToolOperationKind::Import => translate("tools-result-imported"),
        kind => translate_with(
            match kind {
                ToolOperationKind::Install => "tools-result-installed",
                ToolOperationKind::Update => "tools-result-updated",
                ToolOperationKind::Uninstall => "tools-result-uninstalled",
                ToolOperationKind::Forget => "tools-result-forgotten",
                ToolOperationKind::Adopt => "tools-result-managed",
                ToolOperationKind::Link => "tools-result-linked",
                ToolOperationKind::Unlink => "tools-result-unlinked",
                ToolOperationKind::Apply | ToolOperationKind::Import => unreachable!(),
            },
            &[("name", TranslationValue::String(id))],
        ),
    }
}

fn operation_variant(kind: ToolOperationKind) -> ManagerButtonVariant {
    match kind {
        ToolOperationKind::Install | ToolOperationKind::Link => ManagerButtonVariant::Primary,
        ToolOperationKind::Uninstall | ToolOperationKind::Forget | ToolOperationKind::Unlink => {
            ManagerButtonVariant::Danger
        }
        _ => ManagerButtonVariant::Secondary,
    }
}

fn open_tool_file(path: String) {
    let _ = send(&ToolOpenRequest { path });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_routes_are_data_driven() {
        assert_eq!(ToolRoute::from("vmux://tools/").id(), "acp");
        assert_eq!(ToolRoute::from("vmux://tools/acp").id(), "acp");
        assert_eq!(ToolRoute::from("vmux://tools/lsp/").id(), "lsp");
    }
}
