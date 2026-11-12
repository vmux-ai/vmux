use dioxus::prelude::*;
use vmux_api::mcp::McpAuthorization;

use crate::clipboard::Clipboard;
use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::connector_link::ConnectorLink;
use crate::i18n::{TranslationValue, translate, translate_with};

#[component]
pub fn ConnectorAuthorization(authorization: McpAuthorization, service: String) -> Element {
    rsx! {
        div { class: "mt-2 flex flex-col gap-2 text-xs",
            if let Some(code) = authorization.code {
                p { class: "text-muted-foreground",
                    {translate_with("connectors-verification-code-help", &[("service", TranslationValue::String(&service))])}
                }
                div { class: "flex items-center gap-2",
                    code { dir: "ltr", class: "select-all rounded-md bg-muted px-2 py-1 font-mono text-sm tracking-wide", "{code}" }
                    Button { size: ButtonSize::Xs, variant: ButtonVariant::Ghost,
                        onclick: move |_| Clipboard::write(&code),
                        {translate("common-copy")}
                    }
                }
            }
            ConnectorLink { url: authorization.url, label: translate("connectors-open-sign-in"), class: "text-primary" }
        }
    }
}
