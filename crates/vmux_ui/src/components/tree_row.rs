use dioxus::prelude::*;

use crate::components::icon::Icon;
use crate::file_icon::TypeIcon;

pub const SIDEBAR_TREE_ROW_GROUP: &str =
    "group/row flex w-full items-center rounded-md hover:bg-glass-hover";

pub const SIDEBAR_TREE_ROW: &str = "flex h-8 w-full cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-md px-1.5 text-left text-muted-foreground group-hover/row:text-foreground";

pub const SIDEBAR_TREE_SCROLLER: &str = "overflow-x-auto";

pub const SIDEBAR_TREE_COLUMN: &str = "flex w-max min-w-full flex-col";

pub const SIDEBAR_TREE_CHEVRON_OPEN: &str =
    "h-3 w-3 shrink-0 rotate-90 transition-transform duration-200 ease-out";

pub const SIDEBAR_TREE_CHEVRON_CLOSED: &str =
    "h-3 w-3 shrink-0 rotate-0 transition-transform duration-200 ease-out";

pub const SIDEBAR_TREE_CHILDREN_OPEN: &str = "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out";

pub const SIDEBAR_TREE_CHILDREN_CLOSED: &str =
    "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out";

#[component]
pub fn SidebarTreeRowGroup(children: Element) -> Element {
    rsx! {
        div { class: SIDEBAR_TREE_ROW_GROUP, {children} }
    }
}

#[component]
pub fn SidebarTreeChildren(expanded: bool, children: Element) -> Element {
    let mut opened = use_signal(|| expanded);

    use_effect(use_reactive!(|expanded| {
        if expanded {
            opened.set(true);
        }
    }));

    rsx! {
        div {
            class: if expanded { SIDEBAR_TREE_CHILDREN_OPEN } else { SIDEBAR_TREE_CHILDREN_CLOSED },
            div { class: "overflow-hidden pt-0.5",
                if opened() {
                    {children}
                }
            }
        }
    }
}

#[component]
pub fn SidebarTreeRow(
    path: String,
    label: String,
    is_dir: bool,
    #[props(default)] expanded: bool,
    #[props(default)] depth: u32,
    #[props(default)] emphasis: bool,
    #[props(default)] title: Option<String>,
    #[props(default = rsx! {})] trailing: Element,
    on_activate: EventHandler<()>,
) -> Element {
    let indent = 0.375 + f64::from(depth) * 0.75;
    let hint = title.unwrap_or_else(|| path.clone());
    rsx! {
        button {
            r#type: "button",
            title: "{hint}",
            class: SIDEBAR_TREE_ROW,
            style: "padding-left:{indent}rem;",
            onclick: move |_| on_activate.call(()),
            if is_dir {
                Icon {
                    class: if expanded { SIDEBAR_TREE_CHEVRON_OPEN } else { SIDEBAR_TREE_CHEVRON_CLOSED },
                    path { d: "m9 18 6-6-6-6" }
                }
            } else {
                span { class: "w-3 shrink-0" }
                TypeIcon { path: path.clone(), is_dir: false, class: "h-3.5 w-3.5 shrink-0" }
            }
            span {
                class: if emphasis { "flex-1 text-ui font-medium" } else { "flex-1 text-ui" },
                "{label}"
            }
            {trailing}
        }
    }
}
