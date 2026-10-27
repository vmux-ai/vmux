use dioxus::prelude::*;

#[component]
pub fn Collapse(leaving: bool, #[props(default)] class: String, children: Element) -> Element {
    rsx! {
        Collapsible { expanded: !leaving, class, {children} }
    }
}

#[component]
pub fn Collapsible(
    expanded: bool,
    #[props(default)] class: String,
    #[props(default)] content_class: String,
    children: Element,
) -> Element {
    let state = if expanded {
        "grid-rows-[1fr] opacity-100"
    } else {
        "grid-rows-[0fr] opacity-0"
    };
    rsx! {
        div {
            class: "grid {state} transition-[grid-template-rows,opacity] duration-200 ease-out motion-reduce:transition-none {class}",
            div { class: "min-h-0 overflow-hidden {content_class}", {children} }
        }
    }
}
