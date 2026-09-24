#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

use crate::event::GitRepositoryPickerRequest;

use super::model::{BranchCollection, GitPanel};
use super::workspace::GitWorkspace;

#[component]
pub(super) fn GitShortcutBar(
    repo_root: String,
    focused_panel: Signal<GitPanel>,
    branch_collection: Signal<BranchCollection>,
    mut shortcut_help: Signal<bool>,
) -> Element {
    let panel_shortcuts = match focused_panel() {
        GitPanel::Status => Vec::new(),
        GitPanel::Files => vec![
            ("space", translate("git-toggle-stage")),
            ("a", translate("git-stage-all")),
            ("s", translate("git-stash")),
            ("A", translate("git-amend")),
            ("x", translate("git-discard")),
        ],
        GitPanel::Branches if branch_collection() == BranchCollection::Local => vec![
            ("space", translate("git-checkout")),
            ("n", translate("git-new-branch")),
            ("d", translate("git-delete-branch")),
            ("r", translate("git-rebase")),
            ("M", translate("git-merge")),
            ("f", translate("git-fast-forward")),
        ],
        GitPanel::Branches => Vec::new(),
        GitPanel::Commits => vec![
            ("space", translate("git-checkout-commit")),
            ("C", translate("git-cherry-pick")),
            ("t", translate("git-revert-commit")),
        ],
        GitPanel::Stash => vec![
            ("g", translate("git-stash-pop")),
            ("d", translate("git-stash-drop")),
        ],
    };

    rsx! {
        footer { class: "flex h-9 shrink-0 items-center gap-1 overflow-x-auto border-t border-foreground/[0.08] bg-card/92 px-2 text-[10px] text-muted-foreground backdrop-blur-xl",
            if focused_panel() == GitPanel::Status {
                ShortcutButton {
                    keycap: "e",
                    label: translate("git-edit-config"),
                    onclick: {
                        let repo_root = repo_root.clone();
                        move |_| GitWorkspace::edit_config(&repo_root)
                    },
                }
                ShortcutButton {
                    keycap: "u",
                    label: translate("settings-check-updates"),
                    onclick: {
                        move |_| GitWorkspace::check_for_updates()
                    },
                }
                ShortcutButton {
                    keycap: "enter",
                    label: translate("git-switch-recent-repository"),
                    onclick: {
                        let repo_root = repo_root.clone();
                        move |_| {
                            let _ = send(&GitRepositoryPickerRequest { path: repo_root.clone() });
                        }
                    },
                }
            } else {
                for (keycap, label) in panel_shortcuts {
                    ShortcutHint { keycap, label }
                }
                ShortcutHint { keycap: "↑↓", label: translate("git-shortcut-navigate") }
                ShortcutHint { keycap: "1–5", label: translate("git-shortcut-panels") }
            }
            button {
                r#type: "button",
                class: "ml-auto flex h-6 shrink-0 items-center gap-1.5 rounded-md px-2 text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground",
                onclick: move |_| shortcut_help.set(true),
                kbd { class: "rounded border border-foreground/10 bg-foreground/[0.055] px-1.5 py-0.5 font-mono text-[9px] font-semibold text-foreground", "?" }
                span { {translate("git-keybindings")} }
            }
        }
    }
}

#[component]
fn ShortcutButton(
    keycap: &'static str,
    label: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "flex h-6 shrink-0 items-center gap-1.5 rounded-md px-1.5 text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground",
            onclick: move |event| onclick.call(event),
            kbd { class: "rounded border border-foreground/10 bg-foreground/[0.055] px-1.5 py-0.5 font-mono text-[9px] font-semibold text-foreground", "{keycap}" }
            span { "{label}" }
        }
    }
}

#[component]
fn ShortcutHint(keycap: &'static str, label: String) -> Element {
    rsx! {
        span { class: "flex h-6 shrink-0 items-center gap-1.5 rounded-md px-1.5",
            kbd { class: "rounded border border-foreground/10 bg-foreground/[0.055] px-1.5 py-0.5 font-mono text-[9px] font-semibold text-foreground", "{keycap}" }
            span { "{label}" }
        }
    }
}

#[component]
pub(super) fn GitShortcutHelp(shortcut_help: Signal<bool>) -> Element {
    rsx! {
        div {
            class: "absolute inset-0 z-50 flex items-center justify-center bg-background/70 p-4 backdrop-blur-sm",
            onclick: move |_| shortcut_help.set(false),
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
                        onclick: move |_| shortcut_help.set(false),
                        "esc"
                    }
                }
                div { class: "mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2",
                    ShortcutGroup {
                        title: translate("git-shortcut-universal"),
                        shortcuts: vec![
                            ("1–5".to_string(), translate("git-shortcut-panels")),
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
