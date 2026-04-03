use dioxus::prelude::*;
use dioxus_primitives::separator::{self, SeparatorProps};

#[component]
pub fn Separator(props: SeparatorProps) -> Element {
    rsx! {
        separator::Separator {
            class: "bg-border data-[orientation=horizontal]:h-px data-[orientation=horizontal]:w-full data-[orientation=vertical]:h-full data-[orientation=vertical]:w-px",
            horizontal: props.horizontal,
            decorative: props.decorative,
            attributes: props.attributes,
            {props.children}
        }
    }
}
