use dioxus::prelude::*;
use dioxus_primitives::menubar::{
    self, MenubarContentProps, MenubarItemProps, MenubarMenuProps, MenubarProps,
    MenubarTriggerProps,
};

#[component]
pub fn Menubar(props: MenubarProps) -> Element {
    rsx! {
        menubar::Menubar {
            class: "flex gap-1 rounded-lg border-0 bg-background p-1 shadow-[inset_0_0_0_1px_var(--border)]",
            disabled: props.disabled,
            roving_loop: props.roving_loop,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn MenubarMenu(props: MenubarMenuProps) -> Element {
    rsx! {
        menubar::MenubarMenu {
            class: "relative",
            index: props.index,
            disabled: props.disabled,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn MenubarTrigger(props: MenubarTriggerProps) -> Element {
    rsx! {
        menubar::MenubarTrigger {
            class: "cursor-pointer rounded-[calc(0.5rem-0.25rem)] border-0 bg-transparent px-3 py-2 text-muted-foreground transition-colors data-[disabled=true]:cursor-not-allowed data-[state=open]:bg-accent data-[state=open]:text-foreground hover:bg-accent focus-visible:bg-accent focus-visible:text-foreground focus-visible:outline-none dark:data-[state=open]:bg-primary dark:hover:bg-primary",
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn MenubarContent(props: MenubarContentProps) -> Element {
    rsx! {
        menubar::MenubarContent {
            class: "pointer-events-none absolute left-0 top-full z-[1000] mt-2 min-w-[200px] origin-top rounded-lg bg-background p-1 opacity-0 shadow-[inset_0_0_0_1px_var(--border)] will-change-[transform,opacity] first:-ml-1 data-[state=closed]:pointer-events-none data-[state=closed]:animate-[dx-fade-zoom-out_150ms_ease-in_forwards] data-[state=open]:pointer-events-auto data-[state=open]:animate-[dx-fade-zoom-in_150ms_ease-out_forwards] dark:bg-muted dark:shadow-[inset_0_0_0_1px_var(--primary)]",
            id: props.id,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn MenubarItem(props: MenubarItemProps) -> Element {
    rsx! {
        menubar::MenubarItem {
            class: "block cursor-pointer rounded-[calc(0.5rem-0.25rem)] px-3 py-2 text-sm data-[disabled=true]:cursor-not-allowed data-[disabled=true]:text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:bg-accent focus-visible:text-foreground focus-visible:outline-none dark:hover:bg-primary",
            index: props.index,
            value: props.value,
            disabled: props.disabled,
            on_select: props.on_select,
            attributes: props.attributes,
            {props.children}
        }
    }
}
