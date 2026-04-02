//! Dioxus UI: register `cef.listen` for host history, then `cef.emit({})` so Bevy can mark the webview
//! ready and push history via Host Emit ([communication patterns](https://not-elm.github.io/bevy_cef/communication/)).

use crate::bridge::{HOST_HISTORY_CHANNEL, use_core_bridge};
use dioxus::prelude::*;
use serde::Deserialize;

const LOADING_CEF_MSG: &str = "Waiting for `window.cef`…";

#[derive(Clone, Debug, Deserialize)]
struct HostHistoryPayload {
    #[allow(dead_code)]
    url: Option<String>,
    history: Option<Vec<String>>,
}

#[component]
pub fn App() -> Element {
    let bevy_state = use_core_bridge::<HostHistoryPayload, _>(HOST_HISTORY_CHANNEL, |data| {
        println!("Host history: {:?}", data);
    });

    let caption = if let Some(e) = (bevy_state.error)() {
        e
    } else if (bevy_state.is_loading)() {
        LOADING_CEF_MSG.to_string()
    } else if (bevy_state.state)().is_none() {
        "Waiting for host history…".to_string()
    } else {
        "Listening to host updates…".to_string()
    };

    rsx! {
        document::Stylesheet { href: asset!("/assets/input.css") }
        div { class: "p-4 font-sans text-neutral-200",
            h1 { class: "mb-2 text-xl", "History POC" }
            if (bevy_state.is_loading)() {
                p { class: "whitespace-pre text-neutral-400",
                    for (i, ch) in LOADING_CEF_MSG.chars().enumerate() {
                        span {
                            key: "{i}",
                            class: "wave-y-char",
                            style: format!("animation-delay: {}ms", i * 45),
                            "{ch}"
                        }
                    }
                }
            } else {
                p { "{caption}" }
                for h in (bevy_state.state)()
                    .map(|p| p.history.unwrap_or_default())
                    .unwrap_or_default()
                {
                    p { "{h}" }
                }
            }
        }
    }
}
