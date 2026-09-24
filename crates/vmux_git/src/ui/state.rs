use dioxus::prelude::*;
use vmux_ui::hooks::use_ui_state_root;
use vmux_ui::list_nav::{MenuDirection, move_selection};
use vmux_ui::scroll::ScrollIntoView;

use super::model::{BranchCollection, BranchPrompt, GitPanel};
use super::workspace::GitWorkspace;
use crate::event::*;
use crate::state::{
    GitCommandLogEntry, GitPageContext, GitPageSnapshot, GitRepositoryPicked, GitUiState,
    GitUiStatePatch, GitWorkspaceChanged,
};

#[derive(Clone, Copy)]
pub(super) struct GitPageState {
    pub(super) snapshot: Signal<GitPageSnapshot>,
    pub(super) directory_selected: Signal<usize>,
    pub(super) directory_preview_path: Signal<String>,
    pub(super) directory_came_from: Signal<String>,
    pub(super) directory_show_hidden: Signal<bool>,
    pub(super) selected_path: Signal<String>,
    pub(super) selected_path_bytes: Signal<Vec<u8>>,
    pub(super) selected_abs_path: Signal<String>,
    pub(super) selected_commit: Signal<String>,
    pub(super) selected_branch: Signal<String>,
    pub(super) branch_collection: Signal<BranchCollection>,
    pub(super) branch_prompt: Signal<Option<BranchPrompt>>,
    pub(super) branch_draft: Signal<String>,
    pub(super) pending_branch_checkout: Signal<String>,
    pub(super) selected_stash: Signal<String>,
    pub(super) confirm_discard: Signal<Vec<u8>>,
    pub(super) commit_message: Signal<String>,
    pub(super) pending_commit_message: Signal<String>,
    pub(super) focused_panel: Signal<GitPanel>,
    pub(super) shortcut_help: Signal<bool>,
    handled_result_sequence: Signal<u64>,
}

impl GitPageState {
    pub(super) fn use_state() -> Self {
        let state = Self {
            snapshot: use_signal(GitPageSnapshot::default),
            directory_selected: use_signal(|| 0),
            directory_preview_path: use_signal(String::new),
            directory_came_from: use_signal(String::new),
            directory_show_hidden: use_signal(|| true),
            selected_path: use_signal(String::new),
            selected_path_bytes: use_signal(Vec::new),
            selected_abs_path: use_signal(String::new),
            selected_commit: use_signal(String::new),
            selected_branch: use_signal(String::new),
            branch_collection: use_signal(BranchCollection::default),
            branch_prompt: use_signal(|| None),
            branch_draft: use_signal(String::new),
            pending_branch_checkout: use_signal(String::new),
            selected_stash: use_signal(String::new),
            confirm_discard: use_signal(Vec::new),
            commit_message: use_signal(String::new),
            pending_commit_message: use_signal(String::new),
            focused_panel: use_signal(GitPanel::default),
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
                    GitUiStatePatch::Context(context) => self.apply_context(context.clone()),
                    GitUiStatePatch::RepositoryPicked(picked) => {
                        self.apply_repository_picked(picked.clone());
                    }
                    GitUiStatePatch::Workspace(workspace) => {
                        self.apply_workspace_changed(workspace.clone());
                    }
                    GitUiStatePatch::Snapshot(snapshot) => self.apply_snapshot(*snapshot.clone()),
                }
            }
        });
    }

    fn apply_context(self, context: GitPageContext) {
        let path = crate::GitUrl::parse(&context.page_url)
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or(context.working_directory);
        self.reset(path.clone(), String::new());
        GitWorkspace::browse(&path, false);
    }

    fn apply_repository_picked(self, event: GitRepositoryPicked) {
        if event.path.is_empty() {
            return;
        }
        self.start_loading();
        GitWorkspace::browse(&event.path, false);
    }

    fn apply_workspace_changed(self, event: GitWorkspaceChanged) {
        if !event.error.is_empty() {
            self.record_error(event.error);
            return;
        }
        if event.path.is_empty() || event.path == self.workspace() {
            return;
        }
        self.reset(event.path.clone(), event.branch);
        GitWorkspace::request(&event.path);
    }

    fn apply_snapshot(self, snapshot: GitPageSnapshot) {
        if let Some(repository) = snapshot.repository.as_ref() {
            self.reconcile_repository(repository);
        }
        if let Some(directory) = snapshot.directory.as_ref() {
            self.reconcile_directory(directory);
        }
        self.apply_result(&snapshot);
        let mut current = self.snapshot;
        current.set(snapshot);
    }

    fn reconcile_repository(self, repository: &GitRepositorySnapshot) {
        let next_file = repository
            .files
            .iter()
            .find(|entry| entry.path_bytes == (self.selected_path_bytes)())
            .or_else(|| repository.files.first())
            .cloned();
        let next_commit = repository
            .commits
            .iter()
            .find(|entry| entry.sha == (self.selected_commit)())
            .or_else(|| repository.commits.first())
            .map(|entry| entry.sha.clone())
            .unwrap_or_default();
        let next_branch =
            (self.branch_collection)().selected_reference(repository, &(self.selected_branch)());
        let next_stash = repository
            .stashes
            .iter()
            .find(|entry| entry.reference == (self.selected_stash)())
            .or_else(|| repository.stashes.first())
            .map(|entry| entry.reference.clone())
            .unwrap_or_default();
        let mut selected_abs_path = self.selected_abs_path;
        let mut selected_path = self.selected_path;
        let mut selected_path_bytes = self.selected_path_bytes;
        let mut selected_commit = self.selected_commit;
        let mut selected_branch = self.selected_branch;
        let mut selected_stash = self.selected_stash;
        selected_abs_path.set(
            next_file
                .as_ref()
                .map(|entry| GitWorkspace::absolute_path(&repository.repo_root, &entry.path))
                .unwrap_or_default(),
        );
        selected_path.set(
            next_file
                .as_ref()
                .map(|entry| entry.path.clone())
                .unwrap_or_default(),
        );
        selected_path_bytes.set(next_file.map(|entry| entry.path_bytes).unwrap_or_default());
        selected_commit.set(next_commit);
        selected_branch.set(next_branch);
        selected_stash.set(next_stash);
    }

    fn reconcile_directory(self, directory: &GitDirectorySnapshot) {
        let current = (self.snapshot)();
        if current.directory.as_ref().map(|current| &current.path) == Some(&directory.path) {
            return;
        }
        if !directory.repo_root.is_empty() {
            GitWorkspace::activate(&directory.repo_root);
            GitWorkspace::request(&directory.repo_root);
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
        if result.action == "commit" {
            let mut pending = self.pending_commit_message;
            if result.ok && (self.commit_message)().trim() == pending() {
                let mut message = self.commit_message;
                message.set(String::new());
            }
            pending.set(String::new());
        }
        if result.action == "new branch" {
            let branch = (self.pending_branch_checkout)();
            let mut pending = self.pending_branch_checkout;
            pending.set(String::new());
            if result.ok && !branch.is_empty() {
                GitWorkspace::select_branch_name(&snapshot.workspace, &branch);
            }
        }
    }

    fn reset(self, workspace: String, branch: String) {
        let mut snapshot = self.snapshot;
        snapshot.set(GitPageSnapshot {
            workspace,
            loading: true,
            ..Default::default()
        });
        let mut directory_selected = self.directory_selected;
        let mut directory_preview_path = self.directory_preview_path;
        let mut directory_came_from = self.directory_came_from;
        let mut directory_show_hidden = self.directory_show_hidden;
        let mut selected_path = self.selected_path;
        let mut selected_path_bytes = self.selected_path_bytes;
        let mut selected_abs_path = self.selected_abs_path;
        let mut selected_commit = self.selected_commit;
        let mut selected_branch = self.selected_branch;
        let mut branch_collection = self.branch_collection;
        let mut branch_prompt = self.branch_prompt;
        let mut branch_draft = self.branch_draft;
        let mut pending_branch_checkout = self.pending_branch_checkout;
        let mut selected_stash = self.selected_stash;
        let mut focused_panel = self.focused_panel;
        directory_selected.set(0);
        directory_preview_path.set(String::new());
        directory_came_from.set(String::new());
        directory_show_hidden.set(true);
        selected_path.set(String::new());
        selected_path_bytes.set(Vec::new());
        selected_abs_path.set(String::new());
        selected_commit.set(String::new());
        selected_branch.set(branch);
        branch_collection.set(BranchCollection::Local);
        branch_prompt.set(None);
        branch_draft.set(String::new());
        pending_branch_checkout.set(String::new());
        selected_stash.set(String::new());
        focused_panel.set(GitPanel::Status);
    }

    fn start_loading(self) {
        let mut snapshot = self.snapshot;
        snapshot.with_mut(|snapshot| snapshot.loading = true);
    }

    fn record_error(self, error: String) {
        let mut snapshot = self.snapshot;
        snapshot.with_mut(|snapshot| {
            if snapshot.command_log.len() >= 24 {
                snapshot.command_log.remove(0);
            }
            snapshot.command_log.push(GitCommandLogEntry {
                action: String::new(),
                message: error.clone(),
                ok: false,
            });
            snapshot.message = error;
            snapshot.loading = false;
            snapshot.fetching = false;
        });
    }

    pub(super) fn workspace(self) -> String {
        (self.snapshot)().workspace
    }

    pub(super) fn move_selection(self, direction: MenuDirection) -> bool {
        let Some(repository) = (self.snapshot)().repository else {
            return false;
        };
        match (self.focused_panel)() {
            GitPanel::Status => false,
            GitPanel::Files => self.move_file_selection(&repository, direction),
            GitPanel::Branches => self.move_branch_selection(&repository, direction),
            GitPanel::Commits => self.move_commit_selection(&repository, direction),
            GitPanel::Stash => self.move_stash_selection(&repository, direction),
        }
    }

    fn move_file_selection(
        self,
        repository: &GitRepositorySnapshot,
        direction: MenuDirection,
    ) -> bool {
        let len = repository.files.len();
        if len == 0 {
            return false;
        }
        let current = repository
            .files
            .iter()
            .position(|entry| entry.path_bytes == (self.selected_path_bytes)())
            .unwrap_or(match direction {
                MenuDirection::Next => len - 1,
                MenuDirection::Previous => 0,
            });
        let index = move_selection(current, len, direction);
        let entry = &repository.files[index];
        let mut selected_path = self.selected_path;
        let mut selected_path_bytes = self.selected_path_bytes;
        let mut selected_abs_path = self.selected_abs_path;
        selected_path.set(entry.path.clone());
        selected_path_bytes.set(entry.path_bytes.clone());
        selected_abs_path.set(GitWorkspace::absolute_path(
            &repository.repo_root,
            &entry.path,
        ));
        let section = if entry.staged { "staged" } else { "unstaged" };
        ScrollIntoView::nearest(&format!("git-file-{section}-row-{index}"));
        true
    }

    fn move_branch_selection(
        self,
        repository: &GitRepositorySnapshot,
        direction: MenuDirection,
    ) -> bool {
        let references = (self.branch_collection)().references(repository);
        let len = references.len();
        if len == 0 {
            return false;
        }
        let current = references
            .iter()
            .position(|reference| reference == &(self.selected_branch)())
            .unwrap_or(match direction {
                MenuDirection::Next => len - 1,
                MenuDirection::Previous => 0,
            });
        let index = move_selection(current, len, direction);
        let mut selected_branch = self.selected_branch;
        selected_branch.set(references[index].clone());
        ScrollIntoView::nearest(&format!("git-branch-row-{index}"));
        true
    }

    fn move_commit_selection(
        self,
        repository: &GitRepositorySnapshot,
        direction: MenuDirection,
    ) -> bool {
        let len = repository.commits.len();
        if len == 0 {
            return false;
        }
        let current = repository
            .commits
            .iter()
            .position(|entry| entry.sha == (self.selected_commit)())
            .unwrap_or(match direction {
                MenuDirection::Next => len - 1,
                MenuDirection::Previous => 0,
            });
        let index = move_selection(current, len, direction);
        let mut selected_commit = self.selected_commit;
        selected_commit.set(repository.commits[index].sha.clone());
        ScrollIntoView::nearest(&format!("git-commit-row-{index}"));
        true
    }

    fn move_stash_selection(
        self,
        repository: &GitRepositorySnapshot,
        direction: MenuDirection,
    ) -> bool {
        let len = repository.stashes.len();
        if len == 0 {
            return false;
        }
        let current = repository
            .stashes
            .iter()
            .position(|entry| entry.reference == (self.selected_stash)())
            .unwrap_or(match direction {
                MenuDirection::Next => len - 1,
                MenuDirection::Previous => 0,
            });
        let index = move_selection(current, len, direction);
        let mut selected_stash = self.selected_stash;
        selected_stash.set(repository.stashes[index].reference.clone());
        ScrollIntoView::nearest(&format!("git-stash-row-{index}"));
        true
    }

    pub(super) fn submit_branch_prompt(self, prompt: &BranchPrompt) -> bool {
        let workspace = self.workspace();
        match prompt {
            BranchPrompt::Create { base } => {
                let branch = (self.branch_draft)().trim().to_string();
                if branch.is_empty() {
                    return false;
                }
                let mut pending_checkout = self.pending_branch_checkout;
                pending_checkout.set(branch.clone());
                GitWorkspace::operate(
                    &workspace,
                    GitOperation::CreateBranch {
                        branch,
                        start_point: base.clone(),
                    },
                );
            }
            BranchPrompt::Delete { branch } => {
                GitWorkspace::operate(
                    &workspace,
                    GitOperation::DeleteBranch {
                        branch: branch.clone(),
                    },
                );
            }
        }
        true
    }
}
