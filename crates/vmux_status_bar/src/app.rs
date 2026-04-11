#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_status_bar::event::{TABS_EVENT, TabsHostEvent};
use vmux_ui::hooks::use_event_listener;

#[component]
pub fn App() -> Element {
    let mut tabs_state = use_signal(TabsHostEvent::default);
    let listener = use_event_listener::<TabsHostEvent, _>(TABS_EVENT, move |data| {
        tabs_state.set(data);
    });

    rsx! {
        div { class: "sb-root",
            if (listener.is_loading)() {
                div { class: "sb-message",
                    span { class: "sb-message-text", "Connecting…" }
                }
            } else if let Some(err) = (listener.error)() {
                div { class: "sb-message",
                    span { class: "sb-message-text sb-destructive", "{err}" }
                }
            } else {
                div { class: "sb-tabs",
                    for row in tabs_state().tabs {
                        div {
                            class: if row.is_active { "sb-tab sb-tab-active" } else { "sb-tab" },
                            span { class: "sb-tab-label", "{row.title}" }
                        }
                    }
                }
            }
        }
    }
}
