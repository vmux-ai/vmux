use dioxus::prelude::*;
use dioxus_primitives::icon;
use dioxus_primitives::navbar::{
    self, NavbarContentProps, NavbarItemProps, NavbarNavProps, NavbarProps, NavbarTriggerProps,
};

#[component]
pub fn Navbar(props: NavbarProps) -> Element {
    rsx! {
        navbar::Navbar {
            class: "flex gap-1 rounded-lg border-0 p-1",
            disabled: props.disabled,
            roving_loop: props.roving_loop,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn NavbarNav(props: NavbarNavProps) -> Element {
    rsx! {
        navbar::NavbarNav {
            class: "group relative",
            index: props.index,
            disabled: props.disabled,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn NavbarTrigger(props: NavbarTriggerProps) -> Element {
    rsx! {
        navbar::NavbarTrigger {
            class: "flex cursor-pointer flex-row items-center justify-center rounded-[calc(0.5rem-0.25rem)] border-0 bg-transparent px-3 py-2 text-muted-foreground transition-colors data-[disabled=true]:cursor-not-allowed data-[state=open]:bg-accent data-[state=open]:text-foreground hover:bg-accent focus-visible:bg-accent focus-visible:text-foreground focus-visible:outline-none dark:data-[state=open]:bg-primary dark:hover:bg-primary",
            attributes: props.attributes,
            {props.children}
            icon::Icon {
                class: "transition-[rotate] duration-150 ease-[cubic-bezier(0.4,0,0.2,1)] group-data-[state=open]:rotate-180",
                width: "20px",
                height: "20px",
                stroke: "var(--secondary-color-4)",
                polyline { points: "6 9 12 15 18 9" }
            }
        }
    }
}

#[component]
pub fn NavbarContent(props: NavbarContentProps) -> Element {
    rsx! {
        navbar::NavbarContent {
            class: "pointer-events-none absolute left-0 top-full z-[1000] mt-2 min-w-[200px] origin-top rounded-lg bg-background p-1 opacity-0 shadow-[inset_0_0_0_1px_var(--border)] will-change-[transform,opacity] before:absolute before:left-0 before:top-[-0.5rem] before:h-2 before:w-full before:content-[''] first:-ml-1 data-[state=closed]:translate-y-4 data-[state=closed]:scale-[0.98] data-[state=closed]:opacity-0 data-[state=open]:pointer-events-auto data-[state=open]:translate-x-0 data-[state=open]:translate-y-0 data-[state=open]:scale-100 data-[state=open]:opacity-100 data-[state=open]:transition data-[state=open]:duration-200 data-[state=open]:ease-out data-[state=closed]:transition data-[state=closed]:duration-150 data-[state=closed]:ease-in dark:bg-muted dark:shadow-[inset_0_0_0_1px_var(--primary)]",
            id: props.id,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn NavbarItem(props: NavbarItemProps) -> Element {
    rsx! {
        navbar::NavbarItem {
            class: "block cursor-pointer rounded-[calc(0.5rem-0.25rem)] px-3 py-2 text-sm text-muted-foreground no-underline data-[disabled=true]:cursor-not-allowed data-[disabled=true]:text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:bg-accent focus-visible:text-foreground focus-visible:outline-none dark:hover:bg-primary",
            index: props.index,
            value: props.value,
            disabled: props.disabled,
            new_tab: props.new_tab,
            to: props.to,
            active_class: props.active_class,
            attributes: props.attributes,
            on_select: props.on_select,
            onclick: props.onclick,
            onmounted: props.onmounted,
            {props.children}
        }
    }
}
