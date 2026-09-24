#![allow(non_snake_case)]

use dioxus::prelude::*;

use super::branches::{BranchLogCard, BranchesCard};
use super::changes::ChangesCard;
use super::command_log::CommandLogCard;
use super::diff::{CommitDiffCard, DiffCard};
use super::history::{HistoryCard, StashCard};
use super::model::GitPanel;
use super::shortcuts::{GitShortcutBar, GitShortcutHelp};
use super::state::GitPageState;
use super::status::{StatusCard, StatusDetailCard};

#[component]
pub(super) fn GitDashboard() -> Element {
    let GitPageState {
        workspace,
        repository,
        selected_path,
        selected_path_bytes,
        selected_abs_path,
        selected_commit,
        selected_branch,
        branch_collection,
        branch_prompt,
        branch_draft,
        selected_stash,
        confirm_discard,
        commit_message,
        pending_commit_message,
        fetching,
        focused_panel,
        command_log,
        branch_log,
        shortcut_help,
        nonce,
        markers,
        diff_viewport,
        ..
    } = use_context::<GitPageState>();
    let Some(repository) = repository() else {
        return rsx! {};
    };

    rsx! {
        main { class: "min-h-0 flex-1 overflow-y-auto bg-[radial-gradient(120%_90%_at_50%_-20%,color-mix(in_oklab,var(--primary)_5%,transparent),transparent_55%)] p-1.5 sm:overflow-hidden sm:p-2",
            div { class: "grid min-h-full grid-cols-1 gap-1.5 sm:h-full sm:grid-cols-[minmax(18rem,0.86fr)_minmax(0,2.14fr)] sm:grid-rows-[4rem_minmax(7rem,0.55fr)_minmax(9rem,1fr)_minmax(5rem,0.75fr)_minmax(5rem,0.75fr)_1.75rem] sm:gap-2 xl:grid-cols-[minmax(22rem,0.9fr)_minmax(0,2.1fr)]",
                StatusCard { repository: repository.clone(), focused_panel, fetching }
                ChangesCard {
                    repository: repository.clone(),
                    selected_path,
                    selected_path_bytes,
                    selected_abs_path,
                    confirm_discard,
                    commit_message,
                    pending_commit_message,
                    focused_panel,
                }
                BranchesCard {
                    repository: repository.clone(),
                    selected_branch,
                    branch_collection,
                    branch_prompt,
                    branch_draft,
                    focused_panel,
                }
                HistoryCard { repository: repository.clone(), selected_commit, focused_panel }
                StashCard { repository: repository.clone(), selected_stash, focused_panel }
                if focused_panel() == GitPanel::Status {
                    StatusDetailCard { repository: repository.clone(), fetching }
                } else if focused_panel() == GitPanel::Branches {
                    BranchLogCard {
                        branch: selected_branch(),
                        branch_log,
                    }
                } else if focused_panel() == GitPanel::Commits {
                    CommitDiffCard {
                        repository: repository.clone(),
                        repo_root: workspace,
                        selected_commit,
                        nonce,
                        markers,
                        diff_viewport,
                    }
                } else {
                    DiffCard {
                        repo_root: workspace,
                        selected_path,
                        selected_path_bytes,
                        selected_abs_path,
                        nonce,
                        markers,
                        diff_viewport,
                    }
                }
                CommandLogCard { command_log }
            }
        }
        GitShortcutBar {
            repo_root: repository.repo_root,
            focused_panel,
            branch_collection,
            shortcut_help,
        }
        if shortcut_help() {
            GitShortcutHelp { shortcut_help }
        }
    }
}
