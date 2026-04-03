use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::dropdown_menu::{
    self, DropdownMenuContentProps, DropdownMenuItemProps, DropdownMenuProps,
    DropdownMenuTriggerProps,
};
use dioxus_primitives::merge_attributes;

#[component]
pub fn DropdownMenu(props: DropdownMenuProps) -> Element {
    let base = attributes!(div {
        class: "relative inline-block"
    });
    let merged = merge_attributes(vec![base, props.attributes.clone()]);

    rsx! {
        dropdown_menu::DropdownMenu {
            open: props.open,
            default_open: props.default_open,
            on_open_change: props.on_open_change,
            disabled: props.disabled,
            roving_loop: props.roving_loop,
            attributes: merged,
            {props.children}
        }
    }
}

#[component]
pub fn DropdownMenuTrigger(props: DropdownMenuTriggerProps) -> Element {
    let base = attributes!(button {
        class: "cursor-pointer rounded-lg border-0 bg-background px-[18px] py-2 text-base text-muted-foreground shadow-[inset_0_0_0_1px_var(--border)] transition-colors hover:bg-accent hover:text-foreground focus-visible:shadow-[0_0_0_2px_var(--ring)] dark:bg-card"
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        dropdown_menu::DropdownMenuTrigger { as: props.r#as, attributes: merged, {props.children} }
    }
}

#[component]
pub fn DropdownMenuContent(props: DropdownMenuContentProps) -> Element {
    let base = attributes!(div {
        class: "absolute left-0 top-full z-[1000] mt-1 min-w-[200px] origin-top rounded-lg bg-background p-1 opacity-0 shadow-[inset_0_0_0_1px_var(--border)] will-change-[transform,opacity] data-[state=closed]:pointer-events-none data-[state=closed]:animate-[dx-fade-zoom-out_150ms_ease-in_forwards] data-[state=open]:animate-[dx-fade-zoom-in_150ms_ease-out_forwards] dark:bg-muted dark:shadow-[inset_0_0_0_1px_var(--primary)]"
    });
    let merged = merge_attributes(vec![base, props.attributes.clone()]);

    rsx! {
        dropdown_menu::DropdownMenuContent { id: props.id, attributes: merged, {props.children} }
    }
}

#[component]
pub fn DropdownMenuItem<T: Clone + PartialEq + 'static>(
    props: DropdownMenuItemProps<T>,
) -> Element {
    let base = attributes!(div {
        class: "flex cursor-pointer select-none items-center gap-2 rounded-[calc(0.5rem-0.25rem)] px-3 py-2 text-sm text-muted-foreground outline-none data-[disabled=true]:cursor-not-allowed data-[disabled=true]:text-muted-foreground hover:bg-accent hover:text-foreground dark:hover:bg-primary"
    });
    let merged = merge_attributes(vec![base, props.attributes.clone()]);

    rsx! {
        dropdown_menu::DropdownMenuItem {
            disabled: props.disabled,
            value: props.value,
            index: props.index,
            on_select: props.on_select,
            attributes: merged,
            {props.children}
        }
    }
}
