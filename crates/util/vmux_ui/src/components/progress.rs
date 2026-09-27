use dioxus::prelude::*;
use dioxus_primitives::progress::{self, ProgressIndicatorProps, ProgressProps};

#[component]
pub fn Progress(props: ProgressProps) -> Element {
    rsx! {
        progress::Progress {
            class: "group relative h-1.5 w-full overflow-hidden rounded-full bg-foreground/10",
            value: props.value,
            max: props.max,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn ProgressIndicator(props: ProgressIndicatorProps) -> Element {
    rsx! {
        progress::ProgressIndicator {
            class: "h-full w-[var(--progress-value,0%)] rounded-full bg-primary transition-[width] duration-200 group-data-[state=indeterminate]:w-1/3 group-data-[state=indeterminate]:animate-[update-indeterminate_1.2s_ease-in-out_infinite]",
            attributes: props.attributes,
            {props.children}
        }
    }
}
