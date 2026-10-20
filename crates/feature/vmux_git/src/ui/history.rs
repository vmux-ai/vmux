#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::event::*;
use crate::state::GitOperationEligibility;

use super::operation::HeaderOperationButton;
use super::shared::ListPager;

#[component]
pub(super) fn HistoryCard(
    repository: GitRepositorySnapshot,
    selected_commit: String,
    operations: GitOperationEligibility,
) -> Element {
    let total = repository.commits.len();
    let pager = ListPager::use_state();
    let selected_index = repository
        .commits
        .iter()
        .position(|entry| entry.sha == selected_commit);
    let visible_count = pager
        .visible(total)
        .max(selected_index.map(|index| index + 1).unwrap_or_default());
    let selected = repository
        .commits
        .iter()
        .find(|entry| entry.sha == selected_commit)
        .cloned();
    let commit_operations = selected.clone().map(|commit| {
        rsx! {
            div { class: "flex shrink-0 items-center gap-0.5",
                HeaderOperationButton {
                    icon: LineIcon::Check,
                    shortcut: "space",
                    label: translate("git-checkout-commit"),
                    disabled: !operations.checkout_commit,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let commit = commit.sha.clone();
                        move |_| GitOperation::CheckoutCommit { commit: commit.clone() }.send(repo_root.clone())
                    },
                }
                HeaderOperationButton {
                    icon: LineIcon::GitCommit,
                    shortcut: "C",
                    label: translate("git-cherry-pick"),
                    disabled: !operations.cherry_pick,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let commit = commit.sha.clone();
                        move |_| GitOperation::CherryPick { commit: commit.clone() }.send(repo_root.clone())
                    },
                }
                HeaderOperationButton {
                    icon: LineIcon::RotateCcw,
                    shortcut: "t",
                    label: translate("git-revert-commit"),
                    disabled: !operations.revert_commit,
                    danger: true,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let commit = commit.sha.clone();
                        move |_| GitOperation::Revert { commit: commit.clone() }.send(repo_root.clone())
                    },
                }
            }
        }
    });

    rsx! {
        section { class: "flex h-full min-h-0 flex-col",
            div { class: "flex h-10 shrink-0 items-center justify-end px-3",
                {commit_operations}
            }
            div {
                class: "min-h-0 flex-1 overflow-y-auto px-2 py-1.5",
                onscroll: move |event| pager.load_more(event, total),
                if repository.commits.is_empty() {
                    div { class: "flex h-full min-h-32 items-center justify-center p-4 text-center text-xs text-muted-foreground", {translate("git-no-commits")} }
                }
                for (index, commit) in repository.commits.into_iter().take(visible_count).enumerate() {
                    Button {
                        id: "git-commit-row-{index}",
                        variant: ButtonVariant::Ghost,
                        key: "{commit.sha}",
                        title: "{commit.summary} — {commit.author} — {commit.date}",
                        class: if selected_commit == commit.sha {
                            "h-9 w-full justify-start gap-2 rounded-md bg-foreground/[0.075] px-2.5 py-0 text-left text-foreground"
                        } else {
                            "h-9 w-full justify-start gap-2 rounded-md px-2.5 py-0 text-left text-foreground hover:bg-foreground/[0.04]"
                        },
                        onclick: {
                            let sha = commit.sha.clone();
                            move |_| {
                                let _ = send(&GitCommitSelectRequest {
                                    commit: sha.clone(),
                                });
                            }
                        },
                        LineIconView { icon: LineIcon::GitCommit, class: "h-3 w-3 shrink-0 text-git-reference/70" }
                        code { class: "shrink-0 font-mono text-[9px] text-git-reference", "{commit.short_sha}" }
                        span { class: "min-w-0 flex-1 truncate text-xs font-medium", "{commit.summary}" }
                        span { class: "shrink-0 text-[10px] text-muted-foreground", "{commit.date}" }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn StashCard(
    repository: GitRepositorySnapshot,
    selected_stash: String,
    operations: GitOperationEligibility,
) -> Element {
    let total = repository.stashes.len();
    let pager = ListPager::use_state();
    let selected_index = repository
        .stashes
        .iter()
        .position(|entry| entry.reference == selected_stash);
    let visible_count = pager
        .visible(total)
        .max(selected_index.map(|index| index + 1).unwrap_or_default());
    let selected = repository
        .stashes
        .iter()
        .find(|entry| entry.reference == selected_stash)
        .cloned();
    let stashes = repository.stashes.clone();
    let stash_operations = selected.clone().map(|stash| {
        rsx! {
            div { class: "flex shrink-0 items-center gap-0.5",
                HeaderOperationButton {
                    icon: LineIcon::Package,
                    shortcut: "g",
                    label: translate("git-stash-pop"),
                    disabled: !operations.stash_pop,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let reference = stash.reference.clone();
                        move |_| GitOperation::StashPop { reference: reference.clone() }.send(repo_root.clone())
                    },
                }
                HeaderOperationButton {
                    icon: LineIcon::Trash,
                    shortcut: "d",
                    label: translate("git-stash-drop"),
                    disabled: !operations.stash_drop,
                    danger: true,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let reference = stash.reference.clone();
                        move |_| GitOperation::StashDrop { reference: reference.clone() }.send(repo_root.clone())
                    },
                }
            }
        }
    });

    rsx! {
        section { class: "flex h-full min-h-0 flex-col",
            div { class: "flex h-10 shrink-0 items-center justify-end px-3",
                {stash_operations}
            }
            if !stashes.is_empty() {
                div {
                    class: "min-h-0 flex-1 overflow-y-auto px-2 py-1.5",
                    onscroll: move |event| pager.load_more(event, total),
                    for (index, stash) in stashes.into_iter().take(visible_count).enumerate() {
                        Button {
                            id: "git-stash-row-{index}",
                            variant: ButtonVariant::Ghost,
                            key: "{stash.reference}",
                            title: "{stash.message}",
                            class: if selected_stash == stash.reference {
                                "h-9 w-full justify-start gap-2 rounded-md bg-foreground/[0.075] px-2.5 py-0 text-left text-foreground"
                            } else {
                                "h-9 w-full justify-start gap-2 rounded-md px-2.5 py-0 text-left text-foreground hover:bg-foreground/[0.04]"
                            },
                            onclick: {
                                let reference = stash.reference.clone();
                                move |_| {
                                    let _ = send(&GitStashSelectRequest {
                                        reference: reference.clone(),
                                    });
                                }
                            },
                            span { class: "shrink-0 rounded bg-rose-400/[0.08] px-1 font-mono text-[8px] text-rose-400", "{stash.index}" }
                            span { class: "min-w-0 flex-1 truncate text-xs font-medium", "{stash.message}" }
                            code { class: "shrink-0 font-mono text-[8px] text-muted-foreground", "{stash.reference}" }
                        }
                    }
                }
            }
        }
    }
}
