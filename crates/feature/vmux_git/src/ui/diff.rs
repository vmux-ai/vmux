#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::event::GitDiffRow;
use crate::event::{GitDiffViewport, GitRepositorySnapshot};
use crate::ui::DiffView;

#[component]
pub(super) fn CommitDiffCard(
    repository: GitRepositorySnapshot,
    repo_root: ReadSignal<String>,
    selected_commit: ReadSignal<String>,
    diff_viewport: ReadSignal<Option<GitDiffViewport>>,
    diff_rows: ReadSignal<Vec<GitDiffRow>>,
    loading: bool,
) -> Element {
    let empty_path = use_signal(String::new);
    let selected_commit = selected_commit();
    let selected = repository
        .commits
        .iter()
        .find(|commit| commit.sha == selected_commit)
        .cloned();
    let branch_diff = selected_commit.is_empty() && !repository.commits.is_empty();
    let branch_head = if repository.branch.is_empty() {
        "HEAD"
    } else {
        repository.branch.as_str()
    };
    let title = if branch_diff {
        if repository.base_ref.is_empty() {
            translate("git-branch-changes")
        } else {
            format!("{}…{}", repository.base_ref, branch_head)
        }
    } else {
        selected
            .as_ref()
            .map(|commit| format!("{}  {}", commit.short_sha, commit.summary))
            .unwrap_or_else(|| translate("git-no-commits"))
    };
    rsx! {
        section { class: "flex h-full min-h-0 flex-col",
            div { class: "flex h-10 shrink-0 items-center gap-2 bg-card/45 px-3",
                LineIconView {
                    icon: if branch_diff { LineIcon::GitBranch } else { LineIcon::GitCommit },
                    class: "size-3.5 shrink-0 text-muted-foreground",
                }
                span { class: "min-w-0 flex-1 truncate text-[11px] font-medium", "{title}" }
            }
            if !branch_diff && selected.is_none() {
                div { class: "flex min-h-64 flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground",
                    {translate("git-no-commits")}
                }
            } else {
                DiffView {
                    repo_root,
                    path: empty_path,
                    viewport: diff_viewport,
                    display_rows: diff_rows,
                    loading,
                    visible: true,
                }
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
