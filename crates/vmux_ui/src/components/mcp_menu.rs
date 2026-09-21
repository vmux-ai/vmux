use dioxus::prelude::*;
use vmux_api::mcp::{
    McpServerAction, McpServerActionRequest, McpServerActionResult, McpServerEntry,
    McpServerStatus, McpServers, McpServersRequest,
};

use crate::components::prompt_box::{PromptMenuRow, PromptPopup, PromptPopupPlacement};
use crate::hooks::{send, use_listener};
use crate::i18n::translate;

#[derive(Clone, Copy, PartialEq)]
pub struct McpConnections {
    pub servers: Signal<Vec<McpServerEntry>>,
    pub loaded: Signal<bool>,
    pub loading: Signal<bool>,
    pub pending: Signal<String>,
    pub error: Signal<String>,
}

pub fn use_mcp_connections() -> McpConnections {
    let connections = McpConnections {
        servers: use_signal(Vec::new),
        loaded: use_signal(|| false),
        loading: use_signal(|| false),
        pending: use_signal(String::new),
        error: use_signal(String::new),
    };
    let mut servers = connections.servers;
    let mut loaded = connections.loaded;
    let mut loading = connections.loading;
    let _servers = use_listener::<McpServers, _>(move |incoming| {
        servers.set(incoming.servers);
        loaded.set(true);
        loading.set(false);
    });
    let mut pending = connections.pending;
    let mut error = connections.error;
    let _result = use_listener::<McpServerActionResult, _>(move |result| {
        if *pending.peek() == result.id {
            pending.set(String::new());
        }
        if result.success {
            error.set(String::new());
        } else {
            error.set(result.message);
        }
    });
    connections
}

impl McpConnections {
    pub fn request(&self) {
        if *self.loading.peek() {
            return;
        }
        let mut loading = self.loading;
        loading.set(true);
        if send(&McpServersRequest).is_err() {
            loading.set(false);
        }
    }

    pub fn activate(&self, server: &McpServerEntry) {
        if !self.pending.peek().is_empty() {
            return;
        }
        let action = match server.status {
            McpServerStatus::Available
            | McpServerStatus::AuthenticationRequired
            | McpServerStatus::Failed => McpServerAction::Connect,
            McpServerStatus::Configured => return,
            McpServerStatus::Connected => McpServerAction::Disconnect,
        };
        let mut pending = self.pending;
        let mut error = self.error;
        pending.set(server.id.clone());
        error.set(String::new());
        if send(&McpServerActionRequest {
            id: server.id.clone(),
            action,
        })
        .is_err()
        {
            pending.set(String::new());
        }
    }

    pub fn filtered(&self, query: &str) -> Vec<McpServerEntry> {
        let query = query.trim().to_ascii_lowercase();
        let mut matching = Vec::new();
        for server in self.servers.read().iter() {
            let description = McpServerText::description(server);
            if query.is_empty()
                || server.id.to_ascii_lowercase().contains(&query)
                || server.name.to_ascii_lowercase().contains(&query)
                || description.to_ascii_lowercase().contains(&query)
            {
                matching.push(server.clone());
            }
        }
        matching
    }
}

pub struct McpQuery;

impl McpQuery {
    pub fn read(draft: &str) -> Option<&str> {
        let rest = draft.strip_prefix("/mcp")?;
        if rest.is_empty() {
            return Some("");
        }
        rest.chars()
            .next()?
            .is_whitespace()
            .then(|| rest.trim_start())
    }
}

#[component]
pub fn McpMenu(
    connections: McpConnections,
    entries: Vec<McpServerEntry>,
    selected: usize,
    #[props(default)] placement: PromptPopupPlacement,
    on_select: EventHandler<usize>,
    on_hover: EventHandler<usize>,
    on_dismiss: EventHandler<()>,
) -> Element {
    let pending = (connections.pending)();
    let error = (connections.error)();
    rsx! {
        PromptPopup {
            placement,
            heading: Some(translate("tools-provider-mcp-servers")),
            on_dismiss: move |()| on_dismiss.call(()),
            if !error.is_empty() {
                div { class: "mx-3 mb-2 whitespace-pre-wrap rounded-lg bg-ansi-1/10 px-3 py-2 text-xs text-ansi-1 ring-1 ring-inset ring-ansi-1/20",
                    "{error}"
                }
            }
            if (connections.loading)() && !(connections.loaded)() {
                div { class: "px-3.5 py-2 text-sm text-muted-foreground", {translate("common-loading")} }
            } else if entries.is_empty() {
                div { class: "px-3.5 py-2 text-sm text-muted-foreground", {translate("tools-empty")} }
            } else {
                for (index, server) in entries.into_iter().enumerate() {
                    {
                        let server_pending = pending == server.id;
                        let status = McpServerText::status(server.status, server_pending);
                        let description = McpServerText::description(&server);
                        rsx! {
                            button {
                                key: "mcp-{server.id}",
                                r#type: "button",
                                class: PromptMenuRow::class(index == selected),
                                disabled: server.status == McpServerStatus::Configured
                                    || !pending.is_empty() && !server_pending,
                                onclick: move |_| on_select.call(index),
                                onmouseenter: move |_| on_hover.call(index),
                                span { class: "flex min-w-0 flex-1 flex-col",
                                    span { class: "truncate text-sm text-foreground", "{server.name}" }
                                    span { class: "truncate text-xs text-muted-foreground", "{description}" }
                                }
                                span { class: McpServerText::status_class(server.status, server_pending),
                                    if server.status == McpServerStatus::Connected && !server_pending {
                                        span { class: "size-1.5 rounded-full bg-success" }
                                        span { {translate("services-connected")} }
                                        span { class: "text-muted-foreground/50", "·" }
                                        span { {translate("mobile-pair-disconnect")} }
                                    } else {
                                        "{status}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

struct McpServerText;

impl McpServerText {
    fn description(server: &McpServerEntry) -> String {
        if server.id == "linear" {
            translate("mcp-linear-description")
        } else {
            server.description.clone()
        }
    }

    fn status(status: McpServerStatus, pending: bool) -> String {
        if pending {
            return translate("mobile-pair-connecting");
        }
        translate(match status {
            McpServerStatus::Available => "vault-connect",
            McpServerStatus::Configured => "tools-managed",
            McpServerStatus::Connected => "services-connected",
            McpServerStatus::AuthenticationRequired => "vault-connect",
            McpServerStatus::Failed => "common-retry",
        })
    }

    fn status_class(status: McpServerStatus, pending: bool) -> &'static str {
        if pending {
            return "shrink-0 text-xs text-muted-foreground";
        }
        match status {
            McpServerStatus::Connected => "flex shrink-0 items-center gap-1.5 text-xs text-success",
            McpServerStatus::AuthenticationRequired | McpServerStatus::Failed => {
                "shrink-0 text-xs text-amber-600 dark:text-amber-300"
            }
            McpServerStatus::Available | McpServerStatus::Configured => {
                "shrink-0 text-xs text-muted-foreground"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::McpQuery;

    #[test]
    fn query_opens_on_the_complete_command() {
        assert_eq!(McpQuery::read("/mcp"), Some(""));
        assert_eq!(McpQuery::read("/mcp linear"), Some("linear"));
        assert_eq!(McpQuery::read("/mcpx"), None);
    }
}
