#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::state::GitCommandLogEntry;

#[component]
pub(super) fn CommandLogCard(command_log: Vec<GitCommandLogEntry>) -> Element {
    rsx! {
        section { class: "flex max-h-28 shrink-0 flex-col bg-foreground/[0.02]",
            div { class: "flex h-7 shrink-0 items-center gap-1.5 px-3",
                LineIconView { icon: LineIcon::Terminal, class: "size-3 text-muted-foreground" }
                span { class: "min-w-0 flex-1 truncate text-[10px] font-medium text-muted-foreground", {translate("git-command-log")} }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-2 pb-1.5 font-mono text-[10px]",
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
