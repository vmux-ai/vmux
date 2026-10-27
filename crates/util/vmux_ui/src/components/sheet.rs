use dioxus::prelude::*;

use crate::cn::cn;
use crate::components::icon::Icon;
use crate::components::tree_row::{SIDEBAR_CARD_CHEVRON_CLOSED, SIDEBAR_CARD_CHEVRON_OPEN};

pub const SHEET_SECTION: &str =
    "mb-2 flex shrink-0 flex-col rounded-lg bg-card p-1 @max-[12rem]:rounded-md @max-[12rem]:p-0.5";

#[component]
pub fn SheetHeaderAction(
    label: String,
    #[props(default)] active: bool,
    #[props(default)] subtle: bool,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    let class = match (active, subtle) {
        (true, _) => {
            "flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-full bg-foreground/[0.055] text-foreground"
        }
        (false, true) => {
            "flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-full text-muted-foreground opacity-0 transition-[color,opacity] group-hover:opacity-100 focus-visible:opacity-100 hover:text-foreground"
        }
        (false, false) => {
            "flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-full text-muted-foreground transition-colors hover:text-foreground"
        }
    };
    rsx! {
        button {
            r#type: "button",
            aria_label: label.clone(),
            class,
            onclick: move |event| onclick.call(event),
            {children}
        }
    }
}

#[component]
pub fn SheetHeaderChevron(
    expanded: bool,
    label: String,
    #[props(default)] subtle: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        SheetHeaderAction { label, active: !expanded, subtle, onclick,
            Icon {
                class: if expanded { SIDEBAR_CARD_CHEVRON_OPEN } else { SIDEBAR_CARD_CHEVRON_CLOSED },
                path { d: "m9 18 6-6-6-6" }
            }
        }
    }
}

#[component]
pub fn SheetToggleRow(
    label: String,
    expanded: bool,
    onclick: EventHandler<MouseEvent>,
    #[props(default)] title: String,
    #[props(default)] class: String,
    #[props(default)] label_class: String,
    #[props(default = rsx! {})] leading: Element,
) -> Element {
    let row_class = cn([
        "group flex h-8 w-full min-w-0 items-center gap-2 rounded px-2 text-left transition-colors hover:bg-foreground/[0.04] @max-[12rem]:gap-1.5 @max-[12rem]:px-1",
        class.as_str(),
    ]);
    let text_class = cn([
        "min-w-0 flex-1 truncate text-[10px] font-medium text-foreground/85",
        label_class.as_str(),
    ]);
    rsx! {
        button {
            r#type: "button",
            title,
            class: row_class,
            aria_expanded: expanded,
            onclick: move |event| onclick.call(event),
            {leading}
            span { class: text_class, "{label}" }
            span {
                class: if expanded {
                    "flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-muted-foreground"
                } else {
                    "flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-foreground/[0.055] text-foreground"
                },
                Icon {
                    class: if expanded { SIDEBAR_CARD_CHEVRON_OPEN } else { SIDEBAR_CARD_CHEVRON_CLOSED },
                    path { d: "m9 18 6-6-6-6" }
                }
            }
        }
    }
}

#[component]
pub fn SheetSelectorRow(
    label: String,
    open: bool,
    onclick: EventHandler<MouseEvent>,
    #[props(default)] title: String,
    #[props(default)] class: String,
    #[props(default)] label_class: String,
    #[props(default = rsx! {})] leading: Element,
) -> Element {
    let row_class = cn([
        "group flex h-8 w-full min-w-0 items-center gap-2 rounded px-2 text-left transition-colors hover:bg-foreground/[0.04] @max-[12rem]:gap-1.5 @max-[12rem]:px-1",
        class.as_str(),
    ]);
    let text_class = cn([
        "min-w-0 flex-1 truncate text-[10px] font-medium text-foreground/85",
        label_class.as_str(),
    ]);
    rsx! {
        button {
            r#type: "button",
            title,
            class: row_class,
            "data-vmux-dropdown-trigger": "",
            aria_expanded: open,
            onclick: move |event| onclick.call(event),
            {leading}
            span { class: text_class, "{label}" }
        }
    }
}
