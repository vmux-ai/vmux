use dioxus::prelude::*;
use dioxus_primitives::tooltip::{self, TooltipContentProps, TooltipProps, TooltipTriggerProps};

#[component]
pub fn Tooltip(props: TooltipProps) -> Element {
    rsx! {
        tooltip::Tooltip {
            class: "relative inline-block",
            disabled: props.disabled,
            open: props.open,
            default_open: props.default_open,
            on_open_change: props.on_open_change,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn TooltipTrigger(props: TooltipTriggerProps) -> Element {
    rsx! {
        tooltip::TooltipTrigger {
            class: "inline-block",
            id: props.id,
            as: props.r#as,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn TooltipContent(props: TooltipContentProps) -> Element {
    rsx! {
        tooltip::TooltipContent {
            class: "z-[1000] max-w-[250px] rounded-lg bg-muted-foreground px-3 py-2 text-sm leading-snug text-background animate-[dx-fade-in_0.2s_ease-in-out_forwards] data-[state=closed]:hidden data-[state=open]:block",
            id: props.id,
            side: props.side,
            align: props.align,
            attributes: props.attributes,
            {props.children}
        }
    }
}
