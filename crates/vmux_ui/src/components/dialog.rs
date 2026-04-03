use dioxus::prelude::*;
use dioxus_primitives::dialog::{
    self, DialogContentProps, DialogDescriptionProps, DialogRootProps, DialogTitleProps,
};

const DIALOG_ROOT: &str = "group fixed inset-0 z-[1000] bg-scrim opacity-0 will-change-[opacity,transform] data-[state=closed]:pointer-events-none data-[state=closed]:animate-[dx-fade-zoom-out_150ms_ease-in_forwards] data-[state=open]:animate-[dx-fade-zoom-in_150ms_ease-out_forwards]";

const DIALOG_PANEL: &str = "fixed left-1/2 top-1/2 z-[1001] flex w-full max-w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col gap-4 rounded-lg border border-border bg-background px-6 pb-6 pt-8 text-center font-sans text-muted-foreground shadow-[0_2px_10px_rgb(0_0_0_/_18%)] sm:max-w-lg sm:text-left";

const DIALOG_TITLE: &str = "m-0 text-xl font-bold text-muted-foreground";

const DIALOG_DESC: &str = "m-0 text-base text-muted-foreground";

#[component]
pub fn DialogRoot(props: DialogRootProps) -> Element {
    rsx! {
        dialog::DialogRoot {
            class: DIALOG_ROOT,
            id: props.id,
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
pub fn DialogContent(props: DialogContentProps) -> Element {
    let class = props
        .class
        .clone()
        .map(|c| format!("{DIALOG_PANEL} {c}"))
        .unwrap_or_else(|| DIALOG_PANEL.to_string());
    rsx! {
        dialog::DialogContent { class: Some(class), id: props.id, attributes: props.attributes, {props.children} }
    }
}

#[component]
pub fn DialogTitle(props: DialogTitleProps) -> Element {
    rsx! {
        dialog::DialogTitle {
            class: DIALOG_TITLE,
            id: props.id,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn DialogDescription(props: DialogDescriptionProps) -> Element {
    rsx! {
        dialog::DialogDescription {
            class: DIALOG_DESC,
            id: props.id,
            attributes: props.attributes,
            {props.children}
        }
    }
}
