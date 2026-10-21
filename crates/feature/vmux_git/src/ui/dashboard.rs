#![allow(non_snake_case)]

use dioxus::prelude::*;

use super::branches::BranchesCard;
use super::changes::ChangesCard;
use super::command_log::CommandLogCard;
use super::diff::{BranchDiffCard, CommitDiffCard, DiffCard};
use super::history::{HistoryCard, StashCard};
use super::shortcuts::GitShortcutHelp;
use super::state::GitPageState;
use super::status::{RepositoryHeader, RepositoryPanelHeader};
use crate::state::GitPanel;

#[component]
pub(super) fn GitDashboard() -> Element {
    let GitPageState {
        snapshot,
        controller,
        projects,
        ..
    } = use_context::<GitPageState>();
    let ui = snapshot();
    let controller_state = controller();
    let Some(repository) = ui.repository.clone() else {
        return rsx! {};
    };
    let repo_root = use_memo(move || snapshot().workspace);
    let diff_viewport = use_memo(move || snapshot().diff_viewport);
    let diff_rows = use_memo(move || snapshot().diff_rows);
    let selected_path = use_memo(move || controller().selected_path);
    let selected_path_bytes = use_memo(move || controller().selected_path_bytes);
    let selected_abs_path = use_memo(move || controller().selected_abs_path);
    let selected_commit = use_memo(move || controller().selected_commit);
    let selected_branch = use_memo(move || controller().selected_branch);
    let selected_branch_path = use_memo(move || controller().selected_branch_path);
    let active_panel = controller_state.focused_panel;
    let mut list_width = use_signal(|| 336.0_f64);
    let mut resizing = use_signal(|| false);
    let split_style = format!("--git-list-width:{}px;", list_width());

    rsx! {
        RepositoryHeader {
            repository: repository.clone(),
            projects: projects().rows,
            selected_commit: controller_state.selected_commit.clone(),
            can_push: controller_state.operations.push,
            branches_progress: ui.local_branches,
            commits_progress: ui.commits,
        }
        main { class: "flex min-h-0 flex-1 flex-col bg-background",
            div {
                class: "grid min-h-0 flex-1 grid-cols-1 grid-rows-[minmax(12rem,0.7fr)_minmax(18rem,1.3fr)] overflow-hidden sm:grid-cols-[minmax(18rem,var(--git-list-width))_0.5rem_minmax(0,1fr)] sm:grid-rows-1",
                style: "{split_style}",
                aside { class: "flex min-h-0 flex-col overflow-hidden bg-foreground/[0.018]",
                    RepositoryPanelHeader {
                        repository: repository.clone(),
                        focused_panel: active_panel,
                        fetching: ui.fetching,
                        can_stage_all: controller_state.operations.stage_all,
                    }
                    div { class: "min-h-0 flex-1 overflow-hidden",
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
                                repository: repository.clone(),
                                selected_branch: controller_state.selected_branch.clone(),
                                branch_collection: controller_state.branch_collection,
                                files: ui.branch_files.clone(),
                                files_total: ui.branch_files_total,
                                files_loading: ui.branch_files_loading,
                                files_has_more: ui.branch_files_has_more,
                                files_error: ui.branch_files_error.clone(),
                                selected_path_bytes: controller_state.selected_branch_path_bytes.clone(),
                                operations: controller_state.operations.clone(),
                                catalog_progress: ui.catalog(controller_state.branch_collection.into()),
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
                    class: "group relative z-20 hidden h-full cursor-col-resize items-center justify-center sm:flex",
                    onmousedown: move |event: Event<MouseData>| {
                        event.prevent_default();
                        resizing.set(true);
                    },
                    div { class: "pointer-events-none h-10 w-1 rounded-full bg-foreground/20 opacity-0 transition-opacity duration-150 group-hover:opacity-100" }
                }
                section { class: "min-h-0 min-w-0 overflow-hidden bg-background p-2.5",
                    if active_panel == GitPanel::Branches {
                        BranchDiffCard {
                            repository: repository.clone(),
                            repo_root,
                            selected_branch,
                            selected_path: selected_branch_path,
                            diff_viewport,
                            diff_rows,
                            loading: ui.diff_loading,
                        }
                    } else if active_panel == GitPanel::Commits {
                        CommitDiffCard {
                            repository: repository.clone(),
                            repo_root,
                            selected_commit,
                            diff_viewport,
                            diff_rows,
                            loading: ui.diff_loading,
                        }
                    } else {
                        DiffCard {
                            repo_root,
                            selected_path,
                            selected_path_bytes,
                            selected_abs_path,
                            diff_viewport,
                            diff_rows,
                            loading: ui.diff_loading,
                        }
                    }
                }
            }
            if !ui.command_log.is_empty() {
                CommandLogCard { command_log: ui.command_log }
            }
        }
        if resizing() {
            div {
                class: "fixed inset-0 z-[900] cursor-col-resize",
                onmousemove: move |event: Event<MouseData>| {
                    list_width.set(event.client_coordinates().x.clamp(288.0, 640.0));
                },
                onmouseup: move |_| resizing.set(false),
            }
        }
        if controller_state.shortcut_help_visible {
            GitShortcutHelp {}
        }
    }
}
