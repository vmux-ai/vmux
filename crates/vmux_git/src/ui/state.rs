use dioxus::prelude::*;
use vmux_ui::hooks::use_ui_state_root;
use vmux_ui::scroll::ScrollIntoView;

use crate::event::GitOperation;
use crate::state::{
    GitBranchPrompt, GitDirectoryState, GitPageControllerState, GitPageSnapshot, GitUiState,
};

#[derive(Clone, Copy)]
pub(super) struct GitPageState {
    pub(super) snapshot: Signal<GitPageSnapshot>,
    pub(super) controller: Signal<GitPageControllerState>,
    pub(super) directory: Signal<GitDirectoryState>,
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
            directory: use_signal(GitDirectoryState::default),
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
                if let Some(snapshot) = &patch.snapshot {
                    self.apply_snapshot(*snapshot.clone());
                }
                if let Some(directory) = &patch.directory {
                    let mut current = self.directory;
                    current.set(*directory.clone());
                }
                if let Some(controller) = &patch.controller {
                    let mut current = self.controller;
                    current.set(*controller.clone());
                }
                if let Some(request) = &patch.branch_prompt {
                    if matches!(&request.prompt, GitBranchPrompt::Create { .. }) {
                        let mut draft = self.branch_draft;
                        draft.set(String::new());
                    }
                    let mut prompt = self.branch_prompt;
                    prompt.set(Some(request.prompt.clone()));
                }
                if let Some(request) = &patch.selection_reveal {
                    ScrollIntoView::nearest(&request.id);
                }
                if patch.shortcut_help_toggle.is_some() {
                    let mut help = self.shortcut_help;
                    help.toggle();
                }
                if patch.context.is_some() {
                    self.reset_local_render_state();
                }
                if let Some(workspace) = &patch.workspace
                    && workspace.error.is_empty()
                    && !workspace.path.is_empty()
                    && workspace.path != self.workspace()
                {
                    self.reset_local_render_state();
                }
            }
        });
    }

    fn apply_snapshot(self, snapshot: GitPageSnapshot) {
        self.apply_result(&snapshot);
        let mut current = self.snapshot;
        current.set(snapshot);
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
        if result.operation != "commit" {
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
        let mut directory = self.directory;
        let mut branch_prompt = self.branch_prompt;
        let mut branch_draft = self.branch_draft;
        directory.set(GitDirectoryState::default());
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
