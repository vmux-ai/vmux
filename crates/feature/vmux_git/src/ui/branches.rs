#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ecs::event::space::ProjectActivateRequest;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::dialog::{DialogContent, DialogRoot, DialogTitle};
use vmux_ui::components::segmented_control::{SegmentedControl, SegmentedControlItem};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::event::*;
use crate::state::{GitBranchCollection, GitBranchPrompt, GitOperationEligibility};

use super::operation::HeaderOperationButton;
use super::shared::ListPager;

#[component]
pub(super) fn BranchPromptDialog() -> Element {
    let state = use_context::<super::state::GitPageState>();
    let controller = (state.controller)();
    let draft = controller.branch_draft;
    let Some(prompt) = controller.branch_prompt else {
        return rsx! {};
    };
    let title = match &prompt {
        GitBranchPrompt::Create { .. } => translate("git-new-branch"),
        GitBranchPrompt::Delete { .. } => translate("git-delete-branch"),
    };
    let is_create = matches!(prompt, GitBranchPrompt::Create { .. });
    let submit_ready = !draft.trim().is_empty();
    rsx! {
        DialogRoot {
            open: true,
            on_open_change: move |open: bool| {
                if !open {
                    let _ = send(&GitBranchPromptCloseRequest);
                }
            },
            attributes: vec![],
            DialogContent { class: "max-w-[360px] p-4", attributes: vec![],
                DialogTitle { attributes: vec![], "{title}" }
                if is_create {
                    input {
                        class: "w-full rounded-md border border-border bg-foreground/[0.04] px-3 py-2 text-sm text-foreground outline-none transition-colors focus:border-primary/50",
                        autofocus: true,
                        value: "{draft}",
                        placeholder: translate("git-new-branch"),
                        oninput: move |event: Event<FormData>| {
                            let _ = send(&GitBranchDraftRequest { draft: event.value() });
                        },
                        onkeydown: move |event: KeyboardEvent| {
                            event.stop_propagation();
                            if event.key() == Key::Enter && submit_ready {
                                event.prevent_default();
                                let _ = send(&GitBranchSubmitRequest);
                            }
                        },
                    }
                } else if let GitBranchPrompt::Delete { branch } = &prompt {
                    code { class: "rounded-md bg-foreground/[0.05] px-2 py-1 text-sm text-foreground", "{branch}" }
                }
                div { class: "flex justify-end gap-2",
                    Button {
                        size: ButtonSize::Xs,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| {
                            let _ = send(&GitBranchPromptCloseRequest);
                        },
                        {translate("common-cancel")}
                    }
                    Button {
                        size: ButtonSize::Xs,
                        variant: if is_create { ButtonVariant::Primary } else { ButtonVariant::Destructive },
                        disabled: is_create && !submit_ready,
                        onclick: move |_| {
                            let _ = send(&GitBranchSubmitRequest);
                        },
                        {if is_create { translate("common-save") } else { translate("common-delete") }}
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn BranchesCard(
    repository: GitRepositorySnapshot,
    selected_branch: String,
    branch_collection: GitBranchCollection,
    operations: GitOperationEligibility,
) -> Element {
    let collection = branch_collection;
    let references = collection.references(&repository);
    let reference_count = references.len();
    let pager = ListPager::use_state();
    let selected_index = references
        .iter()
        .position(|reference| reference == &selected_branch);
    let visible_count = pager
        .visible(reference_count)
        .max(selected_index.map(|index| index + 1).unwrap_or_default());
    let selected = if collection == GitBranchCollection::Local {
        repository
            .branches
            .iter()
            .find(|entry| entry.name == selected_branch)
            .cloned()
    } else {
        None
    };
    let branch_operations = selected.map(|branch| {
        rsx! {
            div { class: "flex shrink-0 items-center gap-0.5",
                HeaderOperationButton {
                    icon: LineIcon::Check,
                    shortcut: "space",
                    label: translate("git-checkout"),
                    disabled: !operations.checkout_branch,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let branch = branch.clone();
                        move |_| {
                            let _ = send(&ProjectActivateRequest {
                                path: repo_root.clone(),
                                branch: branch.name.clone(),
                                checkout: branch.checkout.clone(),
                                pane_id: None,
                            });
                        }
                    },
                }
                HeaderOperationButton {
                    icon: LineIcon::Plus,
                    shortcut: "n",
                    label: translate("git-new-branch"),
                    disabled: !operations.create_branch,
                    danger: false,
                    onpress: {
                        let base = branch.name.clone();
                        move |_| {
                            let _ = send(&GitBranchPromptOpenRequest {
                                prompt: GitBranchPrompt::Create { base: base.clone() },
                            });
                        }
                    },
                }
                HeaderOperationButton {
                    icon: LineIcon::Trash,
                    shortcut: "d",
                    label: translate("git-delete-branch"),
                    disabled: !operations.delete_branch,
                    danger: true,
                    onpress: {
                        let branch = branch.name.clone();
                        move |_| {
                            let _ = send(&GitBranchPromptOpenRequest {
                                prompt: GitBranchPrompt::Delete { branch: branch.clone() },
                            });
                        }
                    },
                }
                HeaderOperationButton {
                    icon: LineIcon::GitPullRequest,
                    shortcut: "r",
                    label: translate("git-rebase"),
                    disabled: !operations.rebase,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let branch = branch.name.clone();
                        move |_| GitOperation::Rebase { branch: branch.clone() }.send(repo_root.clone())
                    },
                }
                HeaderOperationButton {
                    icon: LineIcon::GitMerge,
                    shortcut: "M",
                    label: translate("git-merge"),
                    disabled: !operations.merge,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let branch = branch.name.clone();
                        move |_| GitOperation::Merge { branch: branch.clone() }.send(repo_root.clone())
                    },
                }
                HeaderOperationButton {
                    icon: LineIcon::FastForward,
                    shortcut: "f",
                    label: translate("git-fast-forward"),
                    disabled: !operations.fast_forward,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let branch = branch.name.clone();
                        move |_| GitOperation::FastForward { branch: branch.clone() }.send(repo_root.clone())
                    },
                }
            }
        }
    });

    rsx! {
        section { class: "flex h-full min-h-0 flex-col",
            div { class: "flex h-10 shrink-0 items-center gap-1 px-3",
                BranchCollectionTabs {
                    branch_collection,
                }
                span { class: "text-[9px] tabular-nums text-muted-foreground", "{reference_count}" }
                div { class: "ml-auto flex shrink-0 items-center gap-1",
                    {branch_operations}
                }
            }
            div {
                class: "min-h-0 flex-1 overflow-y-auto px-2 py-1.5",
                onscroll: move |event| pager.load_more(event, reference_count),
                match collection {
                    GitBranchCollection::Local => rsx! {
                        for (index, branch) in repository.branches.into_iter().take(visible_count).enumerate() {
                            BranchRow {
                                key: "local-{branch.name}",
                                branch,
                                index,
                                remote: false,
                                selected_branch: selected_branch.clone(),
                            }
                        }
                    },
                    GitBranchCollection::Remote => rsx! {
                        for (index, branch) in repository.remote_branches.into_iter().take(visible_count).enumerate() {
                            BranchRow {
                                key: "remote-{branch.name}",
                                branch,
                                index,
                                remote: true,
                                selected_branch: selected_branch.clone(),
                            }
                        }
                    },
                    GitBranchCollection::Tags => rsx! {
                        for (index, tag) in repository.tags.into_iter().take(visible_count).enumerate() {
                            TagRow {
                                key: "tag-{tag.name}",
                                tag,
                                index,
                                selected_branch: selected_branch.clone(),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BranchCollectionTabs(branch_collection: GitBranchCollection) -> Element {
    let active = match branch_collection {
        GitBranchCollection::Local => "local",
        GitBranchCollection::Remote => "remote",
        GitBranchCollection::Tags => "tags",
    };
    let items = vec![
        SegmentedControlItem {
            id: "local".to_string(),
            label: translate("git-local"),
        },
        SegmentedControlItem {
            id: "remote".to_string(),
            label: translate("git-remote"),
        },
        SegmentedControlItem {
            id: "tags".to_string(),
            label: translate("git-tags"),
        },
    ];
    rsx! {
        SegmentedControl {
            active: active.to_string(),
            items,
            onselect: move |id: String| {
                let collection = match id.as_str() {
                    "remote" => GitBranchCollection::Remote,
                    "tags" => GitBranchCollection::Tags,
                    _ => GitBranchCollection::Local,
                };
                let _ = send(&GitBranchCollectionSelectRequest { collection });
            },
        }
    }
}

#[component]
fn BranchRow(
    branch: GitBranchEntry,
    index: usize,
    remote: bool,
    selected_branch: String,
) -> Element {
    let name = branch.name.clone();
    rsx! {
        button {
            id: "git-branch-row-{index}",
            r#type: "button",
            title: if branch.checkout.is_empty() { branch.upstream.clone() } else { branch.checkout.clone() },
            class: if selected_branch == branch.name {
                "flex h-9 w-full items-center justify-start gap-2 rounded-md bg-primary/[0.10] px-2.5 text-left text-foreground"
            } else {
                "flex h-9 w-full items-center justify-start gap-2 rounded-md px-2.5 text-left text-foreground hover:bg-foreground/[0.045]"
            },
            onclick: move |_| {
                let _ = send(&GitBranchSelectRequest {
                    reference: name.clone(),
                });
            },
            BranchMarker {
                current: branch.current,
                upstream: branch.upstream.clone(),
                worktree: !branch.checkout.is_empty(),
                remote,
            }
            div { class: "flex min-w-0 flex-1 items-center gap-1.5",
                span { class: "min-w-0 flex-1 truncate text-xs font-medium", "{branch.name}" }
                if branch.ahead > 0 {
                    span { class: "shrink-0 rounded-full bg-sky-400/10 px-1.5 font-mono text-[8px] text-sky-400", "↑{branch.ahead}" }
                }
                if branch.behind > 0 {
                    span { class: "shrink-0 rounded-full bg-violet-400/10 px-1.5 font-mono text-[8px] text-violet-400", "↓{branch.behind}" }
                }
                if branch.changed_files > 0 {
                    span {
                        class: "flex shrink-0 items-center gap-1 font-mono text-[8px] tabular-nums text-muted-foreground",
                        title: translate("git-branch-changes"),
                        LineIconView { icon: LineIcon::File, class: "size-2.5" }
                        span { class: "text-foreground/80", "{branch.changed_files}" }
                        span { class: "text-success", "+{branch.insertions}" }
                        span { class: "text-destructive", "−{branch.deletions}" }
                    }
                }
                if branch.current {
                    span { class: "shrink-0 rounded-full bg-ansi-2/10 px-1.5 text-[8px] font-medium text-ansi-2", {translate("common-current")} }
                } else if !branch.upstream.is_empty() {
                    span { class: "max-w-[42%] shrink truncate font-mono text-[8px] text-muted-foreground", "{branch.upstream}" }
                }
            }
        }
    }
}

#[component]
fn TagRow(tag: GitTagEntry, index: usize, selected_branch: String) -> Element {
    let name = tag.name.clone();
    rsx! {
        button {
            id: "git-branch-row-{index}",
            r#type: "button",
            title: "{tag.message}",
            class: if selected_branch == tag.name {
                "flex h-9 w-full items-center justify-start gap-2 rounded-md bg-primary/[0.10] px-2.5 text-left text-foreground"
            } else {
                "flex h-9 w-full items-center justify-start gap-2 rounded-md px-2.5 text-left text-foreground hover:bg-foreground/[0.045]"
            },
            onclick: move |_| {
                let _ = send(&GitBranchSelectRequest {
                    reference: name.clone(),
                });
            },
            LineIconView { icon: LineIcon::Tag, class: "size-3 shrink-0 text-violet-400/75" }
            span { class: "min-w-0 flex-1 truncate text-xs font-medium", "{tag.name}" }
            code { class: "shrink-0 font-mono text-[8px] text-muted-foreground", "{tag.short_sha}" }
            span { class: "shrink-0 text-[8px] text-muted-foreground", "{tag.date}" }
        }
    }
}

#[component]
fn BranchMarker(current: bool, upstream: String, worktree: bool, remote: bool) -> Element {
    let icon = if worktree {
        LineIcon::GitFork
    } else {
        LineIcon::GitBranch
    };
    let tone = if current {
        "bg-ansi-2/12 text-ansi-2 ring-ansi-2/20"
    } else if worktree {
        "bg-amber-400/10 text-amber-400 ring-amber-400/20"
    } else if remote || !upstream.is_empty() {
        "bg-sky-400/10 text-sky-400 ring-sky-400/20"
    } else {
        "bg-violet-400/10 text-violet-400 ring-violet-400/20"
    };
    let title = if current {
        translate("common-current")
    } else if worktree {
        translate("layout-worktree")
    } else if remote {
        translate("git-remote")
    } else if !upstream.is_empty() {
        upstream
    } else {
        translate("git-local")
    };
    rsx! {
        span {
            class: "flex size-4 shrink-0 items-center justify-center rounded-md ring-1 ring-inset {tone}",
            title,
            LineIconView { icon, class: "size-2.5" }
        }
    }
}
