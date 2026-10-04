use dioxus::prelude::*;
use dioxus_primitives::alert_dialog::{
    self, AlertDialogActionProps, AlertDialogActionsProps, AlertDialogCancelProps,
    AlertDialogContentProps, AlertDialogDescriptionProps, AlertDialogRootProps,
    AlertDialogTitleProps,
};
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

use crate::util::cn;

#[component]
pub fn AlertDialogRoot(props: AlertDialogRootProps) -> Element {
    rsx! {
        alert_dialog::AlertDialogRoot {
            class: "group fixed inset-0 z-[1000] bg-scrim data-[state=closed]:animate-[dx-fade-zoom-out_150ms_ease-in_forwards] data-[state=open]:animate-[dx-fade-zoom-in_150ms_ease-out_forwards]",
            id: props.id,
            default_open: props.default_open,
            open: props.open,
            on_open_change: props.on_open_change,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn AlertDialogContent(props: AlertDialogContentProps) -> Element {
    let merged = cn([
        "fixed left-1/2 top-1/2 z-[1001] flex w-full max-w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col gap-4 rounded-lg border border-border bg-background px-6 pb-6 pt-8 text-center font-sans text-muted-foreground shadow-[0_2px_10px_rgb(0_0_0_/_18%)] sm:max-w-lg sm:text-left",
        props.class.as_deref().unwrap_or_default(),
    ]);
    rsx! {
        alert_dialog::AlertDialogContent {
            id: props.id,
            class: Some(merged),
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn AlertDialogTitle(props: AlertDialogTitleProps) -> Element {
    let base = attributes!(h2 {
        class: "m-0 text-xl font-bold text-muted-foreground"
    });
    let merged = merge_attributes(vec![base, props.attributes]);
    rsx! {
        alert_dialog::AlertDialogTitle {
            attributes: merged,
            {props.children}
        }
    }
}

#[component]
pub fn AlertDialogDescription(props: AlertDialogDescriptionProps) -> Element {
    let base = attributes!(p {
        class: "m-0 text-base text-muted-foreground"
    });
    let merged = merge_attributes(vec![base, props.attributes]);
    rsx! {
        alert_dialog::AlertDialogDescription {
            attributes: merged,
            {props.children}
        }
    }
}

#[component]
pub fn AlertDialogActions(props: AlertDialogActionsProps) -> Element {
    rsx! {
        alert_dialog::AlertDialogActions {
            class: "flex flex-col-reverse gap-3 sm:flex-row sm:justify-end",
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn AlertDialogCancel(props: AlertDialogCancelProps) -> Element {
    rsx! {
        alert_dialog::AlertDialogCancel {
            on_click: props.on_click,
            class: "cursor-pointer rounded-md border border-border bg-background px-[18px] py-2 text-base text-muted-foreground transition-colors hover:bg-accent dark:bg-card",
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn AlertDialogAction(props: AlertDialogActionProps) -> Element {
    rsx! {
        alert_dialog::AlertDialogAction {
            class: "cursor-pointer rounded-md border border-destructive bg-destructive px-[18px] py-2 text-base text-primary-foreground transition-colors hover:opacity-90 focus-visible:shadow-[0_0_0_2px_var(--ring)]",
            on_click: props.on_click,
            attributes: props.attributes,
            {props.children}
        }
    }
}
