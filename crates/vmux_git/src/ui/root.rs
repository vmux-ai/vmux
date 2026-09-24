#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_core::event::PageContextRequest;
use vmux_ui::hooks::{send, use_theme};
use vmux_ui::i18n::translate;

use super::branches::BranchPromptDialog;
use super::dashboard::GitDashboard;
use super::empty::EmptyRepository;
use super::model::{BranchCollection, BranchPrompt, GitPanel};
use super::state::GitPageState;
use super::workspace::GitWorkspace;
use crate::event::*;

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = GitPageState::use_state();
    use_context_provider(|| state);
    let GitPageState {
        snapshot,
        selected_path_bytes,
        selected_commit,
        selected_branch,
        branch_collection,
        mut branch_prompt,
        mut branch_draft,
        selected_stash,
        mut confirm_discard,
        mut focused_panel,
        mut shortcut_help,
        ..
    } = state;

    use_effect(move || {
        let _ = send(&PageContextRequest {});
    });
    use_effect(move || {
        let ui = snapshot();
        if focused_panel() != GitPanel::Branches {
            return;
        }
        let repo_root = ui.workspace;
        let branch = selected_branch();
        if repo_root.is_empty() || branch.is_empty() {
            return;
        }
        if ui
            .branch_log
            .is_some_and(|event| event.repo_root == repo_root && event.branch == branch)
        {
            return;
        }
        let _ = send(&GitBranchLogRequest { repo_root, branch });
    });

    rsx! {
        document::Title { {translate("git-title")} }
        div {
            class: "relative flex h-screen min-w-0 flex-col bg-background text-foreground outline-none",
            tabindex: "-1",
            autofocus: true,
            onkeydown: move |event: KeyboardEvent| {
                if event.key() == Key::Escape && shortcut_help() {
                    event.prevent_default();
                    event.stop_propagation();
                    shortcut_help.set(false);
                    return;
                }
                if let Some(direction) = GitPanel::menu_direction(&event) {
                    if state.move_selection(direction) {
                        event.prevent_default();
                        event.stop_propagation();
                    }
                    return;
                }
                if event.is_auto_repeating() {
                    return;
                }
                let modifiers = event.modifiers();
                if modifiers.ctrl() || modifiers.alt() || modifiers.meta() {
                    return;
                }
                let key = event.key().to_string();
                if key == "?" {
                    event.prevent_default();
                    event.stop_propagation();
                    shortcut_help.set(!shortcut_help());
                    return;
                }
                if key == "Tab" {
                    event.prevent_default();
                    event.stop_propagation();
                    focused_panel.set(focused_panel().next(modifiers.shift()));
                    return;
                }
                if let Some(panel) = GitPanel::from_key(&key) {
                    event.prevent_default();
                    event.stop_propagation();
                    focused_panel.set(panel);
                    return;
                }
                let ui = snapshot();
                let Some(repository) = ui.repository else {
                    return;
                };
                let handled = match (focused_panel(), key.as_str()) {
                    (GitPanel::Status, "e") => {
                        GitWorkspace::edit_config(&repository.repo_root);
                        true
                    }
                    (GitPanel::Status, "u") => {
                        GitWorkspace::check_for_updates();
                        true
                    }
                    (GitPanel::Status, "Enter") => {
                        let _ = send(&GitRepositoryPickerRequest {
                            path: repository.repo_root.clone(),
                        });
                        true
                    }
                    (GitPanel::Files, "a") => {
                        let _ = send(&GitStageAllRequest { path: repository.repo_root.clone() });
                        true
                    }
                    (GitPanel::Files, "s") => {
                        if repository.files.is_empty() {
                            false
                        } else {
                            GitOperation::StashPush.send(repository.repo_root.clone());
                            true
                        }
                    }
                    (GitPanel::Files, "A") => {
                        let can_amend = !repository.commits.is_empty()
                            && repository.files.iter().any(|entry| entry.staged);
                        if can_amend {
                            GitOperation::Amend.send(repository.repo_root.clone());
                        }
                        can_amend
                    }
                    (GitPanel::Files, " ") | (GitPanel::Files, "Space") => {
                        let Some(entry) = repository
                            .files
                            .iter()
                            .find(|entry| entry.path_bytes == selected_path_bytes())
                        else {
                            return;
                        };
                        let path = GitWorkspace::absolute_path(&repository.repo_root, &entry.path);
                        if entry.unstaged {
                            let _ = send(&GitStageRequest {
                                repo_root: repository.repo_root.clone(),
                                path,
                                path_bytes: entry.path_bytes.clone(),
                            });
                        } else if entry.staged {
                            let _ = send(&GitUnstageRequest {
                                repo_root: repository.repo_root.clone(),
                                path,
                                path_bytes: entry.path_bytes.clone(),
                            });
                        }
                        true
                    }
                    (GitPanel::Files, "x") => {
                        let Some(entry) = repository
                            .files
                            .iter()
                            .find(|entry| entry.path_bytes == selected_path_bytes())
                            .filter(|entry| entry.can_discard())
                        else {
                            return;
                        };
                        if confirm_discard() == entry.path_bytes {
                            let _ = send(&GitDiscardRequest {
                                repo_root: repository.repo_root.clone(),
                                path: GitWorkspace::absolute_path(&repository.repo_root, &entry.path),
                                path_bytes: entry.path_bytes.clone(),
                            });
                            confirm_discard.set(Vec::new());
                        } else {
                            confirm_discard.set(entry.path_bytes.clone());
                        }
                        true
                    }
                    (GitPanel::Branches, "Enter")
                    | (GitPanel::Branches, " ")
                    | (GitPanel::Branches, "Space")
                    | (GitPanel::Branches, "c")
                        if branch_collection() == BranchCollection::Local => {
                        let Some(branch) = repository
                            .branches
                            .iter()
                            .find(|branch| branch.name == selected_branch())
                        else {
                            return;
                        };
                        GitWorkspace::select_branch(&repository.repo_root, branch);
                        true
                    }
                    (GitPanel::Branches, "r")
                    | (GitPanel::Branches, "M")
                    | (GitPanel::Branches, "f")
                        if branch_collection() == BranchCollection::Local => {
                        let Some(branch) = repository
                            .branches
                            .iter()
                            .find(|branch| branch.name == selected_branch() && !branch.current)
                        else {
                            return;
                        };
                        let operation = match key.as_str() {
                            "r" => GitOperation::Rebase {
                                branch: branch.name.clone(),
                            },
                            "M" => GitOperation::Merge {
                                branch: branch.name.clone(),
                            },
                            _ => GitOperation::FastForward {
                                branch: branch.name.clone(),
                            },
                        };
                        operation.send(repository.repo_root.clone());
                        true
                    }
                    (GitPanel::Branches, "n")
                        if branch_collection() == BranchCollection::Local => {
                        let base = selected_branch();
                        if base.is_empty() {
                            false
                        } else {
                            branch_draft.set(String::new());
                            branch_prompt.set(Some(BranchPrompt::Create { base }));
                            true
                        }
                    }
                    (GitPanel::Branches, "d")
                        if branch_collection() == BranchCollection::Local => {
                        let branch = repository
                            .branches
                            .iter()
                            .find(|branch| branch.name == selected_branch())
                            .filter(|branch| !branch.current && branch.checkout.is_empty());
                        if let Some(branch) = branch {
                            branch_prompt.set(Some(BranchPrompt::Delete {
                                branch: branch.name.clone(),
                            }));
                            true
                        } else {
                            false
                        }
                    }
                    (GitPanel::Commits, " ")
                    | (GitPanel::Commits, "Space")
                    | (GitPanel::Commits, "C")
                    | (GitPanel::Commits, "V")
                    | (GitPanel::Commits, "t") => {
                        let Some(commit) = repository
                            .commits
                            .iter()
                            .find(|commit| commit.sha == selected_commit())
                        else {
                            return;
                        };
                        let operation = match key.as_str() {
                            " " | "Space" => GitOperation::CheckoutCommit {
                                commit: commit.sha.clone(),
                            },
                            "t" => GitOperation::Revert {
                                commit: commit.sha.clone(),
                            },
                            _ => GitOperation::CherryPick {
                                commit: commit.sha.clone(),
                            },
                        };
                        operation.send(repository.repo_root.clone());
                        true
                    }
                    (GitPanel::Stash, "g") | (GitPanel::Stash, "d") => {
                        let Some(stash) = repository
                            .stashes
                            .iter()
                            .find(|stash| stash.reference == selected_stash())
                        else {
                            return;
                        };
                        let operation = if key == "g" {
                            GitOperation::StashPop {
                                reference: stash.reference.clone(),
                            }
                        } else {
                            GitOperation::StashDrop {
                                reference: stash.reference.clone(),
                            }
                        };
                        operation.send(repository.repo_root.clone());
                        true
                    }
                    _ => false,
                };
                if handled {
                    event.prevent_default();
                    event.stop_propagation();
                }
            },
            if snapshot().repository.is_some() {
                GitDashboard {}
            } else {
                EmptyRepository {}
            }
            if branch_prompt().is_some() {
                BranchPromptDialog {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GitPanel;

    #[test]
    fn number_keys_select_panels() {
        assert_eq!(GitPanel::from_key("0"), Some(GitPanel::Status));
        assert_eq!(GitPanel::from_key("1"), Some(GitPanel::Status));
        assert_eq!(GitPanel::from_key("2"), Some(GitPanel::Files));
        assert_eq!(GitPanel::from_key("3"), Some(GitPanel::Branches));
        assert_eq!(GitPanel::from_key("4"), Some(GitPanel::Commits));
        assert_eq!(GitPanel::from_key("5"), Some(GitPanel::Stash));
        assert_eq!(GitPanel::from_key("6"), None);
    }

    #[test]
    fn tab_cycles_panels_in_both_directions() {
        assert_eq!(GitPanel::Status.next(false), GitPanel::Files);
        assert_eq!(GitPanel::Commits.next(false), GitPanel::Stash);
        assert_eq!(GitPanel::Stash.next(false), GitPanel::Status);
        assert_eq!(GitPanel::Status.next(true), GitPanel::Stash);
        assert_eq!(GitPanel::Stash.next(true), GitPanel::Commits);
        assert_eq!(GitPanel::Files.next(true), GitPanel::Status);
    }
}
