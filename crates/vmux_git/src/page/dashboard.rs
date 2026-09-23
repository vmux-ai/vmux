#![allow(non_snake_case)]

use std::collections::HashMap;

use dioxus::prelude::*;

use crate::event::{GitBranchLogEvent, GitDiffViewportEvent, GitRepositoryEvent};
use crate::view::EditorDiffMarker;

use super::branches::{BranchLogCard, BranchesCard};
use super::changes::ChangesCard;
use super::command_log::CommandLogCard;
use super::diff::{CommitDiffCard, DiffCard};
use super::history::{HistoryCard, StashCard};
use super::model::{BranchCollection, BranchPrompt, GitCommandLogEntry, GitPanel};
use super::shortcuts::{GitShortcutBar, GitShortcutHelp};
use super::status::{StatusCard, StatusDetailCard};

#[component]
pub(super) fn GitDashboard(
    repository: GitRepositoryEvent,
    selected_path: Signal<String>,
    selected_path_bytes: Signal<Vec<u8>>,
    selected_abs_path: Signal<String>,
    selected_commit: Signal<String>,
    selected_branch: Signal<String>,
    branch_collection: Signal<BranchCollection>,
    branch_prompt: Signal<Option<BranchPrompt>>,
    branch_draft: Signal<String>,
    selected_stash: Signal<String>,
    confirm_discard: Signal<Vec<u8>>,
    commit_message: Signal<String>,
    pending_commit_message: Signal<String>,
    workspace: Signal<String>,
    nonce: Signal<u32>,
    markers: Signal<HashMap<u32, EditorDiffMarker>>,
    diff_viewport: Signal<Option<GitDiffViewportEvent>>,
    focused_panel: Signal<GitPanel>,
    command_log: Signal<Vec<GitCommandLogEntry>>,
    branch_log: Signal<Option<GitBranchLogEvent>>,
    shortcut_help: Signal<bool>,
    fetching: Signal<bool>,
) -> Element {
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
