#![allow(non_snake_case)]

use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use vmux_api::space::ProjectRow;

use super::branches::BranchesCard;
use super::changes::ChangesCard;
use super::diff::{BranchDiffCard, CommitDiffCard, DiffCard, StashDiffCard};
use super::history::{HistoryCard, StashCard};
use super::shortcuts::GitShortcutHelp;
use super::state::GitPageState;
use super::status::RepositoryHeader;
use crate::event::{GitBranchFileEntry, GitRepositorySnapshot};
use crate::state::{
    GitBranchCollection, GitCatalogProgress, GitOperationEligibility, GitPanel, HeaderDropdown,
};

#[component]
pub(super) fn GitDashboard() -> Element {
    let state = use_context::<GitPageState>();
    let repository_loaded = state.use_selector(|state| state.snapshot.repository.is_some());
    let active_branch_file = state.active_branch_file;
    let branch_reveal_path = use_signal(String::new);
    let file_query = use_signal(String::new);
    let viewed_count = use_signal(|| 0usize);
    let mut list_width = use_signal(|| 336.0_f64);
    let mut resizing = use_signal(|| false);
    let mut resize_origin = use_signal(|| None::<(f64, f64)>);
    let split_style = format!("--git-list-width:{}px;", list_width());

    if !repository_loaded() {
        return rsx! {};
    }

    rsx! {
        GitHeader { viewed_count, file_query }
        main { class: "flex min-h-0 flex-1 flex-col bg-transparent",
            div {
                id: "git-dashboard-split",
                class: "grid min-h-0 flex-1 grid-cols-1 grid-rows-[minmax(12rem,0.7fr)_minmax(18rem,1.3fr)] sm:grid-cols-[minmax(18rem,var(--git-list-width))_0.75rem_minmax(0,1fr)] sm:grid-rows-1",
                style: "{split_style}",
                GitListPane { active_branch_file, branch_reveal_path, file_query }
                div {
                    class: "group relative z-30 hidden h-full w-full cursor-col-resize items-center justify-center sm:flex",
                    onpointerdown: move |event: Event<PointerData>| {
                        if event.trigger_button() != Some(MouseButton::Primary) {
                            return;
                        }
                        event.prevent_default();
                        resize_origin.set(Some((event.client_coordinates().x, list_width())));
                        resizing.set(true);
                    },
                    div { class: "pointer-events-none h-10 w-1 rounded-full bg-foreground/20 opacity-0 transition-opacity duration-150 group-hover:opacity-100" }
                }
                GitDiffPane {
                    active_branch_file,
                    branch_reveal_path,
                    viewed_count,
                }
            }
        }
        if resizing() {
            div {
                class: "fixed inset-0 z-[900] cursor-col-resize select-none touch-none",
                onpointermove: move |event: Event<PointerData>| {
                    let Some((start, width)) = resize_origin() else {
                        return;
                    };
                    if !event.held_buttons().contains(MouseButton::Primary) {
                        resize_origin.set(None);
                        resizing.set(false);
                        return;
                    }
                    let width = (width + event.client_coordinates().x - start).clamp(288.0, 640.0);
                    list_width.set(width);
                },
                onpointerup: move |event: Event<PointerData>| {
                    if let Some((start, width)) = resize_origin() {
                        list_width.set(
                            (width + event.client_coordinates().x - start).clamp(288.0, 640.0),
                        );
                    }
                    resize_origin.set(None);
                    resizing.set(false);
                },
                onpointercancel: move |_| {
                    resize_origin.set(None);
                    resizing.set(false);
                },
            }
        }
        GitShortcutOverlay {}
    }
}

#[derive(Clone, PartialEq)]
struct HeaderState {
    repository: Option<GitRepositorySnapshot>,
    open_dropdown: HeaderDropdown,
    projects: Vec<ProjectRow>,
    focused_panel: GitPanel,
    selected_branch: String,
    selected_branch_base: String,
    branch_collection: GitBranchCollection,
    selected_branch_collection: GitBranchCollection,
    selected_commit: String,
    can_push: bool,
    branches_progress: GitCatalogProgress,
    selected_branches_progress: GitCatalogProgress,
    commits_progress: GitCatalogProgress,
    viewed_total: u32,
}

#[component]
fn GitHeader(viewed_count: ReadSignal<usize>, file_query: Signal<String>) -> Element {
    let state = use_context::<GitPageState>();
    let header = state.use_selector(|state| {
        let snapshot = &state.snapshot;
        let controller = &state.controller;
        HeaderState {
            repository: snapshot.repository.clone(),
            open_dropdown: state.header_dropdown,
            projects: state.projects.rows.clone(),
            focused_panel: controller.focused_panel,
            selected_branch: controller.selected_branch.clone(),
            selected_branch_base: snapshot.branch_files_base_ref.clone(),
            branch_collection: controller.branch_collection,
            selected_branch_collection: controller.selected_branch_collection,
            selected_commit: controller.selected_commit.clone(),
            can_push: controller.operations.push,
            branches_progress: snapshot.local_branches,
            selected_branches_progress: snapshot.catalog(controller.branch_collection.into()),
            commits_progress: snapshot.commits,
            viewed_total: snapshot.branch_files_total,
        }
    });
    let header = header();
    let Some(repository) = header.repository else {
        return rsx! {};
    };
    rsx! {
        RepositoryHeader {
            repository,
            open_dropdown: header.open_dropdown,
            projects: header.projects,
            focused_panel: header.focused_panel,
            selected_branch: header.selected_branch,
            selected_branch_base: header.selected_branch_base,
            branch_collection: header.branch_collection,
            selected_branch_collection: header.selected_branch_collection,
            selected_commit: header.selected_commit,
            can_push: header.can_push,
            branches_progress: header.branches_progress,
            selected_branches_progress: header.selected_branches_progress,
            commits_progress: header.commits_progress,
            viewed_count: viewed_count(),
            viewed_total: header.viewed_total,
            file_query,
        }
    }
}

#[derive(Clone, PartialEq)]
struct ListState {
    repository: Option<GitRepositorySnapshot>,
    panel: GitPanel,
    selected_path_bytes: Vec<u8>,
    selected_branch: String,
    selected_branch_path_bytes: Vec<u8>,
    selected_commit: String,
    selected_stash: String,
    confirm_discard: Vec<u8>,
    commit_message: String,
    commit_pending: String,
    operations: GitOperationEligibility,
    branch_files: Vec<GitBranchFileEntry>,
    branch_diff_paths: Vec<String>,
    branch_files_reference: String,
    branch_files_total: u32,
    branch_files_loading: bool,
    branch_files_has_more: bool,
    branch_files_error: String,
    commits: GitCatalogProgress,
    stashes: GitCatalogProgress,
}

#[component]
fn GitListPane(
    active_branch_file: Signal<String>,
    branch_reveal_path: Signal<String>,
    file_query: Signal<String>,
) -> Element {
    let state = use_context::<GitPageState>();
    let list = state.use_selector(|state| {
        let snapshot = &state.snapshot;
        let controller = &state.controller;
        ListState {
            repository: snapshot.repository.clone(),
            panel: controller.focused_panel,
            selected_path_bytes: controller.selected_path_bytes.clone(),
            selected_branch: controller.selected_branch.clone(),
            selected_branch_path_bytes: controller.selected_branch_path_bytes.clone(),
            selected_commit: controller.selected_commit.clone(),
            selected_stash: controller.selected_stash.clone(),
            confirm_discard: controller.confirm_discard.clone(),
            commit_message: controller.commit_message.clone(),
            commit_pending: controller.commit_pending.clone(),
            operations: controller.operations.clone(),
            branch_files: snapshot.branch_files.clone(),
            branch_diff_paths: snapshot.branch_diff_paths.clone(),
            branch_files_reference: snapshot.branch_files_reference.clone(),
            branch_files_total: snapshot.branch_files_total,
            branch_files_loading: snapshot.branch_files_loading,
            branch_files_has_more: snapshot.branch_files_has_more,
            branch_files_error: snapshot.branch_files_error.clone(),
            commits: snapshot.commits,
            stashes: snapshot.stashes,
        }
    });
    let list = list();
    let Some(repository) = list.repository else {
        return rsx! {};
    };
    rsx! {
        aside { class: "relative z-20 flex min-h-0 flex-col overflow-visible bg-transparent",
            div { class: "min-h-0 flex-1",
                if list.panel == GitPanel::Files {
                    ChangesCard {
                        repository,
                        selected_path_bytes: list.selected_path_bytes,
                        confirm_discard: list.confirm_discard,
                        commit_message: list.commit_message,
                        commit_pending: list.commit_pending,
                        operations: list.operations,
                    }
                } else if list.panel == GitPanel::Branches {
                    BranchesCard {
                        files: list.branch_files,
                        files_ready: list.branch_files_reference == list.selected_branch,
                        files_total: list.branch_files_total,
                        files_loading: list.branch_files_loading,
                        files_has_more: list.branch_files_has_more,
                        files_error: list.branch_files_error,
                        selected_path_bytes: list.selected_branch_path_bytes,
                        active_path: active_branch_file,
                        file_query,
                        onfileselect: move |path: String| {
                            let mut active_branch_file = active_branch_file;
                            let mut branch_reveal_path = branch_reveal_path;
                            active_branch_file.set(path.clone());
                            branch_reveal_path.set(path);
                        },
                    }
                } else if list.panel == GitPanel::Commits {
                    HistoryCard {
                        repository,
                        selected_commit: list.selected_commit,
                        operations: list.operations,
                        progress: list.commits,
                    }
                } else {
                    StashCard {
                        repository,
                        selected_stash: list.selected_stash,
                        operations: list.operations,
                        progress: list.stashes,
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct DiffState {
    repository: Option<GitRepositorySnapshot>,
    panel: GitPanel,
    workspace: String,
    selected_path: String,
    selected_path_bytes: Vec<u8>,
    selected_abs_path: String,
    selected_commit: String,
    selected_branch: String,
    selected_branch_base: String,
    selected_stash: String,
    branch_files: Vec<GitBranchFileEntry>,
    branch_diff_paths: Vec<String>,
    branch_files_error: String,
}

#[component]
fn GitDiffPane(
    active_branch_file: Signal<String>,
    branch_reveal_path: Signal<String>,
    viewed_count: Signal<usize>,
) -> Element {
    let state = use_context::<GitPageState>();
    let selected = state.use_selector(|state| {
        let snapshot = &state.snapshot;
        let controller = &state.controller;
        DiffState {
            repository: snapshot.repository.clone(),
            panel: controller.focused_panel,
            workspace: snapshot.workspace.clone(),
            selected_path: controller.selected_path.clone(),
            selected_path_bytes: controller.selected_path_bytes.clone(),
            selected_abs_path: controller.selected_abs_path.clone(),
            selected_commit: controller.selected_commit.clone(),
            selected_branch: controller.selected_branch.clone(),
            selected_branch_base: snapshot.branch_files_base_ref.clone(),
            selected_stash: controller.selected_stash.clone(),
            branch_files: snapshot.branch_files.clone(),
            branch_diff_paths: snapshot.branch_diff_paths.clone(),
            branch_files_error: snapshot.branch_files_error.clone(),
        }
    });
    let repo_root = use_memo(move || selected.read().workspace.clone());
    let selected_path = use_memo(move || selected.read().selected_path.clone());
    let selected_path_bytes = use_memo(move || selected.read().selected_path_bytes.clone());
    let selected_abs_path = use_memo(move || selected.read().selected_abs_path.clone());
    let selected_commit = use_memo(move || selected.read().selected_commit.clone());
    let selected_stash = use_memo(move || selected.read().selected_stash.clone());
    let selected_branch = use_memo(move || selected.read().selected_branch.clone());
    let diff = state.diff;
    use_effect(move || {
        selected_branch.read();
        let mut active_branch_file = active_branch_file;
        let mut branch_reveal_path = branch_reveal_path;
        let mut viewed_count = viewed_count;
        active_branch_file.set(String::new());
        branch_reveal_path.set(String::new());
        viewed_count.set(0);
    });
    let selected_state = selected();
    let Some(repository) = selected_state.repository else {
        return rsx! {};
    };
    rsx! {
        section { class: "min-h-0 min-w-0 overflow-hidden bg-transparent py-2.5 pr-2.5",
            if selected_state.panel == GitPanel::Branches {
                BranchDiffCard {
                    base_ref: selected_state.selected_branch_base,
                    files: selected_state.branch_files,
                    expected_paths: selected_state.branch_diff_paths,
                    active_path: active_branch_file,
                    repo_root,
                    selected_branch,
                    reveal_path: branch_reveal_path,
                    diff_viewport: diff.viewport,
                    diff_rows: diff.rows,
                    loading: (diff.loading)(),
                    error: selected_state.branch_files_error,
                    onviewedchange: Some(EventHandler::new(move |count| {
                        let mut viewed_count = viewed_count;
                        viewed_count.set(count);
                    })),
                }
            } else if selected_state.panel == GitPanel::Commits {
                CommitDiffCard {
                    repository,
                    repo_root,
                    selected_commit,
                    diff_viewport: diff.viewport,
                    diff_rows: diff.rows,
                    loading: (diff.loading)(),
                }
            } else if selected_state.panel == GitPanel::Stash {
                StashDiffCard {
                    repository,
                    repo_root,
                    selected_stash,
                    diff_viewport: diff.viewport,
                    diff_rows: diff.rows,
                    loading: (diff.loading)(),
                }
            } else {
                DiffCard {
                    repo_root,
                    selected_path,
                    selected_path_bytes,
                    selected_abs_path,
                    diff_viewport: diff.viewport,
                    diff_rows: diff.rows,
                    loading: (diff.loading)(),
                }
            }
        }
    }
}

#[component]
fn GitShortcutOverlay() -> Element {
    let state = use_context::<GitPageState>();
    let visible = state.use_selector(|state| state.controller.shortcut_help_visible);
    rsx! {
        if visible() {
            GitShortcutHelp {}
        }
    }
}
