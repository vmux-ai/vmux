#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ecs::event::space::ProjectActivateRequest;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::dialog::{DialogContent, DialogRoot, DialogTitle};
use vmux_ui::components::dropdown::Dropdown;
use vmux_ui::components::manager::ManagerSearch;
use vmux_ui::components::segmented_control::{SegmentedControl, SegmentedControlItem};
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::event::*;
use crate::state::{
    GitBranchCollection, GitBranchPrompt, GitCatalogProgress, GitOperationEligibility,
};

use super::model::GitPathChangeView;

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
    files: Vec<GitBranchFileEntry>,
    files_total: u32,
    files_loading: bool,
    files_has_more: bool,
    files_error: String,
    selected_path_bytes: Vec<u8>,
    operations: GitOperationEligibility,
    catalog_progress: GitCatalogProgress,
) -> Element {
    let collection = branch_collection;
    let catalog = GitCatalog::from(collection);
    let references = collection.references(&repository);
    let reference_count = references.len();
    let mut branch_open = use_signal(|| false);
    let mut file_query = use_signal(String::new);
    let query = file_query().trim().to_lowercase();
    let files_empty = files.is_empty();
    let visible_files = files
        .into_iter()
        .filter(|entry| query.is_empty() || entry.path.to_lowercase().contains(&query))
        .collect::<Vec<_>>();
    use_effect(move || {
        if !query.is_empty() && files_has_more && !files_loading {
            let _ = send(&GitBranchFilesNextRequest);
        }
    });
    let selected = if collection == GitBranchCollection::Local {
        repository
            .branches
            .iter()
            .find(|entry| entry.name == selected_branch)
            .cloned()
    } else {
        None
    };
    rsx! {
        section { class: "flex h-full min-h-0 flex-col",
            div { class: "flex h-10 shrink-0 items-center gap-1 px-3",
                Dropdown {
                    open: branch_open(),
                    class: "min-w-0 flex-1".to_string(),
                    on_dismiss: move |_| branch_open.set(false),
                    button {
                        r#type: "button",
                        class: "flex h-8 min-w-0 max-w-full items-center gap-2 rounded-md px-2 text-left text-xs font-medium text-foreground hover:bg-foreground/[0.055]",
                        onclick: move |_| branch_open.set(!branch_open()),
                        LineIconView { icon: LineIcon::GitBranch, class: "size-3.5 shrink-0 text-muted-foreground" }
                        span { class: "min-w-0 flex-1 truncate", "{selected_branch}" }
                        LineIconView { icon: LineIcon::ChevronDown, class: "size-3 shrink-0 text-muted-foreground" }
                    }
                    if branch_open() {
                        div { class: "absolute left-0 top-9 z-40 flex max-h-96 min-w-80 flex-col overflow-hidden rounded-lg bg-card p-1.5 shadow-[0_16px_48px_rgba(0,0,0,0.28)]",
                            div { class: "flex shrink-0 items-center gap-2 px-1 pb-1.5",
                                BranchCollectionTabs { branch_collection }
                                span { class: "text-[9px] tabular-nums text-muted-foreground", "{reference_count}" }
                            }
                            div {
                                class: "min-h-0 overflow-y-auto",
                                onscroll: move |event: Event<ScrollData>| {
                                    let remaining = event.scroll_height() - event.scroll_top() as i32 - event.client_height();
                                    if remaining <= 192 && catalog_progress.has_more && !catalog_progress.loading {
                                        let _ = send(&GitCatalogNextRequest { catalog });
                                    }
                                },
                                match collection {
                                    GitBranchCollection::Local => rsx! {
                                        for (index, branch) in repository.branches.iter().cloned().enumerate() {
                                            BranchRow {
                                                key: "local-{branch.name}",
                                                branch,
                                                index,
                                                remote: false,
                                                selected_branch: selected_branch.clone(),
                                                onselected: move |_| branch_open.set(false),
                                            }
                                        }
                                    },
                                    GitBranchCollection::Remote => rsx! {
                                        for (index, branch) in repository.remote_branches.iter().cloned().enumerate() {
                                            BranchRow {
                                                key: "remote-{branch.name}",
                                                branch,
                                                index,
                                                remote: true,
                                                selected_branch: selected_branch.clone(),
                                                onselected: move |_| branch_open.set(false),
                                            }
                                        }
                                    },
                                    GitBranchCollection::Tags => rsx! {
                                        for (index, tag) in repository.tags.iter().cloned().enumerate() {
                                            TagRow {
                                                key: "tag-{tag.name}",
                                                tag,
                                                index,
                                                selected_branch: selected_branch.clone(),
                                                onselected: move |_| branch_open.set(false),
                                            }
                                        }
                                    }
                                }
                                if catalog_progress.loading {
                                    div { class: "flex flex-col gap-2 px-2.5 py-2",
                                        for width in ["w-9/12", "w-11/12"] {
                                            Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "ml-auto flex shrink-0 items-center gap-1",
                    if let Some(branch) = selected {
                        Button {
                            size: ButtonSize::Xs,
                            variant: ButtonVariant::Ghost,
                            class: "h-7 gap-1.5 px-2 text-[10px] font-medium text-muted-foreground hover:text-foreground",
                            disabled: !operations.create_branch,
                            onclick: {
                                let base = branch.name.clone();
                                move |_| {
                                    let _ = send(&GitBranchPromptOpenRequest {
                                        prompt: GitBranchPrompt::Create { base: base.clone() },
                                    });
                                }
                            },
                            LineIconView { icon: LineIcon::Plus, class: "size-3" }
                            span { {translate("git-new-branch")} }
                        }
                        BranchActions {
                            repository: repository.clone(),
                            branch,
                            operations: operations.clone(),
                        }
                    }
                }
            }
            div { class: "shrink-0 px-2 pb-1.5",
                ManagerSearch {
                    value: file_query(),
                    placeholder: translate("common-filter"),
                    onsearch: move |event: FormEvent| file_query.set(event.value()),
                    onkeydown: None,
                }
            }
            div { class: "flex h-7 shrink-0 items-center justify-between px-3 text-[10px] font-semibold text-muted-foreground",
                span { {translate("git-files")} }
                span { class: "tabular-nums", "{files_total}" }
            }
            div {
                class: "min-h-0 flex-1 overflow-y-auto px-2 pb-2",
                onscroll: move |event| {
                    let remaining = event.scroll_height() - event.scroll_top() as i32 - event.client_height();
                    let preload_distance = event.client_height().saturating_mul(2).max(480);
                    if remaining <= preload_distance && files_has_more && !files_loading {
                        let _ = send(&GitBranchFilesNextRequest);
                    }
                },
                if files_loading && files_empty {
                    div { class: "flex flex-col gap-2 px-2.5 py-2",
                        for width in ["w-9/12", "w-11/12", "w-8/12", "w-10/12"] {
                            Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                        }
                    }
                } else if !files_error.is_empty() {
                    div { class: "px-2.5 py-3 text-xs text-destructive", "{files_error}" }
                } else if visible_files.is_empty() {
                    div { class: "flex min-h-24 items-center justify-center px-3 text-center text-xs text-muted-foreground", {translate("git-no-changes")} }
                } else {
                    for entry in visible_files {
                        BranchFileRow {
                            key: "branch-file-{entry.path_bytes:?}",
                            entry,
                            selected_path_bytes: selected_path_bytes.clone(),
                        }
                    }
                    if files_loading {
                        div { class: "flex flex-col gap-2 px-2.5 py-2",
                            for width in ["w-10/12", "w-8/12"] {
                                Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BranchFileRow(entry: GitBranchFileEntry, selected_path_bytes: Vec<u8>) -> Element {
    let selected = selected_path_bytes == entry.path_bytes;
    let path_bytes = entry.path_bytes.clone();
    let seek_path = entry.path.clone();
    let row_id = entry.row_id();
    let status_code = entry.change.code();
    let status_class = entry.change.class();
    rsx! {
        button {
            id: "{row_id}",
            r#type: "button",
            class: if selected {
                "flex h-8 w-full items-center gap-2 rounded-md bg-foreground/[0.085] px-2 text-left text-foreground shadow-[inset_2px_0_0_var(--git-selection-accent)]"
            } else {
                "flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-foreground hover:bg-foreground/[0.045]"
            },
            onclick: move |_| {
                let _ = send(&GitBranchFileSelectRequest {
                    path_bytes: path_bytes.clone(),
                });
                let _ = send(&GitDiffSeekRequest {
                    path: seek_path.clone(),
                    path_bytes: path_bytes.clone(),
                });
            },
            TypeIcon { path: entry.path.clone(), is_dir: false, class: "size-3.5 shrink-0 opacity-85" }
            div { class: "flex min-w-0 flex-1 items-baseline gap-0.5 overflow-hidden text-[11px]",
                if !entry.parent().is_empty() {
                    span { class: "min-w-0 truncate text-muted-foreground", "{entry.parent()}" }
                    span { class: "shrink-0 text-muted-foreground/55", "/" }
                }
                span { class: "max-w-[65%] shrink-0 truncate font-medium", "{entry.name()}" }
            }
            span { class: "w-4 shrink-0 text-center font-mono text-[10px] font-semibold {status_class}", "{status_code}" }
        }
    }
}

#[component]
fn BranchActions(
    repository: GitRepositorySnapshot,
    branch: GitBranchEntry,
    operations: GitOperationEligibility,
) -> Element {
    let mut open = use_signal(|| false);
    let has_actions = operations.checkout_branch
        || operations.delete_branch
        || operations.rebase
        || operations.merge
        || operations.fast_forward;
    if !has_actions {
        return rsx! {};
    }
    rsx! {
        Dropdown {
            open: open(),
            class: "shrink-0".to_string(),
            on_dismiss: move |_| open.set(false),
            button {
                r#type: "button",
                class: "flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground",
                title: translate("git-branches"),
                aria_label: translate("git-branches"),
                onclick: move |_| open.set(!open()),
                LineIconView { icon: LineIcon::Ellipsis, class: "size-3.5" }
            }
            if open() {
                div { class: "absolute right-0 top-8 z-40 min-w-44 rounded-lg bg-card p-1.5 shadow-[0_16px_48px_rgba(0,0,0,0.28)]",
                    if operations.checkout_branch {
                        BranchActionItem {
                            icon: LineIcon::Check,
                            label: translate("git-checkout"),
                            danger: false,
                            onpress: {
                                let repo_root = repository.repo_root.clone();
                                let branch = branch.clone();
                                move |_| {
                                    open.set(false);
                                    let _ = send(&ProjectActivateRequest {
                                        path: repo_root.clone(),
                                        branch: branch.name.clone(),
                                        checkout: branch.checkout.clone(),
                                        pane_id: None,
                                    });
                                }
                            },
                        }
                    }
                    if operations.rebase {
                        BranchActionItem {
                            icon: LineIcon::GitPullRequest,
                            label: translate("git-rebase"),
                            danger: false,
                            onpress: {
                                let repo_root = repository.repo_root.clone();
                                let name = branch.name.clone();
                                move |_| {
                                    open.set(false);
                                    GitOperation::Rebase { branch: name.clone() }.send(repo_root.clone());
                                }
                            },
                        }
                    }
                    if operations.merge {
                        BranchActionItem {
                            icon: LineIcon::GitMerge,
                            label: translate("git-merge"),
                            danger: false,
                            onpress: {
                                let repo_root = repository.repo_root.clone();
                                let name = branch.name.clone();
                                move |_| {
                                    open.set(false);
                                    GitOperation::Merge { branch: name.clone() }.send(repo_root.clone());
                                }
                            },
                        }
                    }
                    if operations.fast_forward {
                        BranchActionItem {
                            icon: LineIcon::FastForward,
                            label: translate("git-fast-forward"),
                            danger: false,
                            onpress: {
                                let repo_root = repository.repo_root.clone();
                                let name = branch.name.clone();
                                move |_| {
                                    open.set(false);
                                    GitOperation::FastForward { branch: name.clone() }.send(repo_root.clone());
                                }
                            },
                        }
                    }
                    if operations.delete_branch {
                        div { class: "mx-1 my-1 h-px bg-foreground/[0.07]" }
                        BranchActionItem {
                            icon: LineIcon::Trash,
                            label: translate("git-delete-branch"),
                            danger: true,
                            onpress: {
                                let name = branch.name.clone();
                                move |_| {
                                    open.set(false);
                                    let _ = send(&GitBranchPromptOpenRequest {
                                        prompt: GitBranchPrompt::Delete { branch: name.clone() },
                                    });
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BranchActionItem(
    icon: LineIcon,
    label: String,
    danger: bool,
    onpress: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: if danger {
                "flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[11px] text-destructive hover:bg-destructive/10"
            } else {
                "flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[11px] text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground"
            },
            onclick: move |_| onpress.call(()),
            LineIconView { icon, class: "size-3.5 shrink-0" }
            span { "{label}" }
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
    onselected: EventHandler<()>,
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
                onselected.call(());
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
                    span { class: "shrink-0 rounded-full bg-git-ahead/10 px-1.5 font-mono text-[8px] text-git-ahead", "↑{branch.ahead}" }
                }
                if branch.behind > 0 {
                    span { class: "shrink-0 rounded-full bg-git-behind/10 px-1.5 font-mono text-[8px] text-git-behind", "↓{branch.behind}" }
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
fn TagRow(
    tag: GitTagEntry,
    index: usize,
    selected_branch: String,
    onselected: EventHandler<()>,
) -> Element {
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
                onselected.call(());
            },
            LineIconView { icon: LineIcon::Tag, class: "size-3 shrink-0 text-git-tag/75" }
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
        "text-success"
    } else if worktree {
        "text-git-worktree"
    } else if remote || !upstream.is_empty() {
        "text-git-reference"
    } else {
        "text-muted-foreground"
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
            class: "flex size-4 shrink-0 items-center justify-center {tone}",
            title,
            LineIconView { icon, class: "size-3.5" }
        }
    }
}
