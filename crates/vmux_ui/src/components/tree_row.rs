use dioxus::prelude::*;

use crate::components::icon::Icon;
use crate::file_icon::TypeIcon;

pub const SIDEBAR_TREE_ROW_GROUP: &str =
    "group/row flex w-full items-center rounded-md hover:bg-glass-hover";

pub const SIDEBAR_TREE_ROW: &str = "flex h-[22px] w-full min-w-0 cursor-pointer items-center gap-1 whitespace-nowrap rounded-md px-1 text-left text-muted-foreground group-hover/row:text-foreground";

pub const SIDEBAR_TREE_LIST_GROUP: &str = "group/list";

pub const SIDEBAR_TREE_ROW_BASE: &str = "relative flex h-[22px] items-center gap-1 px-1 cursor-default outline-none transition-colors duration-100";

pub const SIDEBAR_STICKY_SURFACE: &str =
    "border-b border-foreground/10 bg-background/95 backdrop-blur";

pub const SIDEBAR_TREE_SCROLLER: &str = "overflow-x-hidden";

pub const SIDEBAR_TREE_COLUMN: &str = "flex min-w-0 flex-col";

pub const SIDEBAR_TREE_CHEVRON_OPEN: &str =
    "h-3 w-3 shrink-0 rotate-90 transition-transform duration-200 ease-out";

pub const SIDEBAR_TREE_CHEVRON_CLOSED: &str =
    "h-3 w-3 shrink-0 rotate-0 transition-transform duration-200 ease-out";

pub const SIDEBAR_CARD_CHEVRON_OPEN: &str =
    "h-3.5 w-3.5 shrink-0 pointer-events-none rotate-90 transition-transform duration-200 ease-out";

pub const SIDEBAR_CARD_CHEVRON_CLOSED: &str =
    "h-3.5 w-3.5 shrink-0 pointer-events-none rotate-0 transition-transform duration-200 ease-out";

pub const SIDEBAR_TREE_CHILDREN_OPEN: &str = "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out";

pub const SIDEBAR_TREE_CHILDREN_CLOSED: &str =
    "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TreeRowAccent {
    Plain,
    Active,
    Focus,
    ActiveFocus,
}

impl TreeRowAccent {
    pub fn of(active: bool, focus: bool) -> Self {
        match (active, focus) {
            (false, false) => Self::Plain,
            (true, false) => Self::Active,
            (false, true) => Self::Focus,
            (true, true) => Self::ActiveFocus,
        }
    }

    pub fn classes(self) -> &'static str {
        match self {
            Self::Plain => "text-foreground/80 hover:bg-foreground/[0.08]",
            Self::Active => "bg-cyan-400/12 text-foreground",
            Self::Focus => {
                "text-foreground/80 ring-1 ring-inset ring-foreground/40 group-focus-within/list:bg-foreground/[0.16] group-focus-within/list:text-foreground group-focus-within/list:ring-0"
            }
            Self::ActiveFocus => {
                "bg-cyan-400/12 text-foreground ring-1 ring-inset ring-foreground/40 group-focus-within/list:bg-cyan-400/25 group-focus-within/list:ring-0"
            }
        }
    }
}

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
    let indent = 8 + depth * 12;
    let hint = title.unwrap_or_else(|| path.clone());
    rsx! {
        button {
            r#type: "button",
            title: "{hint}",
            class: SIDEBAR_TREE_ROW,
            style: "padding-left:{indent}px;",
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
                class: if emphasis {
                    "min-w-0 flex-1 truncate text-ui font-medium"
                } else {
                    "min-w-0 flex-1 truncate text-ui"
                },
                "{label}"
            }
            {trailing}
        }
    }
}
