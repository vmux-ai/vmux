use dioxus::prelude::*;
use vmux_ui::hooks::use_ui_state_root;
use vmux_ui::scroll::ScrollIntoView;

use super::workspace::GitWorkspace;
use crate::event::GitOperation;
use crate::state::{
    GitBranchPrompt, GitPageControllerState, GitPageSnapshot, GitUiState, GitUiStatePatch,
};

#[derive(Clone, Copy)]
pub(super) struct GitPageState {
    pub(super) snapshot: Signal<GitPageSnapshot>,
    pub(super) controller: Signal<GitPageControllerState>,
    pub(super) directory_selected: Signal<usize>,
    pub(super) directory_preview_path: Signal<String>,
    pub(super) directory_came_from: Signal<String>,
    pub(super) directory_show_hidden: Signal<bool>,
    pub(super) branch_prompt: Signal<Option<GitBranchPrompt>>,
    pub(super) branch_draft: Signal<String>,
    pub(super) commit_message: Signal<String>,
    pub(super) pending_commit_message: Signal<String>,
    pub(super) shortcut_help: Signal<bool>,
    handled_result_sequence: Signal<u64>,
}

impl GitPageState {
    pub(super) fn use_state() -> Self {
        let state = Self {
            snapshot: use_signal(GitPageSnapshot::default),
            controller: use_signal(GitPageControllerState::default),
            directory_selected: use_signal(|| 0),
            directory_preview_path: use_signal(String::new),
            directory_came_from: use_signal(String::new),
            directory_show_hidden: use_signal(|| true),
            branch_prompt: use_signal(|| None),
            branch_draft: use_signal(String::new),
            commit_message: use_signal(String::new),
            pending_commit_message: use_signal(String::new),
            shortcut_help: use_signal(|| false),
            handled_result_sequence: use_signal(|| 0),
        };
        state.subscribe();
        state
    }

    fn subscribe(self) {
        let root = use_ui_state_root::<GitUiState>();
        let mut handled_ui_sequence = use_signal(|| 0);
        use_effect(move || {
            let event = root.state.read();
            if event.sequence == 0 || event.sequence == *handled_ui_sequence.peek() {
                return;
            }
            handled_ui_sequence.set(event.sequence);
            for patch in &event.patches {
                match patch {
                    GitUiStatePatch::Snapshot(snapshot) => self.apply_snapshot(*snapshot.clone()),
                    GitUiStatePatch::Controller(controller) => {
                        let mut current = self.controller;
                        current.set(*controller.clone());
                    }
                    GitUiStatePatch::BranchPrompt(request) => {
                        if matches!(&request.prompt, GitBranchPrompt::Create { .. }) {
                            let mut draft = self.branch_draft;
                            draft.set(String::new());
                        }
                        let mut prompt = self.branch_prompt;
                        prompt.set(Some(request.prompt.clone()));
                    }
                    GitUiStatePatch::SelectionReveal(request) => {
                        ScrollIntoView::nearest(&request.id);
                    }
                    GitUiStatePatch::ShortcutHelpToggle(_) => {
                        let mut help = self.shortcut_help;
                        help.toggle();
                    }
                    GitUiStatePatch::Context(_) => self.reset_local_render_state(),
                    GitUiStatePatch::Workspace(workspace) => {
                        if workspace.error.is_empty()
                            && !workspace.path.is_empty()
                            && workspace.path != self.workspace()
                        {
                            self.reset_local_render_state();
                        }
                    }
                    GitUiStatePatch::RepositoryPicked(_) => {}
                }
            }
        });
    }

    fn apply_snapshot(self, snapshot: GitPageSnapshot) {
        if let Some(directory) = snapshot.directory.as_ref() {
            self.reconcile_directory(directory);
        }
        self.apply_result(&snapshot);
        let mut current = self.snapshot;
        current.set(snapshot);
    }

    fn reconcile_directory(self, directory: &crate::event::GitDirectorySnapshot) {
        let current = (self.snapshot)();
        if current.directory.as_ref().map(|current| &current.path) == Some(&directory.path)
            || !directory.repo_root.is_empty()
        {
            return;
        }
        let came_from = (self.directory_came_from)();
        let selected = directory
            .entries
            .iter()
            .position(|entry| entry.path == came_from)
            .unwrap_or(0);
        let mut directory_came_from = self.directory_came_from;
        let mut directory_selected = self.directory_selected;
        let mut preview_path = self.directory_preview_path;
        directory_came_from.set(String::new());
        directory_selected.set(selected);
        preview_path.set(String::new());
        if let Some(entry) = directory.entries.get(selected).filter(|entry| entry.is_dir) {
            preview_path.set(entry.path.clone());
            GitWorkspace::browse(&entry.path, true);
        }
    }

    fn apply_result(self, snapshot: &GitPageSnapshot) {
        if snapshot.result_sequence == 0
            || snapshot.result_sequence == (self.handled_result_sequence)()
        {
            return;
        }
        let mut handled = self.handled_result_sequence;
        handled.set(snapshot.result_sequence);
        let Some(result) = snapshot.result.as_ref() else {
            return;
        };
        if result.action != "commit" {
            return;
        }
        let mut pending = self.pending_commit_message;
        if result.ok && (self.commit_message)().trim() == pending() {
            let mut message = self.commit_message;
            message.set(String::new());
        }
        pending.set(String::new());
    }

    fn reset_local_render_state(self) {
        let mut directory_selected = self.directory_selected;
        let mut directory_preview_path = self.directory_preview_path;
        let mut directory_came_from = self.directory_came_from;
        let mut directory_show_hidden = self.directory_show_hidden;
        let mut branch_prompt = self.branch_prompt;
        let mut branch_draft = self.branch_draft;
        directory_selected.set(0);
        directory_preview_path.set(String::new());
        directory_came_from.set(String::new());
        directory_show_hidden.set(true);
        branch_prompt.set(None);
        branch_draft.set(String::new());
    }

    pub(super) fn workspace(self) -> String {
        (self.snapshot)().workspace
    }

    pub(super) fn submit_branch_prompt(self, prompt: &GitBranchPrompt) -> bool {
        let workspace = self.workspace();
        match prompt {
            GitBranchPrompt::Create { base } => {
                let branch = (self.branch_draft)().trim().to_string();
                if branch.is_empty() {
                    return false;
                }
                GitOperation::CreateBranch {
                    branch,
                    start_point: base.clone(),
                }
                .send(workspace);
            }
            GitBranchPrompt::Delete { branch } => {
                GitOperation::DeleteBranch {
                    branch: branch.clone(),
                }
                .send(workspace);
            }
        }
        true
    }
}
