#![allow(non_snake_case)]

use std::collections::BTreeSet;

use crate::extension::ExtensionManager;
use crate::state::{
    McpRegisterRequest, ToolAdoptRequest, ToolApplyRequest, ToolForgetRequest, ToolImportRequest,
    ToolInstallRequest, ToolItem, ToolLinkRequest, ToolOpenRequest, ToolOperationKey,
    ToolOperationKind, ToolOperationNotice, ToolProvider, ToolProviderMetadata, ToolStatus,
    ToolUninstallRequest, ToolUnlinkRequest, ToolUpdateRequest, ToolsFilterRequest, ToolsUiState,
};
use dioxus::prelude::*;
use vmux_api::address::{AddressCrumb, AddressIntent, AddressOption, PageAddress};
use vmux_api::extension::ExtensionsUiState;
use vmux_ui::components::avatar::AgentThumbnail;
use vmux_ui::components::breadcrumb::{BreadcrumbOption, BreadcrumbOptionIcon};
use vmux_ui::components::manager::{
    ManagerButton, ManagerButtonVariant, ManagerCard, ManagerCollection, ManagerDetails,
    ManagerEmpty, ManagerList, ManagerPage, ManagerRow, ManagerSection, ManagerSidebarItem,
    ManagerSpinner, ManagerThumbnail, ManagerView, WorkspaceLayout,
};
use vmux_ui::components::page_address::{PageBreadcrumbs, PageToolbar, ToolbarControl};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};
use vmux_ui::navigation::PageNavigation;

use crate::route::ToolRoute;

#[vmux_page::page(
    component = Page,
    states = [vmux_api::address::PageAddressUiState, vmux_ui::theme::ThemeUiState, crate::state::ToolsUiState, vmux_api::extension::ExtensionsUiState],
    subtree
)]
pub struct ToolsPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut manager_view = use_signal(ManagerView::default);
    let tools = use_ui_state::<ToolsUiState>().state;
    let extensions = use_ui_state::<ExtensionsUiState>().state;
    let state = tools();
    let extension_state = extensions();
    let route = if state.view.route.is_empty() {
        try_consume_context::<vmux_ecs::PageMetadata>()
            .and_then(|metadata| ToolRoute::from_metadata(&metadata))
            .unwrap_or_else(|| ToolRoute::named("acp"))
            .id()
            .to_string()
    } else {
        state.view.route.clone()
    };
    let title = ToolRouteOption::selected_label(&route, state.snapshot.providers.clone());
    let mut toolbar = Vec::new();
    if route != "extensions" {
        if state.snapshot.loaded && state.view.visible_count > 0 {
            toolbar.extend(ToolbarControl::view(
                manager_view(),
                translate("tools-view-list"),
                translate("tools-view-grid"),
                move |view| manager_view.set(view),
            ));
        }
        if let Some(provider) = state.view.apply_provider.clone() {
            let pending = state.pending.contains(&ToolOperationKey::new(
                provider.clone(),
                ToolOperationKind::Apply,
                "",
            ));
            toolbar.push(
                ToolbarControl::button(translate("tools-apply"), move |_| {
                    ToolOperationKind::Apply.send(provider.clone(), String::new(), String::new());
                })
                .disabled(pending),
            );
        }
    }
    rsx! {
        document::Title { "{title}" }
        ManagerPage {
            ToolsHeader { active: route.clone(), providers: state.snapshot.providers.clone() }
            WorkspaceLayout {
                search: rsx! { ToolsSearch { active: route.clone(), state: state.clone(), extensions: extension_state.clone() } },
                sidebar: rsx! { ToolsSidebar { active: route.clone(), state: state.clone(), extensions: extension_state.clone() } },
                toolbar: rsx! { if !toolbar.is_empty() { PageToolbar { controls: toolbar } } },
                if route == "extensions" {
                    ExtensionManager { state: extension_state }
                } else {
                    ToolManager { state, manager_view }
                }
            }
        }
    }
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
    icon_url: String,
    icon: vmux_api::BuiltinIcon,
}

impl From<ToolRouteOption> for BreadcrumbOption {
    fn from(option: ToolRouteOption) -> Self {
        Self {
            label: option.label,
            value: option.url,
            identity_seed: String::new(),
            icon_url: option.icon_url,
            icon: Some(option.icon),
        }
    }
}

impl ToolRouteOption {
    fn fallback(id: String) -> Self {
        let label_key = match id.as_str() {
            "acp" => "tools-provider-acp-agents",
            "lsp" => "tools-provider-lsp-servers",
            "homebrew" => "tools-homebrew",
            "npm" => "tools-provider-npm",
            "mcp" => "tools-provider-mcp-servers",
            "dotfiles" => "tools-provider-dotfiles",
            "extensions" => "extensions-title",
            _ => "tools-title",
        };
        Self {
            label: translate(label_key),
            url: ToolRoute::named(id.clone()).url(),
            icon_url: Self::icon_url(&id),
            icon: Self::icon(&id),
            id,
        }
    }

    fn address(active: &str, providers: Vec<ToolProviderMetadata>) -> PageAddress {
        let options = Self::all(providers);
        let selected = options
            .iter()
            .find(|option| option.id == active)
            .cloned()
            .unwrap_or_else(|| Self::fallback(active.to_string()));
        PageAddress {
            current: AddressOption {
                label_key: "tools-title".into(),
                icon: Some(vmux_api::BuiltinIcon::Toolbox),
                intent: Some(AddressIntent::Navigate {
                    url: crate::ToolPlugin::url().to_string(),
                }),
                ..Default::default()
            },
            fixed: vec![AddressCrumb {
                entry: BreadcrumbOption::from(selected).address(),
                options: options
                    .into_iter()
                    .map(|option| BreadcrumbOption::from(option).address())
                    .collect(),
                active: true,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn count(&self, snapshot: &crate::state::ToolsSnapshot) -> usize {
        let mut count = 0;
        for category in &snapshot.categories {
            if snapshot
                .provider(&category.provider)
                .is_some_and(|provider| provider.route == self.id)
            {
                count += category.items.len();
            }
        }
        count
    }

    fn icon_url(route: &str) -> String {
        match route {
            "acp" => "assets/tool-logos/acp.png",
            "lsp" => "assets/favicons/lsp.svg",
            "homebrew" => "assets/tool-logos/homebrew.svg",
            "npm" => "assets/tool-logos/npm.svg",
            "mcp" => "assets/tool-logos/mcp.png",
            "dotfiles" => "assets/favicons/files.svg",
            "extensions" => "assets/favicons/extensions.svg",
            _ => "assets/favicons/tools.svg",
        }
        .to_string()
    }

    fn icon(route: &str) -> vmux_api::BuiltinIcon {
        match route {
            "acp" => vmux_api::BuiltinIcon::Sparkles,
            "lsp" => vmux_api::BuiltinIcon::Brain,
            "homebrew" => vmux_api::BuiltinIcon::Hammer,
            "npm" => vmux_api::BuiltinIcon::Layers,
            "mcp" => vmux_api::BuiltinIcon::Server,
            "dotfiles" => vmux_api::BuiltinIcon::Files,
            "extensions" => vmux_api::BuiltinIcon::Puzzle,
            _ => vmux_api::BuiltinIcon::Toolbox,
        }
    }

    fn all(providers: Vec<ToolProviderMetadata>) -> Vec<Self> {
        let mut options = Vec::new();
        let mut seen = BTreeSet::new();
        for provider in providers {
            if provider.route.is_empty() || !seen.insert(provider.route.clone()) {
                continue;
            }
            let route = ToolRoute::named(provider.route.clone());
            let label = provider.localized_route_title();
            let icon_url = Self::icon_url(&provider.route);
            let icon = Self::icon(&provider.route);
            options.push(Self {
                id: provider.route,
                label,
                url: route.url(),
                icon_url,
                icon,
            });
        }
        options.push(Self {
            id: "extensions".to_string(),
            label: translate("extensions-title"),
            url: crate::EXTENSIONS_URL.to_string(),
            icon_url: Self::icon_url("extensions"),
            icon: Self::icon("extensions"),
        });
        options.sort_by(|left, right| {
            left.label
                .to_ascii_lowercase()
                .cmp(&right.label.to_ascii_lowercase())
        });
        options
    }

    fn selected_label(active: &str, providers: Vec<ToolProviderMetadata>) -> String {
        Self::all(providers)
            .into_iter()
            .find(|option| option.id == active)
            .map(|option| option.label)
            .unwrap_or_else(|| Self::fallback(active.to_string()).label)
    }
}

#[component]
fn ToolsHeader(active: String, providers: Vec<ToolProviderMetadata>) -> Element {
    rsx! { PageBreadcrumbs { state: ToolRouteOption::address(&active, providers) } }
}

#[component]
fn ToolsSearch(active: String, state: ToolsUiState, extensions: ExtensionsUiState) -> Element {
    let extension_search = active == "extensions";
    let query = if extension_search {
        extensions.query.clone()
    } else {
        state.view.query.clone()
    };
    let browse_query = query.clone();
    rsx! {
        vmux_ui::components::manager::ManagerSearchRow {
            vmux_ui::components::manager::ManagerSearch {
                value: query,
                placeholder: if extension_search { translate("extensions-search") } else { translate("tools-search") },
                onsearch: move |query| {
                    if extension_search {
                        let _ = send(&vmux_api::extension::ExtFilterRequest { query });
                    } else {
                        let _ = send(&ToolsFilterRequest { query });
                    }
                },
                onkeydown: Some(EventHandler::new(move |event: KeyboardEvent| {
                    if extension_search && event.key() == Key::Enter && !browse_query.trim().is_empty() {
                        let _ = send(&vmux_api::extension::ExtBrowseStoreRequest { query: browse_query.clone() });
                    }
                })),
            }
        }
    }
}

#[component]
fn ToolsSidebar(active: String, state: ToolsUiState, extensions: ExtensionsUiState) -> Element {
    let options = ToolRouteOption::all(state.snapshot.providers.clone());
    rsx! {
        nav { class: "flex flex-col gap-0.5",
            for option in options {
                ManagerSidebarItem {
                    key: "{option.id}",
                    label: option.label.clone(), selected: option.id == active,
                    count: if option.id == "extensions" { extensions.extensions.len() } else { option.count(&state.snapshot) },
                    leading: rsx! { BreadcrumbOptionIcon {
                        option: BreadcrumbOption::from(option.clone()),
                        class: "size-4 shrink-0".to_string(),
                    } },
                    onclick: move |event| PageNavigation::click(option.url.clone(), &event),
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
fn ToolManager(state: ToolsUiState, manager_view: Signal<ManagerView>) -> Element {
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
        div { class: "flex min-h-0 flex-1 overflow-hidden bg-background",
            ManagerList {
                    if view.show_brewfile {
                        HomebrewSourceCard {
                            root: snapshot.root.clone(),
                        }
                    }
                    if !view.mcp_config.is_empty() {
                        McpRegistryCard {
                            config_path: view.mcp_config.clone(),
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
                                {result.result_message()}
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
fn McpRegistryCard(config_path: String) -> Element {
    let mut id = use_signal(String::new);
    let mut transport = use_signal(|| "http".to_string());
    let mut target = use_signal(String::new);
    let mut args = use_signal(String::new);
    let mut bearer_env = use_signal(String::new);
    let mut oauth = use_signal(|| false);
    let open_path = config_path.clone();
    let is_stdio = transport() == "stdio";
    let can_submit = !id().trim().is_empty() && !target().trim().is_empty();
    rsx! {
        div { class: "flex flex-col gap-3 rounded-2xl bg-foreground/[0.035] p-4 ring-1 ring-inset ring-foreground/10",
            div { class: "flex items-center gap-3",
                div { class: "min-w-0 flex-1",
                    div { class: "font-medium text-foreground/95", {translate("tools-provider-mcp-servers")} }
                    div { class: "truncate font-mono text-xs text-muted-foreground/70", "{config_path}" }
                }
                ManagerButton {
                    variant: ManagerButtonVariant::Secondary,
                    disabled: config_path.is_empty(),
                    onclick: move |_| ToolOpenRequest::send_path(open_path.clone()),
                    {translate("common-open")}
                }
            }
            div { class: "grid grid-cols-1 gap-2 sm:grid-cols-12",
                input {
                    class: "sm:col-span-3 rounded-lg bg-background px-2.5 py-1.5 text-xs text-foreground ring-1 ring-inset ring-foreground/12 focus:outline-none focus:ring-primary/50",
                    placeholder: "server-id",
                    value: "{id}",
                    oninput: move |event| id.set(event.value()),
                }
                select {
                    class: "sm:col-span-2 rounded-lg bg-background px-2 py-1.5 text-xs text-foreground ring-1 ring-inset ring-foreground/12 focus:outline-none focus:ring-primary/50",
                    value: "{transport}",
                    onchange: move |event| transport.set(event.value()),
                    option { value: "http", "http" }
                    option { value: "stdio", "stdio" }
                    option { value: "sse", "sse" }
                }
                input {
                    class: "sm:col-span-4 rounded-lg bg-background px-2.5 py-1.5 text-xs text-foreground ring-1 ring-inset ring-foreground/12 focus:outline-none focus:ring-primary/50",
                    placeholder: if is_stdio { "npx" } else { "https://mcp.example.com/mcp" },
                    value: "{target}",
                    oninput: move |event| target.set(event.value()),
                }
                if is_stdio {
                    input {
                        class: "sm:col-span-2 rounded-lg bg-background px-2.5 py-1.5 text-xs text-foreground ring-1 ring-inset ring-foreground/12 focus:outline-none focus:ring-primary/50",
                        placeholder: "-y @scope/mcp",
                        value: "{args}",
                        oninput: move |event| args.set(event.value()),
                    }
                } else {
                    div { class: "sm:col-span-2 flex items-center gap-1.5",
                        input {
                            class: "min-w-0 flex-1 rounded-lg bg-background px-2 py-1.5 text-xs text-foreground ring-1 ring-inset ring-foreground/12 focus:outline-none focus:ring-primary/50 disabled:opacity-40",
                            placeholder: "TOKEN_ENV",
                            disabled: oauth(),
                            value: "{bearer_env}",
                            oninput: move |event| bearer_env.set(event.value()),
                        }
                        label { class: "flex shrink-0 cursor-pointer items-center gap-1 text-[10px] text-muted-foreground",
                            input {
                                r#type: "checkbox",
                                checked: oauth(),
                                onchange: move |event| oauth.set(event.checked()),
                            }
                            "OAuth"
                        }
                    }
                }
                div { class: "sm:col-span-1 flex justify-end",
                    ManagerButton {
                        variant: ManagerButtonVariant::Primary,
                        disabled: !can_submit,
                        onclick: move |_| {
                            let _ = send(&McpRegisterRequest {
                                id: id(),
                                transport: transport(),
                                target: target(),
                                args: args(),
                                bearer_token_env_var: bearer_env(),
                                oauth: !is_stdio && oauth(),
                            });
                            id.set(String::new());
                            target.set(String::new());
                            args.set(String::new());
                            bearer_env.set(String::new());
                            oauth.set(false);
                        },
                        {translate("common-add")}
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
                onclick: move |_| ToolOpenRequest::send_path(open_brewfile.clone()),
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
                ToolOperationKind::Install.send(provider.clone(), id.clone(), String::new());
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
            span { class: "size-1.5 rounded-full {item.status.dot_class()}" }
            {item.status.label()}
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
                        variant: kind.variant(),
                        disabled: pending.contains(&operation),
                        onclick: move |_| {
                            kind.send(
                                operation_provider.clone(),
                                operation_id.clone(),
                                String::new(),
                            );
                        },
                        {kind.label()}
                    }
                }
            }
        }
    }
}

impl ToolOperationKind {
    fn send(self, provider: ToolProvider, id: String, value: String) {
        let _ = match self {
            Self::Install => send(&ToolInstallRequest { provider, id }),
            Self::Update => send(&ToolUpdateRequest { provider, id }),
            Self::Uninstall => send(&ToolUninstallRequest { provider, id }),
            Self::Forget => send(&ToolForgetRequest { provider, id }),
            Self::Adopt => send(&ToolAdoptRequest {
                provider,
                id,
                value,
            }),
            Self::Link => send(&ToolLinkRequest { provider, id }),
            Self::Unlink => send(&ToolUnlinkRequest { provider, id }),
            Self::Apply => send(&ToolApplyRequest { provider }),
            Self::Import => send(&ToolImportRequest { provider, value }),
        };
    }

    fn label(self) -> String {
        translate(match self {
            Self::Install => "common-install",
            Self::Update => "common-update",
            Self::Uninstall => "common-uninstall",
            Self::Forget => "tools-forget",
            Self::Adopt => "tools-manage",
            Self::Link => "tools-link",
            Self::Unlink => "tools-unlink",
            Self::Apply => "tools-apply",
            Self::Import => "tools-import",
        })
    }

    fn variant(self) -> ManagerButtonVariant {
        match self {
            Self::Install | Self::Link => ManagerButtonVariant::Primary,
            Self::Uninstall | Self::Forget | Self::Unlink => ManagerButtonVariant::Danger,
            _ => ManagerButtonVariant::Secondary,
        }
    }
}

impl ToolStatus {
    fn label(self) -> String {
        translate(match self {
            Self::Available => "tools-status-available",
            Self::Installed => "common-installed",
            Self::Outdated => "lsp-status-outdated",
            Self::Missing => "tools-status-missing",
            Self::Conflict => "tools-status-conflict",
            Self::Failed => "common-failed",
        })
    }

    fn dot_class(self) -> &'static str {
        match self {
            Self::Installed => "bg-success",
            Self::Outdated => "bg-amber-500",
            Self::Conflict | Self::Failed => "bg-rose-500",
            Self::Missing => "bg-muted-foreground/40",
            Self::Available => "bg-primary/70",
        }
    }
}

impl ToolOperationNotice {
    fn result_message(&self) -> String {
        let id = self.operation.item_id.as_str();
        match self.operation.kind {
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
}

impl ToolOpenRequest {
    fn send_path(path: String) {
        let _ = send(&Self { path });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_breadcrumbs_keep_the_page_root_and_navigate_to_the_selected_category() {
        let address = ToolRouteOption::address("acp", Vec::new());
        assert_eq!(
            address.current.intent,
            Some(AddressIntent::Navigate {
                url: "vmux://tools/".into()
            })
        );
        assert_eq!(
            address.fixed[0].entry.intent,
            Some(AddressIntent::Navigate {
                url: "vmux://tools/acp".into()
            })
        );
        assert_eq!(
            address.fixed[0].entry.label,
            translate("tools-provider-acp-agents")
        );
        let extensions = ToolRouteOption::address("extensions", Vec::new());
        assert_eq!(
            extensions.fixed[0].entry.intent,
            Some(AddressIntent::Navigate {
                url: "vmux://tools/extensions".into()
            })
        );
    }

    #[test]
    fn tool_routes_are_data_driven() {
        assert_eq!(
            ToolRoute::parse("vmux://tools/").unwrap_or_default().id(),
            "acp"
        );
        assert_eq!(
            ToolRoute::parse("vmux://tools/acp")
                .unwrap_or_default()
                .id(),
            "acp"
        );
        assert_eq!(
            ToolRoute::parse("vmux://tools/lsp/")
                .unwrap_or_default()
                .id(),
            "lsp"
        );
    }
}
