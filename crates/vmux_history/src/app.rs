#![allow(non_snake_case)]

use dioxus::prelude::*;

#[component]
pub fn App() -> Element {
    rsx! {
        document::Stylesheet { href: asset!("/assets/input.css") }
        div { class: "flex min-h-full min-w-0 flex-col items-stretch gap-4 p-4",
            span { class: "text-xl font-semibold", "History" }
        }
    }
}
