use crate::components::button::{Button, ButtonSize};
use crate::i18n::translate;
use crate::icon::{LineIcon, LineIconView};
use dioxus::prelude::*;
use dioxus_primitives::progress::{self, ProgressIndicatorProps, ProgressProps};

#[component]
pub fn TaskSteps(
    steps: Vec<String>,
    #[props(default)] completed_steps: Vec<usize>,
    #[props(default)] pending_step: Option<usize>,
    #[props(default)] current_step: Option<usize>,
    #[props(default)] on_select: Option<EventHandler<()>>,
    #[props(default)] action_step: usize,
    #[props(default = translate("common-continue"))] action_label: String,
) -> Element {
    let current = current_step
        .or(pending_step)
        .or_else(|| (0..steps.len()).find(|index| !completed_steps.contains(index)));
    rsx! {
        ol { class: "task-steps flex max-w-sm list-none flex-col gap-0.5", role: "list",
            for (index, label) in steps.into_iter().enumerate() {
                TaskStep { number: index + 1, label, completed: completed_steps.contains(&index), pending: pending_step == Some(index), current: current == Some(index), on_select: if index == action_step { on_select } else { None }, action_label: action_label.clone() }
            }
        }
    }
}

#[component]
pub fn TaskStep(
    number: usize,
    label: String,
    #[props(default)] completed: bool,
    #[props(default)] pending: bool,
    #[props(default)] current: bool,
    #[props(default)] on_select: Option<EventHandler<()>>,
    #[props(default = translate("common-continue"))] action_label: String,
    #[props(default = rsx! {})] children: Element,
) -> Element {
    let row = rsx! {
        span { aria_hidden: "true", class: "w-4 shrink-0 font-semibold tabular-nums opacity-65", "{number}." }
        span { class: "min-w-0 flex-1", {label.clone()} }
        if completed {
            span {
                role: "img",
                aria_label: translate("common-done"),
                class: "flex size-4 shrink-0 items-center justify-center rounded-full vmux-primary-fill",
                LineIconView { icon: LineIcon::Check, class: "size-3 [stroke-width:2.5]".to_string() }
            }
        } else if pending {
            LoadingIcon { class: "size-3.5 shrink-0 opacity-75".to_string() }
        }
    };
    rsx! {
        li {
            class: "flex min-w-0 flex-col text-inherit",
            aria_current: (current || pending).then_some("step"),
            aria_busy: pending,
            div { class: if on_select.is_some() { "flex min-h-8 items-center gap-2 px-2 text-sm font-medium" } else if current || pending { "flex min-h-8 items-center gap-2 px-2 py-1 text-sm font-medium" } else { "flex min-h-8 items-center gap-2 px-2 py-1 text-sm opacity-55" },
                {row}
                if let Some(on_select) = on_select {
                    Button {
                        size: ButtonSize::Sm,
                        r#type: "button",
                        class: "shrink-0",
                        aria_label: action_label.clone(),
                        title: label,
                        onclick: move |_| on_select.call(()),
                        {action_label}
                    }
                }
            }
            {children}
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
            class: "h-full w-[var(--progress-value,0%)] rounded-full vmux-primary-fill transition-[width] duration-200 group-data-[state=indeterminate]:w-1/3 group-data-[state=indeterminate]:animate-[update-indeterminate_1.2s_ease-in-out_infinite] motion-reduce:animate-none motion-reduce:transition-none",
            attributes: props.attributes,
            {props.children}
        }
    }
}
