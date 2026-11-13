use dioxus::prelude::*;

#[component]
pub fn ContentTransition(#[props(default)] class: String, children: Element) -> Element {
    let duration = crate::motion::Motion::CONTENT_MS;
    rsx! {
        div { class: "vmux-content-enter {class}", style: "--vmux-content-motion-ms:{duration}ms;", {children} }
    }
}

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
    let duration = crate::motion::Motion::LAYOUT_MS;
    let state = if expanded {
        "grid-rows-[1fr] opacity-100"
    } else {
        "grid-rows-[0fr] opacity-0"
    };
    rsx! {
        div {
            style: "--vmux-motion-ms:{duration}ms;",
            class: "grid {state} transition-[grid-template-rows,opacity] duration-[var(--vmux-motion-ms)] ease-out motion-reduce:transition-none {class}",
            div { class: "min-h-0 overflow-hidden {content_class}", {children} }
        }
    }
}
