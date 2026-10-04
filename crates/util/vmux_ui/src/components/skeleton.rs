use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

#[component]
pub fn Skeleton(#[props(extends=GlobalAttributes)] attributes: Vec<Attribute>) -> Element {
    let base = attributes!(div {
        class: "animate-pulse rounded-md bg-muted motion-reduce:animate-none"
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div { ..merged }
    }
}
