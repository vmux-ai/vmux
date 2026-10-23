#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

use crate::event::GitShortcutHelpRequest;

#[component]
pub(super) fn GitShortcutHelp() -> Element {
    rsx! {
        div {
            class: "absolute inset-0 z-50 flex items-center justify-center bg-background/70 p-4 backdrop-blur-sm",
            onclick: move |_| {
                send(&GitShortcutHelpRequest { visible: false });
            },
            div {
                class: "max-h-[min(42rem,calc(100vh-2rem))] w-full max-w-2xl overflow-y-auto rounded-2xl border border-foreground/10 bg-card p-4 shadow-2xl sm:p-5",
                onclick: move |event| event.stop_propagation(),
                div { class: "flex items-center justify-between gap-3",
                    div {
                        h2 { class: "text-base font-semibold", {translate("git-shortcut-title")} }
                        p { class: "mt-1 text-xs text-muted-foreground", {translate("git-shortcut-description")} }
                    }
                    button {
                        r#type: "button",
                        class: "rounded-lg border border-foreground/10 bg-foreground/[0.04] px-2 py-1 font-mono text-[10px] text-muted-foreground hover:text-foreground",
                        onclick: move |_| {
                            send(&GitShortcutHelpRequest { visible: false });
                        },
                        "esc"
                    }
                }
                div { class: "mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2",
                    ShortcutGroup {
                        title: translate("git-shortcut-universal"),
                        shortcuts: vec![
                            ("1–4".to_string(), translate("git-shortcut-panels")),
                            ("tab".to_string(), translate("git-shortcut-next-panel")),
                            ("↑/k".to_string(), translate("git-shortcut-previous")),
                            ("↓/j".to_string(), translate("git-shortcut-next")),
                            ("ctrl+p".to_string(), translate("git-shortcut-previous")),
                            ("ctrl+n".to_string(), translate("git-shortcut-next")),
                            ("?".to_string(), translate("git-shortcut-help")),
                        ],
                    }
                    ShortcutGroup {
                        title: translate("git-status"),
                        shortcuts: vec![
                            ("f".to_string(), translate("git-fetch")),
                            ("p".to_string(), translate("git-pull")),
                            ("P".to_string(), translate("git-push-label")),
                        ],
                    }
                    ShortcutGroup {
                        title: translate("git-files"),
                        shortcuts: vec![
                            ("space".to_string(), translate("git-toggle-stage")),
                            ("a".to_string(), translate("git-stage-all")),
                            ("s".to_string(), translate("git-stash")),
                            ("A".to_string(), translate("git-amend")),
                            ("x x".to_string(), translate("git-discard")),
                        ],
                    }
                    ShortcutGroup {
                        title: translate("git-branches"),
                        shortcuts: vec![
                            ("space / enter".to_string(), translate("git-checkout")),
                            ("n".to_string(), translate("git-new-branch")),
                            ("d".to_string(), translate("git-delete-branch")),
                            ("r".to_string(), translate("git-rebase")),
                            ("M".to_string(), translate("git-merge")),
                            ("f".to_string(), translate("git-fast-forward")),
                        ],
                    }
                    ShortcutGroup {
                        title: translate("git-commits"),
                        shortcuts: vec![
                            ("space".to_string(), translate("git-checkout-commit")),
                            ("C / V".to_string(), translate("git-cherry-pick")),
                            ("t".to_string(), translate("git-revert-commit")),
                        ],
                    }
                    ShortcutGroup {
                        title: translate("git-stashes"),
                        shortcuts: vec![
                            ("g".to_string(), translate("git-stash-pop")),
                            ("d".to_string(), translate("git-stash-drop")),
                        ],
                    }
                }
            }
        }
    }
}

#[component]
fn ShortcutGroup(title: String, shortcuts: Vec<(String, String)>) -> Element {
    rsx! {
        section { class: "overflow-hidden rounded-xl border border-foreground/[0.08] bg-foreground/[0.02]",
            h3 { class: "border-b border-foreground/[0.07] px-3 py-2 text-xs font-semibold", "{title}" }
            div { class: "divide-y divide-foreground/[0.06]",
                for (keycap, label) in shortcuts {
                    div { class: "flex min-h-8 items-center justify-between gap-3 px-3 py-1.5 text-xs",
                        span { class: "text-muted-foreground", "{label}" }
                        kbd { class: "shrink-0 rounded-md border border-foreground/10 bg-background/70 px-1.5 py-0.5 font-mono text-[10px] font-semibold text-foreground", "{keycap}" }
                    }
                }
            }
        }
    }
}
