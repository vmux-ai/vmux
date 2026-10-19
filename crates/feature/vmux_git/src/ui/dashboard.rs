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
    let Some(repository) = ui.repository else {
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
    let active_panel = controller_state.focused_panel;

    rsx! {
        RepositoryHeader {
            repository: repository.clone(),
            projects: projects().rows,
            selected_commit: controller_state.selected_commit.clone(),
            can_push: controller_state.operations.push,
        }
        main { class: "flex min-h-0 flex-1 flex-col bg-background",
            div { class: "grid min-h-0 flex-1 grid-cols-1 grid-rows-[minmax(12rem,0.7fr)_minmax(18rem,1.3fr)] overflow-hidden sm:grid-cols-[minmax(18rem,24rem)_minmax(0,1fr)] sm:grid-rows-1",
                aside { class: "flex min-h-0 flex-col overflow-hidden bg-card/55",
                    RepositoryPanelHeader {
                        repository: repository.clone(),
                        focused_panel: active_panel,
                        fetching: ui.fetching,
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
                                operations: controller_state.operations.clone(),
                            }
                        } else if active_panel == GitPanel::Commits {
                            HistoryCard {
                                repository: repository.clone(),
                                selected_commit: controller_state.selected_commit.clone(),
                                operations: controller_state.operations.clone(),
                            }
                        } else {
                            StashCard {
                                repository: repository.clone(),
                                selected_stash: controller_state.selected_stash.clone(),
                                operations: controller_state.operations.clone(),
                            }
                        }
                    }
                }
                section { class: "min-h-0 min-w-0 overflow-hidden bg-background",
                    if active_panel == GitPanel::Branches {
                        BranchDiffCard {
                            repository: repository.clone(),
                            repo_root,
                            selected_branch,
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
        if controller_state.shortcut_help_visible {
            GitShortcutHelp {}
        }
    }
}
