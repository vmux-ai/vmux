use crate::icon::{LineIcon, LineIconView};
use dioxus::prelude::*;
use dioxus_primitives::progress::{self, ProgressIndicatorProps, ProgressProps};

#[component]
pub fn ChecklistItem(
    label: String,
    #[props(default)] completed: bool,
    #[props(default)] pending: bool,
    #[props(default)] primary: bool,
    #[props(default)] class: String,
    #[props(default)] on_select: Option<EventHandler<()>>,
) -> Element {
    let interactive = on_select.is_some();
    let interaction = if interactive {
        "cursor-pointer rounded-md transition-colors duration-100 hover:bg-foreground/[0.04] active:bg-foreground/[0.06] focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary motion-reduce:transition-none"
    } else {
        "cursor-default"
    };
    let check = if completed {
        "border-transparent text-white"
    } else {
        "border-foreground/30"
    };
    rsx! {
        button {
            r#type: "button",
            role: "checkbox",
            aria_label: label.clone(),
            aria_checked: completed,
            aria_busy: pending,
            aria_readonly: (!interactive).to_string(),
            disabled: !interactive || pending,
            class: "flex min-h-8 w-full items-center gap-2 px-2 py-1.5 text-left text-sm text-foreground {interaction} {class}",
            onclick: move |_| { if let Some(handler) = on_select { handler.call(()); } },
            span {
                aria_hidden: "true",
                class: "flex size-4 shrink-0 items-center justify-center rounded-[4px] border {check}",
                style: completed.then_some("background:linear-gradient(135deg,var(--primary-action-start),var(--primary-action-middle),var(--primary-action-end));"),
                if completed { LineIconView { icon: LineIcon::Check, class: "size-3".to_string() } }
            }
            span {
                class: "min-w-0 flex-1",
                style: primary.then_some("background-image:linear-gradient(135deg,var(--primary-action-start),var(--primary-action-middle),var(--primary-action-end));background-clip:text;-webkit-background-clip:text;color:transparent;"),
                {label.clone()}
            }
            if pending { LoadingIcon { class: "size-3.5 text-muted-foreground".to_string() } }
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
