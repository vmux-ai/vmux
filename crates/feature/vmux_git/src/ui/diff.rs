#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

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
) -> Element {
    let selected_branch = selected_branch();
    let comparison = if repository.base_ref.is_empty() {
        selected_branch.clone()
    } else {
        format!("{}…{}", repository.base_ref, selected_branch)
    };
    let selected_path = selected_path();
    let title = match selected_path.is_empty() {
        true => comparison,
        false => format!("{selected_path} · {comparison}"),
    };
    let available = !selected_branch.is_empty() && !selected_path.is_empty();
    rsx! {
        RevisionDiffCard {
            repo_root,
            diff_viewport,
            diff_rows,
            loading,
            available,
            title,
            empty: translate("git-select-file"),
            icon: LineIcon::File,
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
            empty: translate("git-no-commits"),
            icon: LineIcon::GitCommit,
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
    empty: String,
    icon: LineIcon,
) -> Element {
    let empty_path = use_signal(String::new);
    rsx! {
        section { class: "flex h-full min-h-0 flex-col",
            div { class: "flex h-10 shrink-0 items-center gap-2 bg-card/45 px-3",
                LineIconView { icon, class: "size-3.5 shrink-0 text-muted-foreground" }
                span { class: "min-w-0 flex-1 truncate text-[11px] font-medium", "{title}" }
            }
            if available {
                DiffView {
                    repo_root,
                    path: empty_path,
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
        section { class: "flex h-full min-h-0 flex-col",
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
                DiffView { repo_root, path: selected_abs_path, path_bytes: selected_path_bytes(), viewport: diff_viewport, display_rows: diff_rows, loading, visible: true }
            }
        }
    }
}
