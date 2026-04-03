use dioxus::prelude::*;
use dioxus_primitives::progress::{self, ProgressIndicatorProps, ProgressProps};

#[component]
pub fn Progress(props: ProgressProps) -> Element {
    rsx! {
        progress::Progress {
            class: "group relative h-2 w-[200px] overflow-hidden rounded-full bg-muted",
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
            class: "h-full bg-foreground transition-all duration-[250ms] ease-in-out w-[var(--progress-value,0%)] group-data-[state=indeterminate]:w-1/2 group-data-[state=indeterminate]:animate-[progress-indeterminate_1s_linear_infinite]",
            attributes: props.attributes,
            {props.children}
        }
    }
}
