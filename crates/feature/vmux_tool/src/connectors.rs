use dioxus::prelude::*;
use vmux_api::mcp::{
    ConnectorSetupRoute, McpServerCancelRequest, McpServerEntry, McpServerRequest, McpServerStatus,
    McpServersUiState,
};
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::composer::ComposerFrame;
use vmux_ui::components::connector_authorization::ConnectorAuthorization;
use vmux_ui::components::connector_icon::ConnectorIcon;
use vmux_ui::components::connector_link::ConnectorLink;
use vmux_ui::components::conversation::{Dock, Page, Transcript};
use vmux_ui::components::manager::{
    ManagerButton, ManagerButtonVariant, ManagerCollection, ManagerEmpty, ManagerItem,
    ManagerSkeleton, ManagerView,
};
use vmux_ui::components::setup::{
    SetupComposer, SetupConnect, SetupGuide, SetupProgress, SetupTurn,
};
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::LineIcon;
use vmux_ui::navigation::PageNavigation;

#[component]
pub(crate) fn Accounts(state: McpServersUiState, query: String, view: ManagerView) -> Element {
    let filter = Filter(query.trim().to_ascii_lowercase());
    let mut entries = Vec::new();
    for server in &state.servers {
        if filter.matches(server) {
            entries.push(server.clone());
        }
    }
    rsx! {
        p { class: "px-2 py-3 text-xs text-muted-foreground", {translate("connectors-description")} }
        if !state.loaded {
            ManagerSkeleton {}
        } else if entries.is_empty() {
            ManagerEmpty { title: translate(if state.servers.is_empty() { "connectors-empty" } else { "common-no-results" }), detail: String::new() }
        } else {
            ManagerCollection { view,
                for server in entries {
                    Account { key: "{server.id}", server, view, pending: state.pending.is_some() }
                }
            }
        }
        if let Some(result) = state.result && !result.success {
            p { role: "alert", class: "px-2 py-3 text-xs text-destructive", "{result.message}" }
        }
    }
}

#[component]
fn Account(server: McpServerEntry, view: ManagerView, pending: bool) -> Element {
    let connected = server.status == McpServerStatus::Connected;
    let id = server.id.clone();
    rsx! {
        ManagerItem {
            view, icon: rsx! { ConnectorIcon { id: server.id.clone() } },
            title: server.name, subtitle: server.description,
            meta: rsx! { if connected { span { class: "text-[10px] text-muted-foreground", {translate("services-connected")} } } },
            details: rsx! {
                if let Some(account) = server.account { span { class: "truncate text-xs text-muted-foreground", "{account}" } }
                if let Some(url) = server.setup_url {
                    ConnectorLink { url, label: translate("connectors-manage-access"), class: "text-xs text-primary" }
                }
            },
            actions: rsx! {
                if server.status == McpServerStatus::Configured {
                    span { class: "text-xs text-muted-foreground", {translate("tools-managed")} }
                } else {
                    ManagerButton {
                        variant: ManagerButtonVariant::Ghost, disabled: pending,
                        onclick: move |_| {
                            if connected { let _ = send(&McpServerRequest { id: id.clone() }); }
                            else { PageNavigation::stack(ConnectorSetupRoute(id.clone()).url()); }
                        },
                        {translate(if connected { "mobile-pair-disconnect" } else { "vault-connect" })}
                    }
                }
            },
        }
    }
}

#[component]
pub(crate) fn Setup(id: String, state: McpServersUiState) -> Element {
    let server = state.servers.iter().find(|server| server.id == id).cloned();
    let name = server
        .as_ref()
        .map(|server| server.name.clone())
        .unwrap_or_default();
    let title = if name.is_empty() {
        translate("connectors-title")
    } else {
        name.clone()
    };
    let busy = state
        .pending
        .as_ref()
        .is_some_and(|pending| pending.id == id);
    let connected = server
        .as_ref()
        .is_some_and(|server| server.status == McpServerStatus::Connected);
    let failed = state
        .result
        .as_ref()
        .filter(|result| result.id == id && !result.success);
    let retry = failed.is_some();
    let connect_id = id.clone();
    rsx! {
        document::Title { "{title}" }
        Page {
            accent: SetupGuide::COLOR.to_string(),
            on_keydown: move |event: KeyboardEvent| {
                let cancel = event.key() == Key::Escape || event.modifiers().ctrl() && event.key() == Key::Character("c".into());
                if busy && cancel {
                    event.prevent_default(); event.stop_propagation();
                    let _ = send(&McpServerCancelRequest);
                }
            },
            Transcript { id: "connector-setup-scroll".to_string(), at_bottom: false,
                if !state.loaded {
                    ManagerSkeleton {}
                } else {
                    SetupTurn {
                        if server.is_none() {
                            p { {translate("connectors-empty")} }
                        } else if connected {
                            p { role: "status", {translate_with("connectors-setup-connected", &[("service", TranslationValue::String(&name))])} }
                            if let Some(account) = server.as_ref().and_then(|server| server.account.as_ref()) {
                                p { class: "text-xs text-muted-foreground", "{account}" }
                            }
                            if let Some(url) = server.as_ref().and_then(|server| server.setup_url.clone()) {
                                ConnectorLink { url, label: translate("connectors-manage-access"), class: "text-xs text-primary" }
                            }
                        } else {
                            p { class: "text-sm leading-relaxed", {translate_with("connectors-setup-intro", &[("service", TranslationValue::String(&name))])} }
                            if busy {
                                SetupProgress { label: translate("onboarding-auth-pending"), icon: LineIcon::Shield }
                                if let Some(authorization) = state.authorization.clone() {
                                    ConnectorAuthorization { authorization, service: name.clone() }
                                }
                            }
                            if let Some(result) = failed {
                                p { role: "alert", class: "whitespace-pre-wrap text-xs text-destructive", "{result.message}" }
                            }
                        }
                    }
                }
            }
            Dock {
                ComposerFrame {
                    muted: true,
                    notice: Some(rsx! {
                        if !busy {
                            if connected || server.is_none() {
                                div { class: "px-3 py-3",
                                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost,
                                        onclick: move |_| PageNavigation::replace(crate::CONNECTORS_URL),
                                        {translate("connectors-manage")}
                                    }
                                }
                            } else if retry {
                                div { class: "px-3 py-3",
                                    Button { size: ButtonSize::Sm, disabled: state.loading,
                                        onclick: move |_| { let _ = send(&McpServerRequest { id: connect_id.clone() }); },
                                        {translate("common-retry")}
                                    }
                                }
                            } else {
                                SetupConnect { available: state.loaded && !state.loading,
                                    onclick: move |_| { let _ = send(&McpServerRequest { id: connect_id.clone() }); }
                                }
                            }
                        }
                    }),
                    SetupComposer { accent: SetupGuide::COLOR.to_string(), busy, cancel_hint: busy,
                        on_stop: move |_| { let _ = send(&McpServerCancelRequest); }
                    }
                }
            }
        }
    }
}

struct Filter(String);

impl Filter {
    fn matches(&self, server: &McpServerEntry) -> bool {
        self.0.is_empty()
            || server.id.to_ascii_lowercase().contains(&self.0)
            || server.name.to_ascii_lowercase().contains(&self.0)
            || server.description.to_ascii_lowercase().contains(&self.0)
            || server
                .account
                .as_ref()
                .is_some_and(|account| account.to_ascii_lowercase().contains(&self.0))
    }
}
