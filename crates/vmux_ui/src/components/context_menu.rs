use dioxus::prelude::*;
use dioxus_primitives::context_menu::{
    self, ContextMenuContentProps, ContextMenuItemProps, ContextMenuProps, ContextMenuTriggerProps,
};

#[component]
pub fn ContextMenu(props: ContextMenuProps) -> Element {
    rsx! {
        context_menu::ContextMenu {
            disabled: props.disabled,
            open: props.open,
            default_open: props.default_open,
            on_open_change: props.on_open_change,
            roving_loop: props.roving_loop,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn ContextMenuTrigger(props: ContextMenuTriggerProps) -> Element {
    rsx! {
        context_menu::ContextMenuTrigger {
            padding: "20px",
            background: "var(--primary-color)",
            border: "1px dashed var(--primary-color-6)",
            border_radius: ".5rem",
            cursor: "context-menu",
            user_select: "none",
            text_align: "center",
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn ContextMenuContent(props: ContextMenuContentProps) -> Element {
    rsx! {
        context_menu::ContextMenuContent {
            class: "z-[1000] min-w-[220px] rounded-lg bg-background p-1 opacity-0 shadow-[inset_0_0_0_1px_var(--border)] will-change-[transform,opacity] data-[state=closed]:pointer-events-none data-[state=closed]:animate-[dx-fade-zoom-out_150ms_ease-in_forwards] data-[state=open]:animate-[dx-fade-zoom-in_150ms_ease-out_forwards] dark:bg-muted dark:shadow-[inset_0_0_0_1px_var(--primary)]",
            id: props.id,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn ContextMenuItem(props: ContextMenuItemProps) -> Element {
    rsx! {
        context_menu::ContextMenuItem {
            class: "flex cursor-pointer select-none items-center rounded-[calc(0.5rem-0.25rem)] px-3 py-2 text-sm text-muted-foreground outline-none transition-colors data-[disabled=true]:cursor-not-allowed data-[disabled=true]:text-muted-foreground hover:bg-accent hover:text-foreground dark:hover:bg-primary dark:hover:text-muted-foreground",
            disabled: props.disabled,
            value: props.value,
            index: props.index,
            on_select: props.on_select,
            attributes: props.attributes,
            {props.children}
        }
    }
}
