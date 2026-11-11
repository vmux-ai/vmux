use dioxus::prelude::*;
use vmux_api::mcp::{
    McpServerCancelRequest, McpServerRequest, McpServerStatus, McpServersRequest, McpServersUiState,
};
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::manager::{
    ManagerEmpty, ManagerList, ManagerPage, ManagerSkeleton, WorkspaceLayout,
};
use vmux_ui::components::page_address::{PageBreadcrumbs, PageToolbar, ToolbarControl};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::translate;
use vmux_ui::icon::LineIcon;

pub(crate) const URL: &str = "vmux://connectors/";

#[vmux_page::page(file = "src/feature.ron", page = "connectors", component = Page, states = ["PageAddressUiState", "ThemeUiState", "McpServersUiState"], subtree)]
pub(crate) struct ConnectorsPage;

#[component]
pub(crate) fn Page() -> Element {
    use_theme();
    let connection = use_ui_state::<McpServersUiState>().state;
    let snapshot = connection();
    rsx! {
        ManagerPage {
            PageBreadcrumbs { state: vmux_api::address::PageAddress {
                current: vmux_api::address::AddressOption { label_key: "connectors-title".into(), icon: Some(vmux_api::BuiltinIcon::Toolbox), ..Default::default() },
                ..Default::default()
            } }
            WorkspaceLayout {
                show_sidebar: false,
                toolbar: rsx! { PageToolbar { controls: vec![
                    ToolbarControl::button(translate("common-refresh"), move |_| { let _ = send(&McpServersRequest); }).icon(LineIcon::RefreshCw),
                ] } },
                ManagerList {
                    p { class: "px-2 py-3 text-xs text-muted-foreground", {translate("connectors-description")} }
                    if !snapshot.loaded {
                        ManagerSkeleton {}
                    } else if snapshot.servers.is_empty() {
                        ManagerEmpty { title: translate("connectors-empty"), detail: String::new() }
                    } else {
                        for server in snapshot.servers {
                            {
                                let pending = snapshot.pending.as_ref().is_some_and(|pending| pending.id == server.id);
                                let connected = server.status == McpServerStatus::Connected;
                                let id = server.id.clone();
                                rsx! {
                                    article { key: "{server.id}", class: "flex min-w-0 items-center gap-3 rounded-md px-2 py-3",
                                        div { class: "min-w-0 flex-1",
                                            div { class: "truncate text-sm font-medium", "{server.name}" }
                                            div { class: "truncate text-xs text-muted-foreground", "{server.description}" }
                                            if let Some(account) = server.account { div { class: "mt-1 text-xs", "{account}" } }
                                            if let Some(url) = server.setup_url {
                                                a { href: url, target: "_blank", class: "mt-1 inline-block text-xs text-primary", {translate("connectors-manage-access")} }
                                            }
                                        }
                                        span { class: "text-xs text-muted-foreground",
                                            {translate(if connected { "services-connected" } else if server.status == McpServerStatus::AuthenticationRequired { "connectors-sign-in-required" } else { "services-disconnected" })}
                                        }
                                        Button {
                                            size: ButtonSize::Sm,
                                            variant: ButtonVariant::Ghost,
                                            disabled: snapshot.pending.is_some() && !pending || server.status == McpServerStatus::Configured,
                                            onclick: move |_| {
                                                if pending { let _ = send(&McpServerCancelRequest); }
                                                else { let _ = send(&McpServerRequest { id: id.clone() }); }
                                            },
                                            {translate(if pending { "common-cancel" } else if connected { "mobile-pair-disconnect" } else { "vault-connect" })}
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(authorization) = snapshot.authorization {
                        div { class: "px-2 py-3 text-sm",
                            if let Some(code) = authorization.code { code { "{code}" } }
                            a { href: authorization.url, target: "_blank", class: "ml-3 text-primary", {translate("connectors-open-sign-in")} }
                        }
                    }
                    if let Some(result) = snapshot.result && !result.success {
                        p { role: "alert", class: "px-2 py-3 text-xs text-destructive", "{result.message}" }
                    }
                }
            }
        }
    }
}
