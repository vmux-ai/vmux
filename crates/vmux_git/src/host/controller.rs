use std::path::Path;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_core::event::space::ProjectActivateRequest;
use vmux_core::host::UiStateWrite;

use crate::event::{
    GitAmendRequest, GitBranchCollectionSelectRequest, GitBranchLogRequest, GitBranchSelectRequest,
    GitCheckoutCommitRequest, GitCherryPickRequest, GitCommitSelectRequest, GitConfigEditRequest,
    GitDiscardFileRequest, GitDiscardRequest, GitFastForwardRequest, GitFileSelectRequest,
    GitKeyRequest, GitMergeRequest, GitPanelSelectRequest, GitRebaseRequest,
    GitRepositoryPickerRequest, GitRepositoryRequest, GitRepositorySnapshot, GitRevertRequest,
    GitStageAllRequest, GitStageRequest, GitStashDropRequest, GitStashPopRequest,
    GitStashPushRequest, GitStashSelectRequest, GitUnstageRequest, GitUpdateCheckRequest,
};
use crate::state::{
    GitBranchCollection, GitBranchPrompt, GitBranchPromptRequested, GitOperationEligibility,
    GitPageContext, GitPageControllerState, GitPanel, GitRepositoryPicked, GitSelectionReveal,
    GitShortcutHelpToggle, GitUiState, GitUiStatePatch, GitWorkspaceChanged,
};

use super::state::GitState;

pub(super) struct ControllerPlugin;

impl Plugin for ControllerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            GitKeyRequest,
            GitPanelSelectRequest,
            GitFileSelectRequest,
            GitBranchCollectionSelectRequest,
            GitBranchSelectRequest,
            GitCommitSelectRequest,
            GitStashSelectRequest,
            GitDiscardFileRequest,
        )>::default())
            .add_observer(on_git_ui_state_write)
            .add_observer(on_key_request)
            .add_observer(on_panel_select_request)
            .add_observer(on_file_select_request)
            .add_observer(on_branch_collection_select_request)
            .add_observer(on_branch_select_request)
            .add_observer(on_commit_select_request)
            .add_observer(on_stash_select_request)
            .add_observer(on_discard_file_request);
    }
}

#[derive(Component, Default)]
pub(super) struct GitController {
    state: GitPageControllerState,
    pending_branch_checkout: String,
}

impl GitController {
    pub(super) fn state(&self) -> &GitPageControllerState {
        &self.state
    }

    pub(super) fn reset(&mut self, branch: String) {
        self.state = GitPageControllerState {
            selected_branch: branch,
            ..Default::default()
        };
        self.pending_branch_checkout.clear();
    }

    pub(super) fn reconcile_repository(&mut self, repository: &GitRepositorySnapshot) {
        let next_file = repository
            .files
            .iter()
            .find(|entry| entry.path_bytes == self.state.selected_path_bytes)
            .or_else(|| repository.files.first());
        self.state.selected_abs_path = next_file
            .map(|entry| {
                Path::new(&repository.repo_root)
                    .join(&entry.path)
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_default();
        self.state.selected_path = next_file
            .map(|entry| entry.path.clone())
            .unwrap_or_default();
        self.state.selected_path_bytes = next_file
            .map(|entry| entry.path_bytes.clone())
            .unwrap_or_default();
        self.state.selected_commit = repository
            .commits
            .iter()
            .find(|entry| entry.sha == self.state.selected_commit)
            .or_else(|| repository.commits.first())
            .map(|entry| entry.sha.clone())
            .unwrap_or_default();
        self.state.selected_branch = self
            .state
            .branch_collection
            .selected_reference(repository, &self.state.selected_branch);
        self.state.selected_stash = repository
            .stashes
            .iter()
            .find(|entry| entry.reference == self.state.selected_stash)
            .or_else(|| repository.stashes.first())
            .map(|entry| entry.reference.clone())
            .unwrap_or_default();
        if !repository
            .files
            .iter()
            .any(|entry| entry.path_bytes == self.state.confirm_discard && entry.can_discard())
        {
            self.state.confirm_discard.clear();
        }
        self.refresh_operations(repository);
    }

    pub(super) fn begin_branch_creation(&mut self, branch: String) {
        self.pending_branch_checkout = branch;
    }

    pub(super) fn apply_result(
        &mut self,
        result: &crate::event::GitOperationResult,
    ) -> Option<String> {
        if result.operation != "new branch" {
            return None;
        }
        let branch = std::mem::take(&mut self.pending_branch_checkout);
        (result.ok && !branch.is_empty()).then_some(branch)
    }

    fn refresh_operations(&mut self, repository: &GitRepositorySnapshot) {
        self.state.operations = GitOperationEligibility::for_controller(&self.state, repository);
    }

    fn select_panel(&mut self, panel: GitPanel, repository: Option<&GitRepositorySnapshot>) {
        self.state.focused_panel = panel;
        if let Some(repository) = repository {
            self.refresh_operations(repository);
        }
    }

    fn select_file(&mut self, path_bytes: &[u8], repository: &GitRepositorySnapshot) -> bool {
        let Some(entry) = repository
            .files
            .iter()
            .find(|entry| entry.path_bytes == path_bytes)
        else {
            return false;
        };
        self.state.focused_panel = GitPanel::Files;
        self.state.selected_path.clone_from(&entry.path);
        self.state.selected_path_bytes.clone_from(&entry.path_bytes);
        self.state.selected_abs_path = Path::new(&repository.repo_root)
            .join(&entry.path)
            .to_string_lossy()
            .into_owned();
        self.refresh_operations(repository);
        true
    }

    fn select_branch_collection(
        &mut self,
        collection: GitBranchCollection,
        repository: &GitRepositorySnapshot,
    ) {
        self.state.branch_collection = collection;
        self.state.selected_branch = collection.selected_reference(repository, "");
        self.refresh_operations(repository);
    }

    fn select_branch(&mut self, reference: &str, repository: &GitRepositorySnapshot) -> bool {
        if !self
            .state
            .branch_collection
            .references(repository)
            .iter()
            .any(|candidate| candidate == reference)
        {
            return false;
        }
        self.state.focused_panel = GitPanel::Branches;
        self.state.selected_branch = reference.to_string();
        self.refresh_operations(repository);
        true
    }

    fn select_commit(&mut self, commit: &str, repository: &GitRepositorySnapshot) -> bool {
        if !repository.commits.iter().any(|entry| entry.sha == commit) {
            return false;
        }
        self.state.focused_panel = GitPanel::Commits;
        self.state.selected_commit = commit.to_string();
        self.refresh_operations(repository);
        true
    }

    fn select_stash(&mut self, reference: &str, repository: &GitRepositorySnapshot) -> bool {
        if !repository
            .stashes
            .iter()
            .any(|entry| entry.reference == reference)
        {
            return false;
        }
        self.state.focused_panel = GitPanel::Stash;
        self.state.selected_stash = reference.to_string();
        self.refresh_operations(repository);
        true
    }

    pub(super) fn branch_log_request(&self, state: &GitState) -> Option<GitBranchLogRequest> {
        if self.state.focused_panel != GitPanel::Branches || self.state.selected_branch.is_empty() {
            return None;
        }
        let repository = state.repository()?;
        if state.branch_log().is_some_and(|event| {
            event.repo_root == repository.repo_root && event.branch == self.state.selected_branch
        }) {
            return None;
        }
        Some(GitBranchLogRequest {
            repo_root: repository.repo_root.clone(),
            branch: self.state.selected_branch.clone(),
        })
    }

    fn move_selection(
        &mut self,
        direction: SelectionDirection,
        repository: &GitRepositorySnapshot,
    ) -> Option<String> {
        let reveal = match self.state.focused_panel {
            GitPanel::Status => return None,
            GitPanel::Files => self.move_file_selection(direction, repository),
            GitPanel::Branches => self.move_branch_selection(direction, repository),
            GitPanel::Commits => self.move_commit_selection(direction, repository),
            GitPanel::Stash => self.move_stash_selection(direction, repository),
        };
        self.refresh_operations(repository);
        reveal
    }

    fn move_file_selection(
        &mut self,
        direction: SelectionDirection,
        repository: &GitRepositorySnapshot,
    ) -> Option<String> {
        let len = repository.files.len();
        if len == 0 {
            return None;
        }
        let current = repository
            .files
            .iter()
            .position(|entry| entry.path_bytes == self.state.selected_path_bytes)
            .unwrap_or(direction.fallback(len));
        let index = direction.index(current, len);
        let entry = &repository.files[index];
        self.state.selected_path.clone_from(&entry.path);
        self.state.selected_path_bytes.clone_from(&entry.path_bytes);
        self.state.selected_abs_path = Path::new(&repository.repo_root)
            .join(&entry.path)
            .to_string_lossy()
            .into_owned();
        let section = if entry.staged { "staged" } else { "unstaged" };
        Some(format!("git-file-{section}-row-{index}"))
    }

    fn move_branch_selection(
        &mut self,
        direction: SelectionDirection,
        repository: &GitRepositorySnapshot,
    ) -> Option<String> {
        let references = self.state.branch_collection.references(repository);
        let len = references.len();
        if len == 0 {
            return None;
        }
        let current = references
            .iter()
            .position(|reference| reference == &self.state.selected_branch)
            .unwrap_or(direction.fallback(len));
        let index = direction.index(current, len);
        self.state.selected_branch.clone_from(&references[index]);
        Some(format!("git-branch-row-{index}"))
    }

    fn move_commit_selection(
        &mut self,
        direction: SelectionDirection,
        repository: &GitRepositorySnapshot,
    ) -> Option<String> {
        let len = repository.commits.len();
        if len == 0 {
            return None;
        }
        let current = repository
            .commits
            .iter()
            .position(|entry| entry.sha == self.state.selected_commit)
            .unwrap_or(direction.fallback(len));
        let index = direction.index(current, len);
        self.state
            .selected_commit
            .clone_from(&repository.commits[index].sha);
        Some(format!("git-commit-row-{index}"))
    }

    fn move_stash_selection(
        &mut self,
        direction: SelectionDirection,
        repository: &GitRepositorySnapshot,
    ) -> Option<String> {
        let len = repository.stashes.len();
        if len == 0 {
            return None;
        }
        let current = repository
            .stashes
            .iter()
            .position(|entry| entry.reference == self.state.selected_stash)
            .unwrap_or(direction.fallback(len));
        let index = direction.index(current, len);
        self.state
            .selected_stash
            .clone_from(&repository.stashes[index].reference);
        Some(format!("git-stash-row-{index}"))
    }

    fn discard_file(
        &mut self,
        webview: Entity,
        path_bytes: &[u8],
        repository: &GitRepositorySnapshot,
        commands: &mut Commands,
    ) -> bool {
        let Some(entry) = repository
            .files
            .iter()
            .find(|entry| entry.path_bytes == path_bytes && entry.can_discard())
        else {
            return false;
        };
        self.state.focused_panel = GitPanel::Files;
        if self.state.confirm_discard == entry.path_bytes {
            commands.trigger(UiInput {
                webview,
                payload: GitDiscardRequest {
                    repo_root: repository.repo_root.clone(),
                    path: Path::new(&repository.repo_root)
                        .join(&entry.path)
                        .to_string_lossy()
                        .into_owned(),
                    path_bytes: entry.path_bytes.clone(),
                },
            });
            self.state.confirm_discard.clear();
        } else {
            self.state.confirm_discard.clone_from(&entry.path_bytes);
        }
        self.refresh_operations(repository);
        true
    }
}

fn dispatch_git_key(
    controller: &mut GitController,
    webview: Entity,
    request: &GitKeyRequest,
    state: &GitState,
    commands: &mut Commands,
) {
    if let Some(direction) = SelectionDirection::from_key(request) {
        let Some(repository) = state.repository() else {
            return;
        };
        if let Some(id) = controller.move_selection(direction, repository) {
            commands.trigger(UiStateWrite::<GitUiState>::from_event(
                webview,
                &GitSelectionReveal { id },
            ));
            request_branch_log(controller, webview, state, commands);
        }
        return;
    }
    if request.repeat
        || request.modifiers.ctrl
        || request.modifiers.alt
        || request.modifiers.super_key
    {
        return;
    }
    if request.key == "?" {
        commands.trigger(UiStateWrite::<GitUiState>::from_event(
            webview,
            &GitShortcutHelpToggle,
        ));
        return;
    }
    if request.key == "Tab" {
        controller.select_panel(
            controller.state.focused_panel.next(request.modifiers.shift),
            state.repository(),
        );
        request_branch_log(controller, webview, state, commands);
        return;
    }
    if let Some(panel) = GitPanel::from_key(&request.key) {
        controller.select_panel(panel, state.repository());
        request_branch_log(controller, webview, state, commands);
        return;
    }
    let Some(repository) = state.repository() else {
        return;
    };
    match (controller.state.focused_panel, request.key.as_str()) {
        (GitPanel::Status, "e") => commands.trigger(UiInput {
            webview,
            payload: GitConfigEditRequest {
                repo_root: repository.repo_root.clone(),
            },
        }),
        (GitPanel::Status, "u") => commands.trigger(UiInput {
            webview,
            payload: GitUpdateCheckRequest,
        }),
        (GitPanel::Status, "Enter") => commands.trigger(UiInput {
            webview,
            payload: GitRepositoryPickerRequest {
                path: repository.repo_root.clone(),
            },
        }),
        (GitPanel::Files, "a") if controller.state.operations.stage_all => {
            commands.trigger(UiInput {
                webview,
                payload: GitStageAllRequest {
                    path: repository.repo_root.clone(),
                },
            })
        }
        (GitPanel::Files, "s") if controller.state.operations.stash => commands.trigger(UiInput {
            webview,
            payload: GitStashPushRequest {
                repo_root: repository.repo_root.clone(),
            },
        }),
        (GitPanel::Files, "A") if controller.state.operations.amend => commands.trigger(UiInput {
            webview,
            payload: GitAmendRequest {
                repo_root: repository.repo_root.clone(),
            },
        }),
        (GitPanel::Files, " " | "Space") if controller.state.operations.toggle_stage => {
            let Some(entry) = repository
                .files
                .iter()
                .find(|entry| entry.path_bytes == controller.state.selected_path_bytes)
            else {
                return;
            };
            let path = Path::new(&repository.repo_root)
                .join(&entry.path)
                .to_string_lossy()
                .into_owned();
            if entry.unstaged {
                commands.trigger(UiInput {
                    webview,
                    payload: GitStageRequest {
                        repo_root: repository.repo_root.clone(),
                        path,
                        path_bytes: entry.path_bytes.clone(),
                    },
                });
            } else if entry.staged {
                commands.trigger(UiInput {
                    webview,
                    payload: GitUnstageRequest {
                        repo_root: repository.repo_root.clone(),
                        path,
                        path_bytes: entry.path_bytes.clone(),
                    },
                });
            }
        }
        (GitPanel::Files, "x") if controller.state.operations.discard => {
            let selected = controller.state.selected_path_bytes.clone();
            controller.discard_file(webview, &selected, repository, commands);
        }
        (GitPanel::Branches, "Enter" | " " | "Space" | "c")
            if controller.state.operations.checkout_branch =>
        {
            let Some(branch) = repository
                .branches
                .iter()
                .find(|branch| branch.name == controller.state.selected_branch)
            else {
                return;
            };
            commands.trigger(UiInput {
                webview,
                payload: ProjectActivateRequest {
                    path: repository.repo_root.clone(),
                    branch: branch.name.clone(),
                    checkout: branch.checkout.clone(),
                    pane_id: None,
                },
            });
        }
        (GitPanel::Branches, "r") if controller.state.operations.rebase => {
            commands.trigger(UiInput {
                webview,
                payload: GitRebaseRequest {
                    repo_root: repository.repo_root.clone(),
                    branch: controller.state.selected_branch.clone(),
                },
            })
        }
        (GitPanel::Branches, "M") if controller.state.operations.merge => {
            commands.trigger(UiInput {
                webview,
                payload: GitMergeRequest {
                    repo_root: repository.repo_root.clone(),
                    branch: controller.state.selected_branch.clone(),
                },
            })
        }
        (GitPanel::Branches, "f") if controller.state.operations.fast_forward => {
            commands.trigger(UiInput {
                webview,
                payload: GitFastForwardRequest {
                    repo_root: repository.repo_root.clone(),
                    branch: controller.state.selected_branch.clone(),
                },
            })
        }
        (GitPanel::Branches, "n") if controller.state.operations.create_branch => {
            commands.trigger(UiStateWrite::<GitUiState>::from_event(
                webview,
                &GitBranchPromptRequested {
                    prompt: GitBranchPrompt::Create {
                        base: controller.state.selected_branch.clone(),
                    },
                },
            ));
        }
        (GitPanel::Branches, "d") if controller.state.operations.delete_branch => {
            commands.trigger(UiStateWrite::<GitUiState>::from_event(
                webview,
                &GitBranchPromptRequested {
                    prompt: GitBranchPrompt::Delete {
                        branch: controller.state.selected_branch.clone(),
                    },
                },
            ));
        }
        (GitPanel::Commits, " " | "Space") if controller.state.operations.checkout_commit => {
            commands.trigger(UiInput {
                webview,
                payload: GitCheckoutCommitRequest {
                    repo_root: repository.repo_root.clone(),
                    commit: controller.state.selected_commit.clone(),
                },
            })
        }
        (GitPanel::Commits, "C" | "V") if controller.state.operations.cherry_pick => commands
            .trigger(UiInput {
                webview,
                payload: GitCherryPickRequest {
                    repo_root: repository.repo_root.clone(),
                    commit: controller.state.selected_commit.clone(),
                },
            }),
        (GitPanel::Commits, "t") if controller.state.operations.revert_commit => {
            commands.trigger(UiInput {
                webview,
                payload: GitRevertRequest {
                    repo_root: repository.repo_root.clone(),
                    commit: controller.state.selected_commit.clone(),
                },
            })
        }
        (GitPanel::Stash, "g") if controller.state.operations.stash_pop => {
            commands.trigger(UiInput {
                webview,
                payload: GitStashPopRequest {
                    repo_root: repository.repo_root.clone(),
                    reference: controller.state.selected_stash.clone(),
                },
            })
        }
        (GitPanel::Stash, "d") if controller.state.operations.stash_drop => {
            commands.trigger(UiInput {
                webview,
                payload: GitStashDropRequest {
                    repo_root: repository.repo_root.clone(),
                    reference: controller.state.selected_stash.clone(),
                },
            })
        }
        _ => {}
    }
}

fn request_branch_log(
    controller: &GitController,
    webview: Entity,
    state: &GitState,
    commands: &mut Commands,
) {
    let Some(payload) = controller.branch_log_request(state) else {
        return;
    };
    commands.trigger(UiInput { webview, payload });
}

impl GitOperationEligibility {
    fn for_controller(
        controller: &GitPageControllerState,
        repository: &GitRepositorySnapshot,
    ) -> Self {
        let file = repository
            .files
            .iter()
            .find(|entry| entry.path_bytes == controller.selected_path_bytes);
        let branch = if controller.branch_collection == GitBranchCollection::Local {
            repository
                .branches
                .iter()
                .find(|branch| branch.name == controller.selected_branch)
        } else {
            None
        };
        let commit = repository
            .commits
            .iter()
            .any(|entry| entry.sha == controller.selected_commit);
        let stash = repository
            .stashes
            .iter()
            .any(|entry| entry.reference == controller.selected_stash);
        let staged = repository.files.iter().any(|entry| entry.staged);
        let branch_selected = branch.is_some();
        let branch_mutable = branch.is_some_and(|branch| !branch.current);
        Self {
            stage_all: true,
            stash: !repository.files.is_empty(),
            amend: staged && !repository.commits.is_empty(),
            toggle_stage: file.is_some_and(|entry| entry.staged || entry.unstaged),
            discard: file.is_some_and(|entry| entry.can_discard()),
            commit: staged,
            push: !repository.branch.is_empty(),
            checkout_branch: branch_selected,
            create_branch: branch_selected,
            delete_branch: branch
                .is_some_and(|branch| !branch.current && branch.checkout.is_empty()),
            rebase: branch_mutable,
            merge: branch_mutable,
            fast_forward: branch_mutable,
            checkout_commit: commit,
            cherry_pick: commit,
            revert_commit: commit,
            stash_pop: stash,
            stash_drop: stash,
        }
    }
}

#[derive(Clone, Copy)]
enum SelectionDirection {
    Next,
    Previous,
}

impl SelectionDirection {
    fn from_key(request: &GitKeyRequest) -> Option<Self> {
        if request.modifiers.alt || request.modifiers.super_key || request.modifiers.shift {
            return None;
        }
        if request.modifiers.ctrl {
            return match request.code.as_str() {
                "KeyN" | "KeyJ" => Some(Self::Next),
                "KeyP" | "KeyK" => Some(Self::Previous),
                _ => None,
            };
        }
        match (request.code.as_str(), request.key.as_str()) {
            ("ArrowDown", _) | (_, "j") => Some(Self::Next),
            ("ArrowUp", _) | (_, "k") => Some(Self::Previous),
            _ => None,
        }
    }

    fn fallback(self, len: usize) -> usize {
        match self {
            Self::Next => len - 1,
            Self::Previous => 0,
        }
    }

    fn index(self, current: usize, len: usize) -> usize {
        match self {
            Self::Next => (current + 1) % len,
            Self::Previous => (current + len - 1) % len,
        }
    }
}

fn on_git_ui_state_write(
    trigger: On<UiStateWrite<GitUiState>>,
    mut pages: Query<(&mut GitState, &mut GitController)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview();
    let Ok((mut state, mut controller)) = pages.get_mut(webview) else {
        return;
    };
    match trigger.event().patch() {
        GitUiStatePatch::Context(GitPageContext {
            working_directory,
            page_url,
        }) => {
            let path = crate::GitUrl::parse(page_url)
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|| working_directory.clone());
            state.reset(path.clone());
            controller.reset(String::new());
            commands.trigger(UiInput {
                webview,
                payload: crate::event::GitDirectoryRequest {
                    path,
                    preview: false,
                },
            });
        }
        GitUiStatePatch::RepositoryPicked(GitRepositoryPicked { path }) => {
            if !path.is_empty() {
                commands.trigger(UiInput {
                    webview,
                    payload: crate::event::GitDirectoryRequest {
                        path: path.clone(),
                        preview: false,
                    },
                });
            }
        }
        GitUiStatePatch::Workspace(GitWorkspaceChanged {
            path,
            branch,
            error,
        }) => {
            if !error.is_empty() {
                state.apply_workspace_error(error.clone());
                return;
            }
            if path.is_empty() || path == state.workspace() {
                return;
            }
            state.reset(path.clone());
            controller.reset(branch.clone());
            commands.trigger(UiInput {
                webview,
                payload: GitRepositoryRequest { path: path.clone() },
            });
        }
        GitUiStatePatch::Snapshot(_)
        | GitUiStatePatch::Controller(_)
        | GitUiStatePatch::BranchPrompt(_)
        | GitUiStatePatch::SelectionReveal(_)
        | GitUiStatePatch::ShortcutHelpToggle(_) => {}
    }
}

fn on_key_request(
    trigger: On<UiInput<GitKeyRequest>>,
    mut pages: Query<(&GitState, &mut GitController)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((state, mut controller)) = pages.get_mut(webview) else {
        return;
    };
    let previous = controller.state().clone();
    dispatch_git_key(
        controller.bypass_change_detection(),
        webview,
        &trigger.event().payload,
        state,
        &mut commands,
    );
    if controller.state() != &previous {
        controller.set_changed();
    }
}

fn on_panel_select_request(
    trigger: On<UiInput<GitPanelSelectRequest>>,
    mut pages: Query<(&GitState, &mut GitController)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((state, mut controller)) = pages.get_mut(webview) else {
        return;
    };
    controller.select_panel(trigger.event().payload.panel, state.repository());
    request_branch_log(&controller, webview, state, &mut commands);
}

fn on_file_select_request(
    trigger: On<UiInput<GitFileSelectRequest>>,
    mut pages: Query<(&GitState, &mut GitController)>,
) {
    let Ok((state, mut controller)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    controller.select_file(&trigger.event().payload.path_bytes, repository);
}

fn on_branch_collection_select_request(
    trigger: On<UiInput<GitBranchCollectionSelectRequest>>,
    mut pages: Query<(&GitState, &mut GitController)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((state, mut controller)) = pages.get_mut(webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    controller.select_branch_collection(trigger.event().payload.collection, repository);
    request_branch_log(&controller, webview, state, &mut commands);
}

fn on_branch_select_request(
    trigger: On<UiInput<GitBranchSelectRequest>>,
    mut pages: Query<(&GitState, &mut GitController)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((state, mut controller)) = pages.get_mut(webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    if controller.select_branch(&trigger.event().payload.reference, repository) {
        request_branch_log(&controller, webview, state, &mut commands);
    }
}

fn on_commit_select_request(
    trigger: On<UiInput<GitCommitSelectRequest>>,
    mut pages: Query<(&GitState, &mut GitController)>,
) {
    let Ok((state, mut controller)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    controller.select_commit(&trigger.event().payload.commit, repository);
}

fn on_stash_select_request(
    trigger: On<UiInput<GitStashSelectRequest>>,
    mut pages: Query<(&GitState, &mut GitController)>,
) {
    let Ok((state, mut controller)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    controller.select_stash(&trigger.event().payload.reference, repository);
}

fn on_discard_file_request(
    trigger: On<UiInput<GitDiscardFileRequest>>,
    mut pages: Query<(&GitState, &mut GitController)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((state, mut controller)) = pages.get_mut(webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    controller.discard_file(
        webview,
        &trigger.event().payload.path_bytes,
        repository,
        &mut commands,
    );
}
