use dioxus::prelude::*;
use dioxus_primitives::hover_card::{
    self, HoverCardContentProps, HoverCardProps, HoverCardTriggerProps,
};

#[component]
pub fn HoverCard(props: HoverCardProps) -> Element {
    rsx! {
        hover_card::HoverCard {
            class: "relative inline-block",
            open: props.open,
            default_open: props.default_open,
            on_open_change: props.on_open_change,
            disabled: props.disabled,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn HoverCardTrigger(props: HoverCardTriggerProps) -> Element {
    rsx! {
        hover_card::HoverCardTrigger {
            class: "inline-block",
            id: props.id,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn HoverCardContent(props: HoverCardContentProps) -> Element {
    rsx! {
        hover_card::HoverCardContent {
            class: "z-[1000] min-w-[200px] rounded-lg border border-border bg-background p-1.5 shadow-[0_2px_10px_rgb(0_0_0_/_10%)] animate-[dx-fade-in_0.2s_ease-in-out_forwards] data-[state=closed]:hidden dark:border-primary dark:bg-muted dark:shadow-none",
            side: props.side,
            align: props.align,
            id: props.id,
            force_mount: props.force_mount,
            attributes: props.attributes,
            {props.children}
        }
    }
}
