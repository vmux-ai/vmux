use crate::i18n::translate;
use crate::icon::{LineIcon, LineIconView};
use dioxus::prelude::*;
use dioxus_primitives::progress::{self, ProgressIndicatorProps, ProgressProps};

#[component]
pub fn TaskStep(
    number: usize,
    label: String,
    #[props(default)] completed: bool,
    #[props(default)] pending: bool,
) -> Element {
    rsx! {
        li {
            class: "flex min-h-6 items-center gap-2 px-2 py-1 text-xs text-inherit",
            aria_current: pending.then_some("step"),
            aria_busy: pending,
            span { aria_hidden: "true", class: "w-4 shrink-0 font-semibold tabular-nums opacity-75", "{number}." }
            span { class: if pending { "min-w-0 font-medium" } else { "min-w-0 opacity-85" }, {label} }
            if completed {
                span {
                    role: "img",
                    aria_label: translate("common-done"),
                    class: "flex size-4 shrink-0 items-center justify-center rounded-full bg-[linear-gradient(135deg,var(--primary-action-start),var(--primary-action-middle),var(--primary-action-end))] text-white",
                    LineIconView { icon: LineIcon::Check, class: "size-3 [stroke-width:2.5]".to_string() }
                }
            } else if pending {
                LoadingIcon { class: "size-3.5 shrink-0 opacity-75".to_string() }
            }
        }
    }
}

#[component]
pub fn LoadingIcon(#[props(default = "size-4".to_string())] class: String) -> Element {
    rsx! {
        svg {
            class: "loading-icon inline-block shrink-0 {class}",
            view_box: "0 0 20 20",
            fill: "none",
            stroke: "currentColor",
            "aria-hidden": "true",
            circle {
                cx: "10",
                cy: "10",
                r: "7",
                stroke_width: "1.75",
                stroke_linecap: "round",
            }
        }
    }
}

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
            class: "h-full w-[var(--progress-value,0%)] rounded-full bg-primary transition-[width] duration-200 group-data-[state=indeterminate]:w-1/3 group-data-[state=indeterminate]:animate-[update-indeterminate_1.2s_ease-in-out_infinite] motion-reduce:animate-none motion-reduce:transition-none",
            attributes: props.attributes,
            {props.children}
        }
    }
}
