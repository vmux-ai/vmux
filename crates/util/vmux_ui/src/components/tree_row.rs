use dioxus::prelude::*;

use crate::components::icon::Icon;
use crate::file_icon::TypeIcon;

pub const FILE_TREE_ROW_GROUP: &str =
    "group/row flex w-full items-center rounded-md hover:bg-glass-hover";

pub const FILE_TREE_ROW: &str = "flex h-[22px] w-full min-w-0 cursor-pointer items-center gap-1 whitespace-nowrap rounded-md px-1 text-left text-muted-foreground group-hover/row:text-foreground";

pub const FILE_TREE_LIST_GROUP: &str = "group/list";

pub const FILE_TREE_ROW_BASE: &str = "relative flex h-[22px] items-center gap-1 px-1 cursor-default outline-none transition-colors duration-100";

pub const FILE_TREE_STICKY_SURFACE: &str =
    "border-b border-foreground/10 bg-background/95 backdrop-blur";

pub const FILE_TREE_SCROLLER: &str = "min-h-0 overflow-x-hidden overflow-y-auto";

pub const FILE_TREE_COLUMN: &str = "flex min-w-0 flex-col";

pub const FILE_TREE_CHEVRON_OPEN: &str =
    "h-3 w-3 shrink-0 rotate-90 transition-transform duration-200 ease-out";

pub const FILE_TREE_CHEVRON_CLOSED: &str =
    "h-3 w-3 shrink-0 rotate-0 transition-transform duration-200 ease-out";

pub const SIDEBAR_CARD_CHEVRON_OPEN: &str =
    "h-3.5 w-3.5 shrink-0 pointer-events-none rotate-90 transition-transform duration-200 ease-out";

pub const SIDEBAR_CARD_CHEVRON_CLOSED: &str =
    "h-3.5 w-3.5 shrink-0 pointer-events-none rotate-0 transition-transform duration-200 ease-out";

pub const FILE_TREE_CHILDREN_OPEN: &str = "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out motion-reduce:transition-none";

pub const FILE_TREE_CHILDREN_CLOSED: &str = "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out motion-reduce:transition-none";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TreeRowAccent {
    Plain,
    Active,
    Focus,
    ActiveFocus,
}

impl TreeRowAccent {
    pub fn resolve(active: bool, focus: bool) -> Self {
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
            Self::Active => "bg-primary/12 text-foreground",
            Self::Focus => {
                "text-foreground/80 ring-1 ring-inset ring-foreground/40 group-focus-within/list:bg-foreground/[0.16] group-focus-within/list:text-foreground group-focus-within/list:ring-0"
            }
            Self::ActiveFocus => {
                "bg-primary/12 text-foreground ring-1 ring-inset ring-foreground/40 group-focus-within/list:bg-primary/25 group-focus-within/list:ring-0"
            }
        }
    }
}

#[component]
pub fn FileTree(children: Element) -> Element {
    rsx! {
        div { class: "flex min-h-0 min-w-0 flex-1 flex-col", {children} }
    }
}

#[component]
pub fn FileTreeHeader(children: Element) -> Element {
    rsx! {
        div { class: "sticky top-0 z-10 flex h-7 shrink-0 items-center justify-between bg-background/95 px-3 text-[10px] font-semibold text-muted-foreground backdrop-blur", {children} }
    }
}

#[component]
pub fn FileTreeScroller(
    #[props(default)] class: String,
    #[props(default)] onscroll: Option<EventHandler<Event<ScrollData>>>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: "{FILE_TREE_SCROLLER} {class}",
            onscroll: move |event| {
                if let Some(handler) = onscroll {
                    handler.call(event);
                }
            },
            {children}
        }
    }
}

#[component]
pub fn FileTreeRowGroup(children: Element) -> Element {
    rsx! {
        div { class: FILE_TREE_ROW_GROUP, {children} }
    }
}

#[component]
pub fn FileTreeChildren(expanded: bool, children: Element) -> Element {
    let mut opened = use_signal(|| expanded);

    use_effect(use_reactive!(|expanded| {
        if expanded {
            opened.set(true);
        }
    }));

    rsx! {
        div {
            class: if expanded { FILE_TREE_CHILDREN_OPEN } else { FILE_TREE_CHILDREN_CLOSED },
            div { class: "overflow-hidden pt-0.5",
                if opened() {
                    {children}
                }
            }
        }
    }
}

#[component]
pub fn FileTreeRow(
    path: String,
    label: String,
    is_dir: bool,
    #[props(default)] expanded: bool,
    #[props(default)] active: bool,
    #[props(default)] show_dir_icon: bool,
    #[props(default)] depth: u32,
    #[props(default)] emphasis: bool,
    #[props(default)] title: Option<String>,
    #[props(default)] leading: Option<Element>,
    #[props(default = rsx! {})] label_suffix: Element,
    #[props(default = rsx! {})] trailing: Element,
    on_activate: EventHandler<()>,
) -> Element {
    let indent = 8 + depth * 12;
    let hint = title.unwrap_or_else(|| path.clone());
    rsx! {
        button {
            r#type: "button",
            title: "{hint}",
            class: if active {
                "flex h-[22px] w-full min-w-0 cursor-pointer items-center gap-1 whitespace-nowrap rounded-md bg-foreground/[0.085] px-1 text-left text-foreground"
            } else {
                FILE_TREE_ROW
            },
            style: "padding-left:{indent}px;",
            onclick: move |_| on_activate.call(()),
            if is_dir {
                Icon {
                    class: if expanded { FILE_TREE_CHEVRON_OPEN } else { FILE_TREE_CHEVRON_CLOSED },
                    path { d: "m9 18 6-6-6-6" }
                }
                if show_dir_icon {
                    TypeIcon { path: path.clone(), is_dir: true, class: "h-3.5 w-3.5 shrink-0" }
                }
            } else if let Some(leading) = leading {
                span { class: "w-3 shrink-0" }
                {leading}
            } else {
                span { class: "w-3 shrink-0" }
                TypeIcon { path: path.clone(), is_dir: false, class: "h-3.5 w-3.5 shrink-0" }
            }
            span { class: "flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden",
                span {
                    class: if emphasis {
                        "min-w-0 truncate text-ui font-medium"
                    } else {
                        "min-w-0 truncate text-ui"
                    },
                    "{label}"
                }
                {label_suffix}
            }
            {trailing}
        }
    }
}
