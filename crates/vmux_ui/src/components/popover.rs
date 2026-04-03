use dioxus::prelude::*;
use dioxus_primitives::popover::{
    self, PopoverContentProps, PopoverRootProps, PopoverTriggerProps,
};

use crate::util::merge_class;

const POPOVER_CONTENT: &str = "z-[1000] flex min-w-[200px] max-w-[calc(100%-2rem)] flex-col rounded-lg border-0 bg-background p-1 text-center text-muted-foreground shadow-[inset_0_0_0_1px_var(--border)] will-change-[transform,opacity] data-[state=closed]:hidden data-[state=open]:flex data-[state=open]:animate-[dx-fade-in_0.2s_ease-in-out_forwards] dark:bg-muted dark:shadow-[inset_0_0_0_1px_var(--primary)] sm:max-w-lg sm:text-left";

#[component]
pub fn PopoverRoot(props: PopoverRootProps) -> Element {
    rsx! {
        popover::PopoverRoot {
            class: "relative inline-block",
            is_modal: props.is_modal,
            open: props.open,
            default_open: props.default_open,
            on_open_change: props.on_open_change,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn PopoverTrigger(props: PopoverTriggerProps) -> Element {
    rsx! {
        popover::PopoverTrigger {
            class: "cursor-pointer rounded-lg border border-border bg-background px-[18px] py-2 text-base text-muted-foreground transition-colors hover:bg-accent dark:bg-card",
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn PopoverContent(props: PopoverContentProps) -> Element {
    let class = merge_class(POPOVER_CONTENT, props.class.as_deref());
    rsx! {
        popover::PopoverContent {
            class: Some(class),
            id: props.id,
            side: props.side,
            align: props.align,
            attributes: props.attributes,
            {props.children}
        }
    }
}
