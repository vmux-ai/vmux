use dioxus::prelude::*;
use vmux_api::mcp::{McpServerEntry, McpServerStatus, McpServersUiState};

use crate::components::prompt_box::{PromptMenuRow, PromptPopup, PromptPopupPlacement};
use crate::i18n::translate;

#[component]
pub fn McpMenu(
    state: McpServersUiState,
    entries: Vec<McpServerEntry>,
    selected: usize,
    #[props(default)] placement: PromptPopupPlacement,
    on_select: EventHandler<usize>,
    on_hover: EventHandler<usize>,
    on_dismiss: EventHandler<()>,
) -> Element {
    let pending = state
        .pending
        .as_ref()
        .map(|pending| pending.id.clone())
        .unwrap_or_default();
    let error = state
        .result
        .as_ref()
        .filter(|result| !result.success)
        .map(|result| result.message.clone())
        .unwrap_or_default();
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
            if state.loading && !state.loaded {
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
