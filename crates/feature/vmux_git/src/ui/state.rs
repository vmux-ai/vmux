use dioxus::prelude::*;
use vmux_ui::hooks::use_ui_state_binding;
use vmux_ui::scroll::ScrollIntoView;

use crate::event::GitOperation;
use crate::state::{
    GitBranchPrompt, GitBranchPromptRequested, GitDirectoryState, GitPageContext,
    GitPageControllerState, GitPageSnapshot, GitSelectionReveal, GitUiState, GitWorkspaceChanged,
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
    handled_result_sequence: Signal<u64>,
    handled_selection_reveal: Signal<u64>,
}

impl GitPageState {
    pub(super) fn use_state() -> Self {
        let ui = use_ui_state_binding::<GitUiState>();
        let state = Self {
            snapshot: ui.use_value::<GitPageSnapshot>().value,
            controller: ui.use_value::<GitPageControllerState>().value,
            directory: ui.use_value::<GitDirectoryState>().value,
            branch_prompt: use_signal(|| None),
            branch_draft: use_signal(String::new),
            commit_message: use_signal(String::new),
            pending_commit_message: use_signal(String::new),
            handled_result_sequence: use_signal(|| 0),
            handled_selection_reveal: use_signal(|| 0),
        };
        state.subscribe(ui);
        state
    }

    fn subscribe(self, ui: vmux_ui::hooks::UiStateBinding<GitUiState>) {
        ui.use_updates::<GitPageSnapshot>(move |snapshot| self.apply_result(&snapshot));
        ui.use_updates::<GitBranchPromptRequested>(move |request| {
            if matches!(&request.prompt, GitBranchPrompt::Create { .. }) {
                let mut draft = self.branch_draft;
                draft.set(String::new());
            }
            let mut prompt = self.branch_prompt;
            prompt.set(Some(request.prompt));
        });
        ui.use_updates::<GitSelectionReveal>(move |request| {
            if request.revision <= (self.handled_selection_reveal)() {
                return;
            }
            let mut handled = self.handled_selection_reveal;
            handled.set(request.revision);
            ScrollIntoView::nearest(&request.id);
        });
        ui.use_updates::<GitPageContext>(move |_| self.reset_local_render_state());
        ui.use_updates::<GitWorkspaceChanged>(move |workspace| {
            if workspace.error.is_empty()
                && !workspace.path.is_empty()
                && workspace.path != self.workspace()
            {
                self.reset_local_render_state();
            }
        });
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
