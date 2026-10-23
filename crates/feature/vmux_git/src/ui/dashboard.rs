#![allow(non_snake_case)]

use dioxus::prelude::*;

use super::branches::BranchesCard;
use super::changes::ChangesCard;
use super::diff::{BranchDiffCard, CommitDiffCard, DiffCard, StashDiffCard};
use super::history::{HistoryCard, StashCard};
use super::shortcuts::GitShortcutHelp;
use super::state::GitPageState;
use super::status::RepositoryHeader;
use crate::state::GitPanel;

#[component]
pub(super) fn GitDashboard() -> Element {
    let GitPageState {
        snapshot,
        controller,
        header_dropdown,
        projects,
        diff,
        mut active_branch_file,
        ..
    } = use_context::<GitPageState>();
    let ui = snapshot.read();
    let controller_state = controller.read();
    let Some(repository) = ui.repository.clone() else {
        return rsx! {};
    };
    let repo_root = use_memo(move || snapshot.read().workspace.clone());
    let selected_path = use_memo(move || controller.read().selected_path.clone());
    let selected_path_bytes = use_memo(move || controller.read().selected_path_bytes.clone());
    let selected_abs_path = use_memo(move || controller.read().selected_abs_path.clone());
    let selected_commit = use_memo(move || controller.read().selected_commit.clone());
    let selected_stash = use_memo(move || controller.read().selected_stash.clone());
    let selected_branch = use_memo(move || controller.read().selected_branch.clone());
    let mut branch_reveal_path = use_signal(String::new);
    let mut viewed_count = use_signal(|| 0usize);
    use_effect(move || {
        selected_branch.read();
        let mut active_branch_file = active_branch_file;
        active_branch_file.set(String::new());
        branch_reveal_path.set(String::new());
        viewed_count.set(0);
    });
    let active_panel = controller_state.focused_panel;
    let mut list_width = use_signal(|| 336.0_f64);
    let mut resizing = use_signal(|| false);
    let mut resize_origin = use_signal(|| None::<(f64, f64)>);
    let split_style = format!("--git-list-width:{}px;", list_width());

    rsx! {
        RepositoryHeader {
            repository: repository.clone(),
            open_dropdown: header_dropdown(),
            projects: projects().rows,
            focused_panel: active_panel,
            selected_branch: controller_state.selected_branch.clone(),
            branch_collection: controller_state.branch_collection,
            selected_commit: controller_state.selected_commit.clone(),
            can_push: controller_state.operations.push,
            branches_progress: ui.local_branches,
            selected_branches_progress: ui.catalog(controller_state.branch_collection.into()),
            commits_progress: ui.commits,
            viewed_count: viewed_count(),
            viewed_total: ui.branch_files_total,
        }
        main { class: "flex min-h-0 flex-1 flex-col bg-transparent",
            div {
                id: "git-dashboard-split",
                class: "grid min-h-0 flex-1 grid-cols-1 grid-rows-[minmax(12rem,0.7fr)_minmax(18rem,1.3fr)] sm:grid-cols-[minmax(18rem,var(--git-list-width))_0_minmax(0,1fr)] sm:grid-rows-1",
                style: "{split_style}",
                aside { class: "relative z-20 flex min-h-0 flex-col overflow-visible bg-transparent",
                    div { class: "min-h-0 flex-1",
                        if active_panel == GitPanel::Files {
                            ChangesCard {
                                repository: repository.clone(),
                                selected_path_bytes: controller_state.selected_path_bytes.clone(),
                                confirm_discard: controller_state.confirm_discard.clone(),
                                commit_message: controller_state.commit_message.clone(),
                                commit_pending: controller_state.commit_pending.clone(),
                                operations: controller_state.operations.clone(),
                            }
                        } else if active_panel == GitPanel::Branches {
                            BranchesCard {
                                files: ui.branch_files.clone(),
                                files_total: ui.branch_files_total,
                                files_loading: ui.branch_files_loading,
                                files_has_more: ui.branch_files_has_more,
                                files_error: ui.branch_files_error.clone(),
                                selected_path_bytes: controller_state.selected_branch_path_bytes.clone(),
                                active_path: active_branch_file,
                                onfileselect: move |path: String| {
                                    active_branch_file.set(path.clone());
                                    branch_reveal_path.set(path);
                                },
                            }
                        } else if active_panel == GitPanel::Commits {
                            HistoryCard {
                                repository: repository.clone(),
                                selected_commit: controller_state.selected_commit.clone(),
                                operations: controller_state.operations.clone(),
                                progress: ui.commits,
                            }
                        } else {
                            StashCard {
                                repository: repository.clone(),
                                selected_stash: controller_state.selected_stash.clone(),
                                operations: controller_state.operations.clone(),
                                progress: ui.stashes,
                            }
                        }
                    }
                }
                div {
                    class: "group relative z-30 hidden h-full w-2 -translate-x-1/2 cursor-col-resize items-center justify-center sm:flex",
                    onmousedown: move |event: Event<MouseData>| {
                        event.prevent_default();
                        resize_origin.set(Some((event.client_coordinates().x, list_width())));
                        resizing.set(true);
                    },
                    div { class: "pointer-events-none h-10 w-1 rounded-full bg-foreground/20 opacity-0 transition-opacity duration-150 group-hover:opacity-100" }
                }
                section { class: "min-h-0 min-w-0 overflow-hidden bg-transparent py-2.5 pr-2.5",
                    if active_panel == GitPanel::Branches {
                        BranchDiffCard {
                            repository: repository.clone(),
                            files: ui.branch_files.clone(),
                            active_path: active_branch_file,
                            repo_root,
                            selected_branch,
                            reveal_path: branch_reveal_path,
                            diff_viewport: diff.viewport,
                            diff_rows: diff.rows,
                            loading: (diff.loading)(),
                            error: ui.branch_files_error.clone(),
                            onviewedchange: Some(EventHandler::new(move |count| {
                                viewed_count.set(count);
                            })),
                        }
                    } else if active_panel == GitPanel::Commits {
                        CommitDiffCard {
                            repository: repository.clone(),
                            repo_root,
                            selected_commit,
                            diff_viewport: diff.viewport,
                            diff_rows: diff.rows,
                            loading: (diff.loading)(),
                        }
                    } else if active_panel == GitPanel::Stash {
                        StashDiffCard {
                            repository: repository.clone(),
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
        if resizing() {
            div {
                class: "fixed inset-0 z-[900] cursor-col-resize",
                onmousemove: move |event: Event<MouseData>| {
                    let Some((start, width)) = resize_origin() else {
                        return;
                    };
                    let width = (width + event.client_coordinates().x - start).clamp(288.0, 640.0);
                    let _ = document::eval(&format!(
                        "document.getElementById('git-dashboard-split')?.style.setProperty('--git-list-width','{width}px')"
                    ));
                },
                onmouseup: move |event: Event<MouseData>| {
                    if let Some((start, width)) = resize_origin() {
                        list_width.set(
                            (width + event.client_coordinates().x - start).clamp(288.0, 640.0),
                        );
                    }
                    resize_origin.set(None);
                    resizing.set(false);
                },
            }
        }
        if controller_state.shortcut_help_visible {
            GitShortcutHelp {}
        }
    }
}
