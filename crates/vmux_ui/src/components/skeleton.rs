use dioxus::prelude::*;

#[component]
pub fn Skeleton(#[props(extends=GlobalAttributes)] attributes: Vec<Attribute>) -> Element {
    rsx! {
        div {
            class: "rounded-md bg-muted animate-pulse",
            ..attributes
        }
    }
}
