#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_history::event::{HISTORY_EVENT, HistoryEvent};
use vmux_ui::hooks::use_event_listener;

#[component]
pub fn App() -> Element {
    let mut history = use_signal(Vec::<String>::new);
    let listener = use_event_listener::<HistoryEvent, _>(HISTORY_EVENT, move |data| {
        history.set(data.history);
    });

    rsx! {
        document::Stylesheet { href: asset!("/assets/input.css") }
        div { class: "flex min-h-full min-w-0 flex-col items-stretch gap-4 p-4",
            span { class: "text-xl font-semibold", "History POC" }
            if (listener.is_loading)() {
                div { class: "flex min-w-0 flex-col items-stretch gap-3",
                    span { class: "text-ui text-muted-foreground", "Connecting…" }
                    span { class: "text-ui-xs text-muted-foreground/50 animate-pulse", "Waiting for `window.cef`…" }
                }
            } else if let Some(err) = (listener.error)() {
                span { class: "text-destructive", "{err}" }
            } else {
                div { class: "flex min-w-0 flex-col items-stretch gap-1",
                    for h in history() {
                        span { class: "whitespace-pre-wrap font-mono text-sm text-chart-3/90", "{h}" }
                    }
                }
            }
        }
    }
}
