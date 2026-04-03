use dioxus::prelude::*;
use dioxus_primitives::label::{self, LabelProps};

#[component]
pub fn Label(props: LabelProps) -> Element {
    rsx! {
        label::Label {
            class: "flex items-center text-xs leading-none text-muted-foreground",
            html_for: props.html_for,
            attributes: props.attributes,
            {props.children}
        }
    }
}
