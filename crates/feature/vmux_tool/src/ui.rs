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
    ManagerRow, ManagerSpinner, ManagerTab, ManagerTabs, ManagerThumbnail, ManagerView,
    ManagerViewToggle,
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

#[derive(Clone, PartialEq, Eq)]
struct ToolSelection {
    provider: ToolProvider,
    id: String,
}

impl ToolSelection {
    fn of(item: &ToolItem) -> Self {
        Self {
            provider: item.provider.clone(),
            id: item.id.clone(),
        }
    }

    fn matches(&self, item: &ToolItem) -> bool {
        self.provider == item.provider && self.id == item.id
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
            content_class: "mx-auto flex w-full max-w-5xl items-center".to_string(),
            onselect: move |url: String| {
                let _ = send(&ToolsNavigateRequest { url });
            },
        }
    }
}

#[component]
fn ToolManager(state: ToolsUiState) -> Element {
    let mut manager_view = use_signal(ManagerView::default);
    let mut selected = use_signal(|| None::<ToolSelection>);
    let current = state;
    let pending = current.pending.into_iter().collect::<BTreeSet<_>>();
    let notice = current.notice;
    let snapshot = &current.snapshot;
    let view = &current.view;
    let active_route = view.route.clone();
    use_effect(use_reactive!(|active_route| {
        let _ = active_route;
        selected.set(None);
    }));
    let apply_provider = view.apply_provider.clone();
    let route_title = if view.route_title_message_id.is_empty() {
        view.route_title.clone()
    } else {
        translate(&view.route_title_message_id)
    };
    let selection = selected();
    let selected_tool = selection.as_ref().and_then(|selection| {
        view.categories.iter().find_map(|category| {
            let item = category.items.iter().find(|item| selection.matches(item))?;
            let provider = snapshot.provider(&category.provider)?;
            Some((item.clone(), provider.clone()))
        })
    });
    rsx! {
        ManagerPage {
            ToolsManagerTabs { active: view.route.clone(), providers: snapshot.providers.clone() }
            ManagerHeader {
                content_class: "mx-auto w-full max-w-5xl".to_string(),
                title: route_title,
                count: view.visible_count as usize,
                search_value: view.query.clone(),
                search_placeholder: translate("tools-search"),
                onsearch: move |event: FormEvent| {
                    let _ = send(&ToolsFilterRequest { query: event.value() });
                },
                onkeydown: None,
                actions: rsx! {
                    if snapshot.loaded && view.visible_count > 0 {
                        ManagerViewToggle {
                            view: manager_view(),
                            list_label: translate("tools-view-list"),
                            grid_label: translate("tools-view-grid"),
                            onchange: move |view| manager_view.set(view),
                        }
                    }
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
            div { class: "flex min-h-0 flex-1 overflow-hidden bg-background/45",
                ManagerList {
                    class: "mx-auto flex w-full max-w-5xl flex-col gap-3".to_string(),
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
                                div { class: if manager_view() == ManagerView::Grid {
                                        "grid grid-cols-1 gap-2 sm:grid-cols-2 xl:grid-cols-3"
                                    } else {
                                        "flex flex-col gap-2"
                                    },
                                    for item in category.items.iter() {
                                        if manager_view() == ManagerView::Grid {
                                            ToolCard {
                                                key: "{category.provider.id()}:{item.id}",
                                                item: item.clone(),
                                                provider: provider.clone(),
                                                selected: selection.as_ref().is_some_and(|selected| selected.matches(item)),
                                                onselect: move |selection| selected.set(Some(selection)),
                                            }
                                        } else {
                                            ToolRow {
                                                key: "{category.provider.id()}:{item.id}",
                                                item: item.clone(),
                                                provider: provider.clone(),
                                                selected: selection.as_ref().is_some_and(|selected| selected.matches(item)),
                                                onselect: move |selection| selected.set(Some(selection)),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some((item, provider)) = selected_tool {
                    ToolDetails {
                        item,
                        provider,
                        pending: pending.clone(),
                        onclose: move |_| selected.set(None),
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
    selected: bool,
    onselect: EventHandler<ToolSelection>,
) -> Element {
    let version = item.version.clone().unwrap_or_default();
    let show_icon = provider.thumbnails;
    let selection = ToolSelection::of(&item);
    rsx! {
        ManagerRow {
            show_icon,
            selected,
            onselect: move |_| onselect.call(selection.clone()),
            icon: rsx! {
                ManagerThumbnail { src: item.icon.clone(), fallback: "ACP".to_string() }
            },
            title: item.name.clone(),
            subtitle: version,
            meta: rsx! {
                ToolMeta { item: item.clone(), provider: provider.clone() }
            },
            actions: rsx! {},
        }
    }
}

#[component]
fn ToolCard(
    item: ToolItem,
    provider: ToolProviderMetadata,
    selected: bool,
    onselect: EventHandler<ToolSelection>,
) -> Element {
    let version = item.version.clone().unwrap_or_default();
    let selection = ToolSelection::of(&item);
    let keyboard_selection = selection.clone();
    rsx! {
        div {
            class: if selected {
                "group flex min-h-36 min-w-0 cursor-pointer flex-col rounded-xl bg-primary/[0.09] p-3.5 shadow-[inset_2px_0_0_var(--primary)] transition-colors"
            } else {
                "group flex min-h-36 min-w-0 cursor-pointer flex-col rounded-xl bg-foreground/[0.03] p-3.5 transition-colors hover:bg-foreground/[0.06]"
            },
            role: "button",
            tabindex: "0",
            aria_selected: selected,
            onclick: move |_| onselect.call(selection.clone()),
            onkeydown: move |event| {
                let activate = event.key() == Key::Enter
                    || matches!(event.key(), Key::Character(key) if key == " ");
                if !activate {
                    return;
                }
                event.prevent_default();
                onselect.call(keyboard_selection.clone());
            },
            div { class: "flex min-w-0 items-start gap-3",
                if provider.thumbnails {
                    div { class: "flex size-10 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.065]",
                        ManagerThumbnail { src: item.icon.clone(), fallback: provider.short_label.clone() }
                    }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "truncate font-medium text-foreground/95", "{item.name}" }
                    if !version.is_empty() {
                        div { class: "mt-0.5 truncate text-xs text-muted-foreground/70", "{version}" }
                    }
                }
            }
            div { class: "mt-3 flex min-w-0 flex-wrap items-center gap-1.5",
                ToolMeta { item: item.clone(), provider }
            }
        }
    }
}

#[component]
fn ToolDetails(
    item: ToolItem,
    provider: ToolProviderMetadata,
    pending: BTreeSet<ToolOperationKey>,
    onclose: EventHandler<MouseEvent>,
) -> Element {
    let version = item.version.clone().unwrap_or_default();
    rsx! {
        aside { class: "flex min-h-0 w-[21rem] shrink-0 flex-col bg-foreground/[0.018]",
            header { class: "flex shrink-0 items-start gap-3 p-4",
                if provider.thumbnails {
                    div { class: "flex size-9 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.065]",
                        ManagerThumbnail { src: item.icon.clone(), fallback: provider.short_label.clone() }
                    }
                }
                div { class: "min-w-0 flex-1",
                    h2 { class: "truncate text-sm font-semibold text-foreground", "{item.name}" }
                    if !version.is_empty() {
                        div { class: "mt-0.5 truncate text-xs text-muted-foreground/70", "{version}" }
                    }
                }
                button {
                    r#type: "button",
                    class: "flex size-7 shrink-0 items-center justify-center rounded-md text-lg leading-none text-muted-foreground hover:bg-foreground/[0.07] hover:text-foreground",
                    aria_label: translate("common-close"),
                    onclick: move |event| onclose.call(event),
                    "×"
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-4 pb-4",
                if !item.detail.is_empty() {
                    p { class: "mb-4 whitespace-pre-wrap text-xs leading-5 text-muted-foreground", "{item.detail}" }
                }
                section { class: "rounded-lg bg-foreground/[0.035] p-3",
                    div { class: "flex flex-wrap items-center gap-1.5",
                        ToolMeta { item: item.clone(), provider: provider.clone() }
                    }
                    div { class: "mt-3 break-all font-mono text-[10px] text-muted-foreground/65", "{item.id}" }
                    div { class: "mt-2 text-[10px] text-muted-foreground/60", {provider.localized_title()} }
                }
            }
            if !item.operations.is_empty() {
                footer { class: "flex shrink-0 flex-wrap items-center justify-end gap-2 px-4 pb-4 pt-2",
                    ToolActions { item, pending }
                }
            }
        }
    }
}

#[component]
fn ToolMeta(item: ToolItem, provider: ToolProviderMetadata) -> Element {
    rsx! {
        span { class: "shrink-0 text-[10px] text-muted-foreground/60", "{provider.short_label}" }
        if item.managed {
            span { class: "shrink-0 text-[10px] text-muted-foreground/60", {format!("· {}", translate("tools-managed"))} }
        }
        span { class: "flex shrink-0 items-center gap-1 text-[10px] text-muted-foreground/70",
            span { class: "size-1.5 rounded-full {status_dot_class(item.status)}" }
            {status_label(item.status)}
        }
    }
}

#[component]
fn ToolActions(item: ToolItem, pending: BTreeSet<ToolOperationKey>) -> Element {
    let provider_id = item.provider.clone();
    let id = item.id.clone();
    rsx! {
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
