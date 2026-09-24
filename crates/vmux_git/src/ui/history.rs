#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::card::{Card, CardVariant};
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::event::*;

use super::model::GitPanel;
use super::panel::{HeaderActionButton, PanelHeader, PanelIcon};
use super::workspace::GitWorkspace;

#[component]
pub(super) fn HistoryCard(
    repository: GitRepositorySnapshot,
    selected_commit: Signal<String>,
    focused_panel: Signal<GitPanel>,
) -> Element {
    let selected = repository
        .commits
        .iter()
        .find(|entry| entry.sha == selected_commit())
        .cloned();
    let commit_actions = selected.clone().map(|commit| {
        rsx! {
            div { class: "flex shrink-0 items-center gap-0.5",
                HeaderActionButton {
                    icon: LineIcon::Check,
                    shortcut: "space",
                    label: translate("git-checkout-commit"),
                    disabled: false,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let commit = commit.sha.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::CheckoutCommit { commit: commit.clone() })
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::GitCommit,
                    shortcut: "C",
                    label: translate("git-cherry-pick"),
                    disabled: false,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let commit = commit.sha.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::CherryPick { commit: commit.clone() })
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::RotateCcw,
                    shortcut: "t",
                    label: translate("git-revert-commit"),
                    disabled: false,
                    danger: true,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let commit = commit.sha.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::Revert { commit: commit.clone() })
                    },
                }
            }
        }
    });

    rsx! {
        Card {
            variant: CardVariant::Panel,
            class: if focused_panel() == GitPanel::Commits {
                "order-6 min-h-48 border-ansi-2/55 shadow-[0_0_0_1px_color-mix(in_oklab,var(--ansi-2)_18%,transparent)] sm:col-start-1 sm:row-start-4 sm:row-span-2 sm:min-h-0 sm:order-none"
            } else {
                "order-6 min-h-48 border-t-sky-400/30 sm:col-start-1 sm:row-start-4 sm:row-span-2 sm:min-h-0 sm:order-none"
            },
            onclick: move |_| focused_panel.set(GitPanel::Commits),
            PanelHeader {
                index: 4,
                title: translate("git-commits"),
                count: Some(repository.commits.len()),
                icon: PanelIcon::Line(LineIcon::Clock),
                icon_class: "bg-sky-400/10 text-sky-400 ring-1 ring-inset ring-sky-400/15",
                badge_class: "border-sky-400/20 bg-sky-400/[0.08] text-sky-400",
                focused: focused_panel() == GitPanel::Commits,
                actions: commit_actions,
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-1 py-1",
                if repository.commits.is_empty() {
                    div { class: "flex h-full min-h-32 items-center justify-center p-4 text-center text-xs text-muted-foreground", {translate("git-no-commits")} }
                }
                for (index, commit) in repository.commits.into_iter().enumerate() {
                    Button {
                        id: "git-commit-row-{index}",
                        variant: ButtonVariant::Ghost,
                        key: "{commit.sha}",
                        title: "{commit.summary} — {commit.author} — {commit.date}",
                        class: if selected_commit() == commit.sha {
                            "h-6 w-full justify-start gap-1.5 rounded-md bg-primary/[0.10] px-1.5 py-0 text-left text-foreground shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--primary)_16%,transparent)]"
                        } else {
                            "h-6 w-full justify-start gap-1.5 rounded-md px-1.5 py-0 text-left text-foreground hover:bg-foreground/[0.045]"
                        },
                        onclick: {
                            let sha = commit.sha.clone();
                            move |_| {
                                focused_panel.set(GitPanel::Commits);
                                selected_commit.set(sha.clone());
                            }
                        },
                        LineIconView { icon: LineIcon::GitCommit, class: "h-3 w-3 shrink-0 text-sky-400/70" }
                        code { class: "shrink-0 font-mono text-[9px] text-sky-400", "{commit.short_sha}" }
                        span { class: "min-w-0 flex-1 truncate text-[11px] font-medium", "{commit.summary}" }
                        span { class: "shrink-0 text-[8px] text-muted-foreground", "{commit.date}" }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn StashCard(
    repository: GitRepositorySnapshot,
    mut selected_stash: Signal<String>,
    mut focused_panel: Signal<GitPanel>,
) -> Element {
    let selected = repository
        .stashes
        .iter()
        .find(|entry| entry.reference == selected_stash())
        .cloned();
    let stashes = repository.stashes.clone();
    let stash_actions = selected.clone().map(|stash| {
        rsx! {
            div { class: "flex shrink-0 items-center gap-0.5",
                HeaderActionButton {
                    icon: LineIcon::Package,
                    shortcut: "g",
                    label: translate("git-stash-pop"),
                    disabled: false,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let reference = stash.reference.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::StashPop { reference: reference.clone() })
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::Trash,
                    shortcut: "d",
                    label: translate("git-stash-drop"),
                    disabled: false,
                    danger: true,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let reference = stash.reference.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::StashDrop { reference: reference.clone() })
                    },
                }
            }
        }
    });

    rsx! {
        Card {
            variant: CardVariant::Panel,
            class: if focused_panel() == GitPanel::Stash {
                "order-7 min-h-28 border-ansi-2/55 shadow-[0_0_0_1px_color-mix(in_oklab,var(--ansi-2)_18%,transparent)] sm:col-start-1 sm:row-start-6 sm:min-h-0 sm:order-none"
            } else {
                "order-7 min-h-28 border-t-rose-400/30 sm:col-start-1 sm:row-start-6 sm:min-h-0 sm:order-none"
            },
            onclick: move |_| focused_panel.set(GitPanel::Stash),
            PanelHeader {
                index: 5,
                title: translate("git-stashes"),
                count: Some(stashes.len()),
                icon: PanelIcon::Line(LineIcon::Package),
                icon_class: "bg-rose-400/10 text-rose-400 ring-1 ring-inset ring-rose-400/15",
                badge_class: "border-rose-400/20 bg-rose-400/[0.08] text-rose-400",
                focused: focused_panel() == GitPanel::Stash,
                actions: stash_actions,
            }
            if !stashes.is_empty() {
                div { class: "min-h-0 flex-1 overflow-y-auto px-1 py-1",
                    for (index, stash) in stashes.into_iter().enumerate() {
                        Button {
                            id: "git-stash-row-{index}",
                            variant: ButtonVariant::Ghost,
                            key: "{stash.reference}",
                            title: "{stash.message}",
                            class: if selected_stash() == stash.reference {
                                "h-6 w-full justify-start gap-1.5 rounded-md bg-primary/[0.10] px-1.5 py-0 text-left text-foreground shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--primary)_16%,transparent)]"
                            } else {
                                "h-6 w-full justify-start gap-1.5 rounded-md px-1.5 py-0 text-left text-foreground hover:bg-foreground/[0.045]"
                            },
                            onclick: {
                                let reference = stash.reference.clone();
                                move |_| {
                                    focused_panel.set(GitPanel::Stash);
                                    selected_stash.set(reference.clone());
                                }
                            },
                            span { class: "shrink-0 rounded bg-rose-400/[0.08] px-1 font-mono text-[8px] text-rose-400", "{stash.index}" }
                            span { class: "min-w-0 flex-1 truncate text-[11px] font-medium", "{stash.message}" }
                            code { class: "shrink-0 font-mono text-[8px] text-muted-foreground", "{stash.reference}" }
                        }
                    }
                }
            }
        }
    }
}
