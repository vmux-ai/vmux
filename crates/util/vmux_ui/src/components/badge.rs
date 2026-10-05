use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

#[component]
pub fn Badge(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(span {
        class: "inline-flex shrink-0 items-center justify-center"
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        span { ..merged, {children} }
    }
}
