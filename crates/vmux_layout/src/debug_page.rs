#![allow(non_snake_case)]

use crate::event::{DebugUpdateClear, DebugUpdateReady, RestartRequestEvent};
use dioxus::prelude::*;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_theme};

const BTN: &str = "cursor-pointer rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground transition-colors hover:border-foreground/30 hover:bg-muted";

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut version = use_signal(|| "v99.0.0".to_string());

    rsx! {
        div { class: "flex h-full min-h-0 flex-col gap-4 bg-background p-6 text-foreground",
            h1 { class: "text-lg font-semibold", "Debug" }
            section { class: "flex flex-col gap-2",
                h2 { class: "text-sm font-medium text-muted-foreground", "Auto-update" }
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
                        "Simulate update available"
                    }
                    button {
                        r#type: "button",
                        class: "{BTN}",
                        onclick: move |_| {
                            let _ = try_cef_bin_emit_rkyv(&DebugUpdateClear);
                        },
                        "Clear update"
                    }
                    button {
                        r#type: "button",
                        class: "{BTN}",
                        onclick: move |_| {
                            let _ = try_cef_bin_emit_rkyv(&RestartRequestEvent);
                        },
                        "Trigger restart"
                    }
                }
            }
        }
    }
}
