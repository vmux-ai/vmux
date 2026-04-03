use dioxus::prelude::*;
use dioxus_primitives::alert_dialog::{
    self, AlertDialogActionProps, AlertDialogActionsProps, AlertDialogCancelProps,
    AlertDialogContentProps, AlertDialogDescriptionProps, AlertDialogRootProps,
    AlertDialogTitleProps,
};

use crate::util::merge_class;

const ALERT_DIALOG_STYLE: &str = r#"
.alert-dialog-title {
  margin: 0;
  color: var(--muted-foreground);
  font-size: 1.25rem;
  font-weight: 700;
}

.alert-dialog-description {
  margin: 0;
  color: var(--muted-foreground);
  font-size: 1rem;
}
"#;

#[component]
pub fn AlertDialogRoot(props: AlertDialogRootProps) -> Element {
    rsx! {
        Fragment {
            style { "{ALERT_DIALOG_STYLE}" }
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
}

#[component]
pub fn AlertDialogContent(props: AlertDialogContentProps) -> Element {
    let merged = merge_class(
        "fixed left-1/2 top-1/2 z-[1001] flex w-full max-w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col gap-4 rounded-lg border border-border bg-background px-6 pb-6 pt-8 text-center font-sans text-muted-foreground shadow-[0_2px_10px_rgb(0_0_0_/_18%)] sm:max-w-lg sm:text-left",
        props.class.as_deref(),
    );
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
    alert_dialog::AlertDialogTitle(props)
}

#[component]
pub fn AlertDialogDescription(props: AlertDialogDescriptionProps) -> Element {
    alert_dialog::AlertDialogDescription(props)
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
