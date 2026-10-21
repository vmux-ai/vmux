use dioxus::prelude::*;

#[component]
pub fn Collapse(leaving: bool, #[props(default)] class: String, children: Element) -> Element {
    let state = if leaving {
        "grid-rows-[0fr] opacity-0"
    } else {
        "grid-rows-[1fr] opacity-100"
    };
    rsx! {
        div {
            class: "grid {state} transition-[grid-template-rows,opacity] duration-200 ease-out motion-reduce:transition-none {class}",
            div { class: "min-h-0 overflow-hidden", {children} }
        }
    }
}
