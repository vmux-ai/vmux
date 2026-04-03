use dioxus::prelude::*;
use dioxus_primitives::toolbar::{self, ToolbarButtonProps, ToolbarProps, ToolbarSeparatorProps};

const TOOLBAR: &str = "flex flex-wrap items-center justify-between gap-1 rounded-lg border-0 p-1 shadow-[inset_0_0_0_1px_var(--border)]";

const TOOLBAR_BUTTON: &str = "cursor-pointer rounded-[calc(0.5rem-0.25rem)] border-0 bg-transparent px-3 py-2 text-sm text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:bg-accent focus-visible:outline-none disabled:cursor-not-allowed disabled:text-muted-foreground dark:hover:bg-primary";

const TOOLBAR_SEPARATOR: &str = "mx-[5px] h-6 w-px bg-border";

const TOOLBAR_GROUP: &str = "flex flex-row gap-[5px]";

#[component]
pub fn Toolbar(props: ToolbarProps) -> Element {
    rsx! {
        toolbar::Toolbar {
            class: TOOLBAR,
            aria_label: props.aria_label,
            disabled: props.disabled,
            horizontal: props.horizontal,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn ToolbarButton(props: ToolbarButtonProps) -> Element {
    rsx! {
        toolbar::ToolbarButton {
            class: TOOLBAR_BUTTON,
            index: props.index,
            disabled: props.disabled,
            on_click: props.on_click,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn ToolbarSeparator(props: ToolbarSeparatorProps) -> Element {
    rsx! {
        toolbar::ToolbarSeparator {
            class: TOOLBAR_SEPARATOR,
            decorative: props.decorative,
            horizontal: props.horizontal,
            attributes: props.attributes,
        }
    }
}

#[component]
pub fn ToolbarGroup(
    #[props(extends = GlobalAttributes)]
    #[props(extends = div)]
    attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div { class: TOOLBAR_GROUP, ..attributes, {children} }
    }
}
