use dioxus::prelude::*;

use crate::components::icon::Icon;
use crate::components::tree_row::{SIDEBAR_CARD_CHEVRON_CLOSED, SIDEBAR_CARD_CHEVRON_OPEN};

pub const SHEET_SECTION: &str = "vmux-sheet-section mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg bg-foreground/[0.055] p-1";

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
