#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::dialog::{DialogContent, DialogRoot, DialogTitle};
use vmux_ui::components::error_state::ErrorState;
use vmux_ui::components::manager::ManagerSearch;
use vmux_ui::components::segmented_control::{SegmentedControl, SegmentedControlItem};
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};
use vmux_ui::scroll::ScrollIntoView;

use crate::event::*;
use crate::state::{GitBranchCollection, GitBranchPrompt};

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
    files: Vec<GitBranchFileEntry>,
    files_total: u32,
    files_loading: bool,
    files_has_more: bool,
    files_error: String,
    selected_path_bytes: Vec<u8>,
) -> Element {
    let mut file_query = use_signal(String::new);
    let query = file_query().trim().to_lowercase();
    let files_empty = files.is_empty();
    let visible_files = files
        .into_iter()
        .filter(|entry| query.is_empty() || entry.path.to_lowercase().contains(&query))
        .collect::<Vec<_>>();
    let visible_file_count = visible_files.len();
    rsx! {
        section { class: "flex h-full min-h-0 flex-col",
            div { class: "shrink-0 px-2 pb-1.5 pt-2",
                ManagerSearch {
                    value: file_query(),
                    placeholder: translate("common-filter"),
                    onsearch: move |query: String| file_query.set(query),
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
                    ErrorState {
                        class: "min-h-40".to_string(),
                        message: files_error,
                        on_retry: move |_| {
                            let _ = send(&GitBranchFilesRetryRequest);
                        },
                    }
                } else if visible_files.is_empty() {
                    div { class: "flex min-h-24 items-center justify-center px-3 text-center text-xs text-muted-foreground", {translate("git-no-changes")} }
                    if files_has_more {
                        Button {
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::Xs,
                            class: "w-full".to_string(),
                            onclick: move |_| {
                                let _ = send(&GitBranchFilesNextRequest);
                            },
                            {translate("common-expand")}
                        }
                    }
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
                    } else if files_has_more {
                        div {
                            key: "branch-files-next-{visible_file_count}",
                            class: "flex flex-col gap-2 px-2.5 py-2",
                            onvisible: move |_| {
                                let _ = send(&GitBranchFilesNextRequest);
                            },
                            for width in ["w-10/12", "w-8/12", "w-11/12"] {
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
    let path = entry.path.clone();
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
                ScrollIntoView::nearest_vertical(&format!("git-diff-file-{path}"));
                let _ = send(&GitBranchFileSelectRequest {
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
pub(super) fn BranchCollectionTabs(branch_collection: GitBranchCollection) -> Element {
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
pub(super) fn BranchRow(
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
pub(super) fn TagRow(
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
