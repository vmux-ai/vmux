use dioxus::prelude::*;
use vmux_api::mcp::{
    McpServerCancelRequest, McpServerEntry, McpServerRequest, McpServerStatus, McpServersUiState,
};
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::connector_icon::ConnectorIcon;
use vmux_ui::components::connector_link::ConnectorLink;
use vmux_ui::components::manager::{ManagerEmpty, ManagerSkeleton};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

#[component]
pub(crate) fn Accounts(state: McpServersUiState, query: String) -> Element {
    let filter = Filter(query.trim().to_ascii_lowercase());
    let mut entries = Vec::new();
    for server in &state.servers {
        if filter.matches(server) {
            entries.push(server.clone());
        }
    }
    let snapshot = state;
    rsx! {
            p { class: "px-2 py-3 text-xs text-muted-foreground", {translate("connectors-description")} }
            if !snapshot.loaded {
                ManagerSkeleton {}
            } else if entries.is_empty() {
            ManagerEmpty { title: translate(if snapshot.servers.is_empty() { "connectors-empty" } else { "common-no-results" }), detail: String::new() }
            } else {
                for server in entries {
                    {
                        let pending = snapshot.pending.as_ref().is_some_and(|pending| pending.id == server.id);
                        let connected = server.status == McpServerStatus::Connected;
                        let id = server.id.clone();
                        rsx! {
                            article { key: "{server.id}", class: "flex min-w-0 items-center gap-3 rounded-md px-2 py-2",
                                ConnectorIcon { id: server.id.clone() }
                                div { class: "min-w-0 flex-1",
                                    div { class: "truncate text-sm font-medium", "{server.name}" }
                                    div { class: "truncate text-xs text-muted-foreground", "{server.description}" }
                                    if let Some(account) = server.account { div { class: "mt-1 text-xs", "{account}" } }
                                    if let Some(url) = server.setup_url {
                                        ConnectorLink { url, label: translate("connectors-manage-access"), class: "mt-1 text-xs text-primary" }
                                    }
                                }
                                if server.status == McpServerStatus::Configured {
                                    span { class: "text-xs text-muted-foreground", {translate("tools-managed")} }
                                } else {
                                Button {
                                    size: ButtonSize::Xs,
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
            }
            if let Some(authorization) = snapshot.authorization {
                div { class: "flex items-center gap-3 px-2 py-3 text-sm",
                    if let Some(code) = authorization.code { code { "{code}" } }
                    ConnectorLink { url: authorization.url, label: translate("connectors-open-sign-in"), class: "text-xs text-primary" }
                }
            }
            if let Some(result) = snapshot.result && !result.success {
                p { role: "alert", class: "px-2 py-3 text-xs text-destructive", "{result.message}" }
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
