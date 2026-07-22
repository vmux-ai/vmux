#![allow(non_snake_case)]

use crate::event::{
    DebugSimulateDownload, DebugUpdateClear, DebugUpdateReady, RestartRequestEvent,
};
use dioxus::prelude::*;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_theme};
use vmux_ui::i18n::translate;

const BTN: &str = "cursor-pointer rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground transition-colors hover:border-foreground/30 hover:bg-muted";

#[component]
pub fn Page() -> Element {
    use_theme();
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        document.set_title(&translate("debug-title"));
    }
    let mut version = use_signal(|| "v99.0.0".to_string());

    rsx! {
        div { class: "flex h-full min-h-0 flex-col gap-4 bg-background p-6 text-foreground",
            h1 { class: "text-lg font-semibold", {translate("debug-title")} }
            section { class: "flex flex-col gap-2",
                h2 { class: "text-sm font-medium text-muted-foreground", {translate("debug-auto-update")} }
                input {
                    r#type: "text",
                    class: "rounded-md border border-border bg-card px-3 py-2 text-sm outline-none",
                    value: "{version}",
                    oninput: move |e| version.set(e.value()),
                }
                div { class: "flex flex-wrap gap-2",
                    button {
                        r#type: "button",
                        class: "{BTN}",
                        onclick: move |_| {
                            let _ = try_cef_bin_emit_rkyv(&DebugUpdateReady { version: version() });
                        },
                        {translate("debug-simulate-update")}
                    }
                    button {
                        r#type: "button",
                        class: "{BTN}",
                        onclick: move |_| {
                            let _ = try_cef_bin_emit_rkyv(&DebugSimulateDownload);
                        },
                        {translate("debug-simulate-download")}
                    }
                    button {
                        r#type: "button",
                        class: "{BTN}",
                        onclick: move |_| {
                            let _ = try_cef_bin_emit_rkyv(&DebugUpdateClear);
                        },
                        {translate("debug-clear-update")}
                    }
                    button {
                        r#type: "button",
                        class: "{BTN}",
                        onclick: move |_| {
                            let _ = try_cef_bin_emit_rkyv(&RestartRequestEvent);
                        },
                        {translate("debug-trigger-restart")}
                    }
                }
            }
        }
    }
}
