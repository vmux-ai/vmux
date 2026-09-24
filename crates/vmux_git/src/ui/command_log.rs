#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::card::{Card, CardVariant};
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::state::GitCommandLogEntry;

#[component]
pub(super) fn CommandLogCard(command_log: Vec<GitCommandLogEntry>) -> Element {
    rsx! {
        Card { variant: CardVariant::Panel, class: "order-3 min-h-36 border-t-sky-400/25 sm:col-start-2 sm:row-start-5 sm:row-span-2 sm:min-h-0 sm:order-none",
            div { class: "flex h-7 shrink-0 items-center gap-1.5 border-b border-foreground/[0.07] bg-gradient-to-r from-sky-400/[0.055] to-transparent px-2",
                div { class: "flex size-5 shrink-0 items-center justify-center rounded-md bg-sky-400/10 text-sky-400 ring-1 ring-inset ring-sky-400/15",
                    LineIconView { icon: LineIcon::Terminal, class: "h-3 w-3" }
                }
                span { class: "min-w-0 flex-1 truncate text-[11px] font-semibold tracking-[-0.01em]", {translate("git-command-log")} }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-2 py-1.5 font-mono text-[10px]",
                if command_log.is_empty() {
                    div { class: "flex h-full min-h-20 items-center px-2 text-muted-foreground", {translate("git-command-log-empty")} }
                } else {
                    for (index, entry) in command_log.iter().rev().enumerate() {
                        div { key: "{index}-{entry.operation}", class: "flex min-h-6 items-start gap-2 rounded-md px-2 py-1 hover:bg-foreground/[0.035]",
                            span { class: if entry.ok { "mt-1 size-1.5 shrink-0 rounded-full bg-ansi-2" } else { "mt-1 size-1.5 shrink-0 rounded-full bg-ansi-1" } }
                            span { class: "shrink-0 font-semibold text-foreground",
                                if entry.operation.is_empty() { {translate("git-command-error")} } else { "{entry.operation}" }
                            }
                            span { class: if entry.ok { "min-w-0 truncate text-muted-foreground" } else { "min-w-0 break-words text-ansi-1" }, "{entry.message}" }
                        }
                    }
                }
            }
        }
    }
}
