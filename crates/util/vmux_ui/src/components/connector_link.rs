use dioxus::prelude::*;
use vmux_api::mcp::McpLinkOpenRequest;

use crate::components::context_menu::{
    ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger,
};
use crate::hooks::send;
use crate::i18n::translate;

#[component]
pub fn ConnectorLink(url: String, label: String, #[props(default)] class: String) -> Element {
    let in_app = url.clone();
    let external = url.clone();
    let menu_in_app = url.clone();
    let value = use_signal(String::new);
    rsx! {
        ContextMenu {
            attributes: vec![Attribute::new("class", "inline-flex", None, false)],
            ContextMenuTrigger { attributes: vec![],
                a {
                    href: url,
                    class,
                    onclick: move |event| {
                        event.prevent_default();
                        let _ = send(&McpLinkOpenRequest { url: in_app.clone(), external: false });
                    },
                    "{label}"
                }
            }
            ContextMenuContent { attributes: vec![],
                ContextMenuItem {
                    attributes: vec![], index: 0usize,
                    value: Into::<ReadSignal<String>>::into(value),
                    on_select: move |_: String| { let _ = send(&McpLinkOpenRequest { url: menu_in_app.clone(), external: false }); },
                    {translate("connectors-open-in-app")}
                }
                ContextMenuItem {
                    attributes: vec![], index: 1usize,
                    value: Into::<ReadSignal<String>>::into(value),
                    on_select: move |_: String| { let _ = send(&McpLinkOpenRequest { url: external.clone(), external: true }); },
                    {translate("connectors-open-external")}
                }
            }
        }
    }
}
