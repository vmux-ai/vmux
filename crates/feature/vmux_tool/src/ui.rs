#![allow(non_snake_case)]

use std::collections::BTreeSet;

use crate::state::{
    ToolAdoptRequest, ToolApplyRequest, ToolForgetRequest, ToolImportRequest, ToolInstallRequest,
    ToolItem, ToolLinkRequest, ToolOpenRequest, ToolOperationKey, ToolOperationKind,
    ToolOperationNotice, ToolProvider, ToolProviderMetadata, ToolStatus, ToolUninstallRequest,
    ToolUnlinkRequest, ToolUpdateRequest, ToolsFilterRequest, ToolsNavigateRequest, ToolsUiState,
};
use dioxus::prelude::*;
use vmux_ui::components::avatar::AgentThumbnail;
use vmux_ui::components::dropdown::Dropdown;
use vmux_ui::components::manager::{
    ManagerButton, ManagerButtonVariant, ManagerCard, ManagerCollection, ManagerControls,
    ManagerDetails, ManagerEmpty, ManagerList, ManagerPage, ManagerRow, ManagerSearch,
    ManagerSection, ManagerSpinner, ManagerThumbnail, ManagerToolbar, ManagerView,
    ManagerViewToggle,
};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};

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

impl ToolItem {
    fn installed(&self) -> bool {
        matches!(self.status, ToolStatus::Installed | ToolStatus::Outdated)
    }

    fn installable(&self) -> bool {
        self.operations.contains(&ToolOperationKind::Install)
    }
}

#[derive(Clone, PartialEq)]
struct ToolEntry {
    item: ToolItem,
    provider: ToolProviderMetadata,
}

#[derive(Clone, PartialEq)]
struct ToolRouteOption {
    id: String,
    label: String,
    url: String,
}

impl ToolRouteOption {
    fn all(providers: Vec<ToolProviderMetadata>) -> Vec<Self> {
        let mut options = Vec::new();
        let mut seen = BTreeSet::new();
        for provider in providers {
            if provider.route.is_empty() || !seen.insert(provider.route.clone()) {
                continue;
            }
            let route = ToolRoute::named(provider.route.clone());
            let label = provider.localized_route_title();
            options.push(Self {
                id: provider.route,
                label,
                url: route.url(),
            });
        }
        options
    }
}

#[component]
fn ToolsRouteSelector(active: String, providers: Vec<ToolProviderMetadata>) -> Element {
    let mut open = use_signal(|| false);
    let options = ToolRouteOption::all(providers);
    let selected_label = options
        .iter()
        .find(|option| option.id == active)
        .map(|option| option.label.clone())
        .unwrap_or_else(|| translate("tools-title"));
    rsx! {
        Dropdown {
            open: open(),
            class: "shrink-0".to_string(),
            on_dismiss: move |_| open.set(false),
            button {
                r#type: "button",
                class: "flex h-7 min-w-32 max-w-48 items-center gap-2 rounded-md px-1.5 text-xs font-semibold text-foreground transition-colors hover:bg-foreground/[0.045]",
                aria_expanded: open(),
                onclick: move |_| open.set(!open()),
                span { class: "min-w-0 flex-1 truncate text-left", "{selected_label}" }
                LineIconView {
                    icon: LineIcon::ChevronDown,
                    class: if open() {
                        "size-3 shrink-0 rotate-180 text-muted-foreground".to_string()
                    } else {
                        "size-3 shrink-0 text-muted-foreground".to_string()
                    },
                }
            }
            if open() {
                div { class: "absolute left-0 top-8 z-30 min-w-full w-48 rounded-md bg-background p-1 shadow-xl ring-1 ring-inset ring-foreground/[0.12]",
                    for option in options {
                        button {
                            key: "{option.id}",
                            r#type: "button",
                            class: if option.id == active {
                                "flex w-full items-center gap-2 rounded-md bg-foreground/[0.07] px-2 py-1.5 text-left text-[11px] text-foreground"
                            } else {
                                "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11px] text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground"
                            },
                            onclick: move |_| {
                                open.set(false);
                                let _ = send(&ToolsNavigateRequest { url: option.url.clone() });
                            },
                            span { class: "min-w-0 flex-1 truncate", "{option.label}" }
                            if option.id == active {
                                LineIconView { icon: LineIcon::Check, class: "size-3 shrink-0".to_string() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ToolThumbnail(item: ToolItem, provider: ToolProviderMetadata, class: String) -> Element {
    if provider.provider.is("acp") {
        return rsx! {
            AgentThumbnail {
                src: item.icon,
                seed: item.id,
                background: String::new(),
                alt: item.name,
                class,
            }
        };
    }
    rsx! {
        ManagerThumbnail { src: item.icon, fallback: provider.short_label }
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
    let selection = selected();
    let mut installed = Vec::new();
    let mut available = Vec::new();
    for category in &view.categories {
        let Some(provider) = snapshot.provider(&category.provider) else {
            continue;
        };
        for item in &category.items {
            let entry = ToolEntry {
                item: item.clone(),
                provider: provider.clone(),
            };
            if item.installed() {
                installed.push(entry);
            } else {
                available.push(entry);
            }
        }
    }
    let selected_tool = selection.as_ref().and_then(|selection| {
        view.categories.iter().find_map(|category| {
            let item = category.items.iter().find(|item| selection.matches(item))?;
            let provider = snapshot.provider(&category.provider)?;
            Some((item.clone(), provider.clone()))
        })
    });
    rsx! {
        ManagerPage {
            ManagerToolbar {
                ToolsRouteSelector {
                    active: view.route.clone(),
                    providers: snapshot.providers.clone(),
                }
                ManagerSearch {
                    value: view.query.clone(),
                    placeholder: translate("tools-search"),
                    onsearch: move |event: FormEvent| {
                        let _ = send(&ToolsFilterRequest { query: event.value() });
                    },
                    onkeydown: None,
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
            }
            div { class: "flex min-h-0 flex-1 overflow-hidden bg-background/45",
                ManagerList {
                    if snapshot.loaded && view.visible_count > 0 {
                        ManagerControls {
                            ManagerViewToggle {
                                view: manager_view(),
                                list_label: translate("tools-view-list"),
                                grid_label: translate("tools-view-grid"),
                                onchange: move |view| manager_view.set(view),
                            }
                        }
                    }
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
                        if !installed.is_empty() {
                            ToolSection {
                                label: translate("common-installed"),
                                entries: installed,
                                view: manager_view(),
                                selection: selection.clone(),
                                pending: pending.clone(),
                                onselect: move |selection| selected.set(Some(selection)),
                            }
                        }
                        if !available.is_empty() {
                            ToolSection {
                                label: translate("tools-status-available"),
                                entries: available,
                                view: manager_view(),
                                selection,
                                pending: pending.clone(),
                                onselect: move |selection| selected.set(Some(selection)),
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
fn ToolSection(
    label: String,
    entries: Vec<ToolEntry>,
    view: ManagerView,
    selection: Option<ToolSelection>,
    pending: BTreeSet<ToolOperationKey>,
    onselect: EventHandler<ToolSelection>,
) -> Element {
    let count = entries.len();
    rsx! {
        ManagerSection { label, count,
            ManagerCollection { view,
                for entry in entries {
                    if view == ManagerView::Grid {
                        ToolCard {
                            key: "{entry.provider.provider.id()}:{entry.item.id}",
                            item: entry.item.clone(),
                            provider: entry.provider,
                            selected: selection.as_ref().is_some_and(|selected| selected.matches(&entry.item)),
                            pending: pending.clone(),
                            onselect,
                        }
                    } else {
                        ToolRow {
                            key: "{entry.provider.provider.id()}:{entry.item.id}",
                            item: entry.item.clone(),
                            provider: entry.provider,
                            selected: selection.as_ref().is_some_and(|selected| selected.matches(&entry.item)),
                            pending: pending.clone(),
                            onselect,
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
    selected: bool,
    pending: BTreeSet<ToolOperationKey>,
    onselect: EventHandler<ToolSelection>,
) -> Element {
    let version = item.version.clone().unwrap_or_default();
    let show_icon = provider.thumbnails;
    let selection = ToolSelection::of(&item);
    rsx! {
        ManagerRow {
            show_icon,
            icon_surface: !provider.provider.is("acp"),
            selected,
            onselect: move |_| onselect.call(selection.clone()),
            icon: rsx! {
                ToolThumbnail {
                    item: item.clone(),
                    provider: provider.clone(),
                    class: "size-7 text-[8px]".to_string(),
                }
            },
            title: item.name.clone(),
            subtitle: version,
            meta: rsx! {
                ToolMeta { item: item.clone(), provider: provider.clone() }
            },
            actions: rsx! {
                ToolQuickInstall { item: item.clone(), pending }
            },
        }
    }
}

#[component]
fn ToolCard(
    item: ToolItem,
    provider: ToolProviderMetadata,
    selected: bool,
    pending: BTreeSet<ToolOperationKey>,
    onselect: EventHandler<ToolSelection>,
) -> Element {
    let version = item.version.clone().unwrap_or_default();
    let selection = ToolSelection::of(&item);
    rsx! {
        ManagerCard {
            show_icon: provider.thumbnails,
            icon_surface: !provider.provider.is("acp"),
            selected,
            onselect: move |_| onselect.call(selection.clone()),
            icon: rsx! {
                ToolThumbnail {
                    item: item.clone(),
                    provider: provider.clone(),
                    class: "size-10 text-[10px]".to_string(),
                }
            },
            title: item.name.clone(),
            subtitle: version,
            actions: rsx! {
                ToolQuickInstall { item: item.clone(), pending }
            },
            meta: rsx! {
                ToolMeta { item: item.clone(), provider }
            },
        }
    }
}

#[component]
fn ToolQuickInstall(item: ToolItem, pending: BTreeSet<ToolOperationKey>) -> Element {
    if !item.installable() {
        return rsx! {};
    }
    let operation = ToolOperationKey::new(
        item.provider.clone(),
        ToolOperationKind::Install,
        item.id.clone(),
    );
    let disabled = pending.contains(&operation);
    let provider = item.provider;
    let id = item.id;
    rsx! {
        button {
            r#type: "button",
            class: "flex size-6 shrink-0 items-center justify-center rounded-md bg-foreground/[0.04] text-muted-foreground/70 opacity-60 transition-[background-color,color,opacity] hover:bg-foreground/[0.10] hover:text-foreground hover:opacity-100 disabled:pointer-events-none disabled:opacity-25",
            title: translate("common-install"),
            aria_label: translate("common-install"),
            disabled,
            onclick: move |event| {
                event.stop_propagation();
                send_operation(
                    provider.clone(),
                    ToolOperationKind::Install,
                    id.clone(),
                    String::new(),
                );
            },
            LineIconView { icon: LineIcon::Plus, class: "size-3.5".to_string() }
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
        ManagerDetails {
            show_icon: provider.thumbnails,
            icon: rsx! {
                ToolThumbnail {
                    item: item.clone(),
                    provider: provider.clone(),
                    class: "size-9 text-[9px]".to_string(),
                }
            },
            title: item.name.clone(),
            subtitle: version,
            onclose,
            actions: rsx! {
                if !item.operations.is_empty() {
                    ToolActions { item: item.clone(), pending }
                }
            },
            div {
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
