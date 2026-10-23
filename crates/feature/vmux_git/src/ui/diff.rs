#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::error_state::ErrorState;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};
use vmux_ui::scroll::ScrollIntoView;

use crate::event::GitDiffRow;
use crate::event::{GitDiffViewport, GitRepositorySnapshot};
use crate::ui::DiffView;

#[component]
pub(super) fn BranchDiffCard(
    repository: GitRepositorySnapshot,
    repo_root: ReadSignal<String>,
    selected_branch: ReadSignal<String>,
    selected_path: ReadSignal<String>,
    diff_viewport: ReadSignal<Option<GitDiffViewport>>,
    diff_rows: ReadSignal<Vec<GitDiffRow>>,
    loading: bool,
    error: String,
) -> Element {
    let selected_branch_value = selected_branch();
    let comparison = if repository.base_ref.is_empty() {
        selected_branch_value.clone()
    } else {
        format!("{}…{}", repository.base_ref, selected_branch_value)
    };
    use_effect(move || {
        let path = selected_path();
        let loaded_lines = diff_viewport()
            .as_ref()
            .map(|viewport| viewport.lines.len())
            .unwrap_or_default();
        if path.is_empty() || loaded_lines == 0 {
            return;
        }
        ScrollIntoView::nearest_vertical(&format!("git-diff-file-{path}"));
    });
    let available = !selected_branch_value.is_empty();
    rsx! {
        RevisionDiffCard {
            repo_root,
            diff_viewport,
            diff_rows,
            loading,
            available,
            title: comparison,
            subtitle: String::new(),
            file_path: String::new(),
            empty: translate("git-select-file"),
            icon: LineIcon::File,
            error,
        }
    }
}

#[component]
pub(super) fn CommitDiffCard(
    repository: GitRepositorySnapshot,
    repo_root: ReadSignal<String>,
    selected_commit: ReadSignal<String>,
    diff_viewport: ReadSignal<Option<GitDiffViewport>>,
    diff_rows: ReadSignal<Vec<GitDiffRow>>,
    loading: bool,
) -> Element {
    let selected_commit = selected_commit();
    let selected = repository
        .commits
        .iter()
        .find(|commit| commit.sha == selected_commit)
        .cloned();
    let title = selected
        .as_ref()
        .map(|commit| format!("{}  {}", commit.short_sha, commit.summary))
        .unwrap_or_default();
    rsx! {
        RevisionDiffCard {
            repo_root,
            diff_viewport,
            diff_rows,
            loading,
            available: selected.is_some(),
            title,
            subtitle: selected
                .as_ref()
                .map(|commit| format!("{} · {}", commit.author, commit.date))
                .unwrap_or_default(),
            file_path: String::new(),
            empty: translate("git-no-commits"),
            icon: LineIcon::GitCommit,
            error: String::new(),
        }
    }
}

#[component]
pub(super) fn StashDiffCard(
    repository: GitRepositorySnapshot,
    repo_root: ReadSignal<String>,
    selected_stash: ReadSignal<String>,
    diff_viewport: ReadSignal<Option<GitDiffViewport>>,
    diff_rows: ReadSignal<Vec<GitDiffRow>>,
    loading: bool,
) -> Element {
    let selected_stash = selected_stash();
    let selected = repository
        .stashes
        .iter()
        .find(|stash| stash.reference == selected_stash)
        .cloned();
    rsx! {
        RevisionDiffCard {
            repo_root,
            diff_viewport,
            diff_rows,
            loading,
            available: selected.is_some(),
            title: selected
                .as_ref()
                .map(|stash| stash.message.clone())
                .unwrap_or_default(),
            subtitle: selected_stash,
            file_path: String::new(),
            empty: translate("git-no-stashes"),
            icon: LineIcon::Package,
            error: String::new(),
        }
    }
}

#[component]
fn RevisionDiffCard(
    repo_root: ReadSignal<String>,
    diff_viewport: ReadSignal<Option<GitDiffViewport>>,
    diff_rows: ReadSignal<Vec<GitDiffRow>>,
    loading: bool,
    available: bool,
    title: String,
    subtitle: String,
    file_path: String,
    empty: String,
    icon: LineIcon,
    error: String,
) -> Element {
    let empty_path = use_signal(String::new);
    let identity = title.clone();
    rsx! {
        section { class: "flex h-full min-h-0 flex-col overflow-hidden rounded-lg bg-card/60 ring-1 ring-inset ring-foreground/[0.10]",
            div { class: "flex min-h-12 shrink-0 items-center gap-2.5 bg-foreground/[0.025] px-3",
                if file_path.is_empty() {
                    LineIconView { icon, class: "size-3.5 shrink-0 text-muted-foreground" }
                } else {
                    TypeIcon { path: file_path, is_dir: false, class: "size-4 shrink-0" }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "truncate font-mono text-[11px] font-semibold text-foreground", "{title}" }
                    if !subtitle.is_empty() {
                        div { class: "mt-0.5 flex items-center gap-1 truncate text-[9px] text-muted-foreground",
                            LineIconView { icon: LineIcon::GitBranch, class: "size-2.5 shrink-0" }
                            span { class: "truncate", "{subtitle}" }
                        }
                    }
                }
            }
            if !error.is_empty() {
                ErrorState {
                    class: "flex-1".to_string(),
                    message: error,
                    on_retry: move |_| {
                        let _ = send(&crate::event::GitBranchFilesRetryRequest);
                    },
                }
            } else if available {
                DiffView {
                    repo_root,
                    path: empty_path,
                    identity,
                    viewport: diff_viewport,
                    display_rows: diff_rows,
                    loading,
                    visible: true,
                }
            } else {
                div { class: "flex min-h-64 flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground", "{empty}" }
            }
        }
    }
}

#[component]
pub(super) fn DiffCard(
    repo_root: ReadSignal<String>,
    selected_path: ReadSignal<String>,
    selected_path_bytes: ReadSignal<Vec<u8>>,
    selected_abs_path: ReadSignal<String>,
    diff_viewport: ReadSignal<Option<GitDiffViewport>>,
    diff_rows: ReadSignal<Vec<GitDiffRow>>,
    loading: bool,
) -> Element {
    rsx! {
        section { class: "flex h-full min-h-0 flex-col overflow-hidden rounded-lg bg-card/60 ring-1 ring-inset ring-foreground/[0.10]",
            div { class: "flex h-10 shrink-0 items-center gap-2 bg-card/45 px-3",
                TypeIcon { path: selected_path(), is_dir: false, class: "size-3.5 shrink-0" }
                span { class: "min-w-0 flex-1 truncate font-mono text-[11px] font-medium",
                    if selected_path().is_empty() { {translate("git-select-file")} } else { "{selected_path}" }
                }
            }
            if selected_abs_path().is_empty() {
                div { class: "flex min-h-64 flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground",
                    {translate("git-select-file")}
                }
            } else {
                DiffView { repo_root, path: selected_abs_path, path_bytes: selected_path_bytes(), identity: selected_path(), viewport: diff_viewport, display_rows: diff_rows, loading, visible: true }
            }
        }
    }
}
