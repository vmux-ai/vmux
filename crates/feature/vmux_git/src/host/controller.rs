use std::path::Path;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::UiStateWrite;
use vmux_ecs::event::space::ProjectActivateRequest;

use crate::event::{
    GitAmendRequest, GitBranchCollectionSelectRequest, GitBranchDraftRequest, GitBranchLogRequest,
    GitBranchPromptCloseRequest, GitBranchPromptOpenRequest, GitBranchSelectRequest,
    GitBranchSubmitRequest, GitCheckoutCommitRequest, GitCherryPickRequest, GitCommitDraftRequest,
    GitCommitRequest, GitCommitSelectRequest, GitCommitSubmitRequest, GitConfigEditRequest,
    GitCreateBranchRequest, GitDeleteBranchRequest, GitDiscardFileRequest, GitDiscardRequest,
    GitFastForwardRequest, GitFileSelectRequest, GitKeyRequest, GitMergeRequest,
    GitPanelSelectRequest, GitRebaseRequest, GitRepositoryPickerRequest, GitRepositoryRequest,
    GitRepositorySnapshot, GitRevertRequest, GitShortcutHelpRequest, GitStageAllRequest,
    GitStageRequest, GitStashDropRequest, GitStashPopRequest, GitStashPushRequest,
    GitStashSelectRequest, GitUnstageRequest, GitUpdateCheckRequest,
};
use crate::state::{
    GitBranchCollection, GitBranchPrompt, GitOperationEligibility, GitPageContext,
    GitPageControllerState, GitPanel, GitRepositoryPicked, GitSelectionReveal, GitUiState,
    GitWorkspaceChanged,
};

use super::directory::GitDirectoryNavigation;
use super::state::{GitDiffRevealRanges, GitState};

pub(super) struct ControllerPlugin;

impl Plugin for ControllerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            GitKeyRequest,
            GitPanelSelectRequest,
            GitShortcutHelpRequest,
            GitFileSelectRequest,
            GitBranchCollectionSelectRequest,
            GitBranchSelectRequest,
            GitCommitSelectRequest,
            GitStashSelectRequest,
            GitDiscardFileRequest,
        )>::default())
            .add_observer(ui_state_write)
            .add_observer(key_request)
            .add_observer(panel_select_request)
            .add_observer(shortcut_help_request)
            .add_observer(file_select_request)
            .add_observer(select_branch_collection)
            .add_observer(branch_select_request)
            .add_observer(commit_select_request)
            .add_observer(stash_select_request)
            .add_observer(discard_file_request)
            .add_observer(open_branch_prompt)
            .add_observer(close_branch_prompt)
            .add_observer(edit_branch_draft)
            .add_observer(submit_branch)
            .add_observer(edit_commit_draft)
            .add_observer(submit_commit)
            .configure_sets(
                Update,
                (
                    ControllerSet::Reconcile,
                    ControllerSet::Operations,
                    ControllerSet::BranchLog,
                )
                    .chain()
                    .after(super::GitUpdateSet::Jobs),
            )
            .add_systems(
                Update,
                (
                    (reconcile, settle_commit)
                        .chain()
                        .in_set(ControllerSet::Reconcile),
                    operations.in_set(ControllerSet::Operations),
                    branch_log.in_set(ControllerSet::BranchLog),
                ),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum ControllerSet {
    Reconcile,
    Operations,
    BranchLog,
}

#[derive(Component, Default)]
pub(super) struct PendingBranchCheckout(pub(super) String);

#[derive(Component, Default)]
pub(super) struct SelectionRevealRevision(u64);

#[derive(Component, Default)]
pub(super) struct GitCommitResultSequence(u64);

impl GitPageControllerState {
    pub(super) fn reset(&mut self, branch: String) {
        *self = GitPageControllerState {
            selected_branch: branch,
            ..Default::default()
        };
    }

    fn select_panel(&mut self, panel: GitPanel) {
        self.focused_panel = panel;
    }

    fn open_branch_prompt(&mut self, prompt: GitBranchPrompt) {
        self.branch_prompt = Some(prompt);
        self.branch_draft.clear();
    }

    fn select_file(&mut self, path_bytes: &[u8], repository: &GitRepositorySnapshot) -> bool {
        let Some(entry) = repository
            .files
            .iter()
            .find(|entry| entry.path_bytes == path_bytes)
        else {
            return false;
        };
        self.focused_panel = GitPanel::Files;
        self.selected_path.clone_from(&entry.path);
        self.selected_path_bytes.clone_from(&entry.path_bytes);
        self.selected_abs_path = Path::new(&repository.repo_root)
            .join(&entry.path)
            .to_string_lossy()
            .into_owned();
        true
    }

    fn select_branch_collection(
        &mut self,
        collection: GitBranchCollection,
        repository: &GitRepositorySnapshot,
    ) {
        self.branch_collection = collection;
        self.selected_branch = collection.selected_reference(repository, "");
    }

    fn select_branch(&mut self, reference: &str, repository: &GitRepositorySnapshot) -> bool {
        if !self
            .branch_collection
            .references(repository)
            .iter()
            .any(|candidate| candidate == reference)
        {
            return false;
        }
        self.focused_panel = GitPanel::Branches;
        self.selected_branch = reference.to_string();
        true
    }

    fn select_commit(&mut self, commit: &str, repository: &GitRepositorySnapshot) -> bool {
        if !repository.commits.iter().any(|entry| entry.sha == commit) {
            return false;
        }
        self.focused_panel = GitPanel::Commits;
        self.selected_commit = commit.to_string();
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
        self.focused_panel = GitPanel::Stash;
        self.selected_stash = reference.to_string();
        true
    }

    fn move_selection(
        &mut self,
        direction: SelectionDirection,
        repository: &GitRepositorySnapshot,
        revision: &mut SelectionRevealRevision,
    ) -> Option<GitSelectionReveal> {
        let id = match self.focused_panel {
            GitPanel::Status => return None,
            GitPanel::Files => self.move_file_selection(direction, repository),
            GitPanel::Branches => self.move_branch_selection(direction, repository),
            GitPanel::Commits => self.move_commit_selection(direction, repository),
            GitPanel::Stash => self.move_stash_selection(direction, repository),
        }?;
        revision.0 = revision.0.wrapping_add(1).max(1);
        Some(GitSelectionReveal {
            id,
            revision: revision.0,
        })
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
            .position(|entry| entry.path_bytes == self.selected_path_bytes)
            .unwrap_or(direction.fallback(len));
        let index = direction.index(current, len);
        let entry = &repository.files[index];
        self.selected_path.clone_from(&entry.path);
        self.selected_path_bytes.clone_from(&entry.path_bytes);
        self.selected_abs_path = Path::new(&repository.repo_root)
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
        let references = self.branch_collection.references(repository);
        let len = references.len();
        if len == 0 {
            return None;
        }
        let current = references
            .iter()
            .position(|reference| reference == &self.selected_branch)
            .unwrap_or(direction.fallback(len));
        let index = direction.index(current, len);
        self.selected_branch.clone_from(&references[index]);
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
            .position(|entry| entry.sha == self.selected_commit)
            .unwrap_or(direction.fallback(len));
        let index = direction.index(current, len);
        self.selected_commit
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
            .position(|entry| entry.reference == self.selected_stash)
            .unwrap_or(direction.fallback(len));
        let index = direction.index(current, len);
        self.selected_stash
            .clone_from(&repository.stashes[index].reference);
        Some(format!("git-stash-row-{index}"))
    }

    fn discard_file(
        &mut self,
        path_bytes: &[u8],
        repository: &GitRepositorySnapshot,
    ) -> Option<GitDiscardRequest> {
        let entry = repository
            .files
            .iter()
            .find(|entry| entry.path_bytes == path_bytes && entry.can_discard())?;
        self.focused_panel = GitPanel::Files;
        if self.confirm_discard == entry.path_bytes {
            self.confirm_discard.clear();
            Some(GitDiscardRequest {
                repo_root: repository.repo_root.clone(),
                path: Path::new(&repository.repo_root)
                    .join(&entry.path)
                    .to_string_lossy()
                    .into_owned(),
                path_bytes: entry.path_bytes.clone(),
            })
        } else {
            self.confirm_discard.clone_from(&entry.path_bytes);
            None
        }
    }
}

type OperationPage<'a> = (&'a GitState, &'a mut GitPageControllerState);
type ChangedOperationPage = Or<(Changed<GitState>, Changed<GitPageControllerState>)>;

#[derive(SystemParam)]
struct OperationPages<'w, 's> {
    values: Query<'w, 's, OperationPage<'static>, ChangedOperationPage>,
}

fn dispatch_git_key(
    controller: &mut GitPageControllerState,
    revision: &mut SelectionRevealRevision,
    webview: Entity,
    request: &GitKeyRequest,
    state: &GitState,
    commands: &mut Commands,
) {
    if let Some(direction) = SelectionDirection::from_key(request) {
        let Some(repository) = state.repository() else {
            return;
        };
        if let Some(effect) = controller.move_selection(direction, repository, revision) {
            commands.trigger(UiStateWrite::<GitUiState>::from_event(webview, &effect));
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
        controller.shortcut_help_visible = !controller.shortcut_help_visible;
        return;
    }
    if request.key == "Tab" {
        controller.select_panel(controller.focused_panel.next(request.modifiers.shift));
        return;
    }
    if let Some(panel) = GitPanel::from_key(&request.key) {
        controller.select_panel(panel);
        return;
    }
    let Some(repository) = state.repository() else {
        return;
    };
    match (controller.focused_panel, request.key.as_str()) {
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
        (GitPanel::Files, "a") if controller.operations.stage_all => commands.trigger(UiInput {
            webview,
            payload: GitStageAllRequest {
                path: repository.repo_root.clone(),
            },
        }),
        (GitPanel::Files, "s") if controller.operations.stash => commands.trigger(UiInput {
            webview,
            payload: GitStashPushRequest {
                repo_root: repository.repo_root.clone(),
            },
        }),
        (GitPanel::Files, "A") if controller.operations.amend => commands.trigger(UiInput {
            webview,
            payload: GitAmendRequest {
                repo_root: repository.repo_root.clone(),
            },
        }),
        (GitPanel::Files, " " | "Space") if controller.operations.toggle_stage => {
            let Some(entry) = repository
                .files
                .iter()
                .find(|entry| entry.path_bytes == controller.selected_path_bytes)
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
        (GitPanel::Files, "x") if controller.operations.discard => {
            let selected = controller.selected_path_bytes.clone();
            if let Some(payload) = controller.discard_file(&selected, repository) {
                commands.trigger(UiInput { webview, payload });
            }
        }
        (GitPanel::Branches, "Enter" | " " | "Space" | "c")
            if controller.operations.checkout_branch =>
        {
            let Some(branch) = repository
                .branches
                .iter()
                .find(|branch| branch.name == controller.selected_branch)
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
        (GitPanel::Branches, "r") if controller.operations.rebase => commands.trigger(UiInput {
            webview,
            payload: GitRebaseRequest {
                repo_root: repository.repo_root.clone(),
                branch: controller.selected_branch.clone(),
            },
        }),
        (GitPanel::Branches, "M") if controller.operations.merge => commands.trigger(UiInput {
            webview,
            payload: GitMergeRequest {
                repo_root: repository.repo_root.clone(),
                branch: controller.selected_branch.clone(),
            },
        }),
        (GitPanel::Branches, "f") if controller.operations.fast_forward => {
            commands.trigger(UiInput {
                webview,
                payload: GitFastForwardRequest {
                    repo_root: repository.repo_root.clone(),
                    branch: controller.selected_branch.clone(),
                },
            })
        }
        (GitPanel::Branches, "n") if controller.operations.create_branch => {
            controller.open_branch_prompt(GitBranchPrompt::Create {
                base: controller.selected_branch.clone(),
            });
        }
        (GitPanel::Branches, "d") if controller.operations.delete_branch => {
            controller.open_branch_prompt(GitBranchPrompt::Delete {
                branch: controller.selected_branch.clone(),
            });
        }
        (GitPanel::Commits, " " | "Space") if controller.operations.checkout_commit => commands
            .trigger(UiInput {
                webview,
                payload: GitCheckoutCommitRequest {
                    repo_root: repository.repo_root.clone(),
                    commit: controller.selected_commit.clone(),
                },
            }),
        (GitPanel::Commits, "C" | "V") if controller.operations.cherry_pick => {
            commands.trigger(UiInput {
                webview,
                payload: GitCherryPickRequest {
                    repo_root: repository.repo_root.clone(),
                    commit: controller.selected_commit.clone(),
                },
            })
        }
        (GitPanel::Commits, "t") if controller.operations.revert_commit => {
            commands.trigger(UiInput {
                webview,
                payload: GitRevertRequest {
                    repo_root: repository.repo_root.clone(),
                    commit: controller.selected_commit.clone(),
                },
            })
        }
        (GitPanel::Stash, "g") if controller.operations.stash_pop => commands.trigger(UiInput {
            webview,
            payload: GitStashPopRequest {
                repo_root: repository.repo_root.clone(),
                reference: controller.selected_stash.clone(),
            },
        }),
        (GitPanel::Stash, "d") if controller.operations.stash_drop => commands.trigger(UiInput {
            webview,
            payload: GitStashDropRequest {
                repo_root: repository.repo_root.clone(),
                reference: controller.selected_stash.clone(),
            },
        }),
        _ => {}
    }
}

fn reconcile(mut pages: Query<(&GitState, &mut GitPageControllerState), Changed<GitState>>) {
    for (state, mut controller) in &mut pages {
        let Some(repository) = state.repository() else {
            continue;
        };
        let mut next = controller.clone();
        let next_file = repository
            .files
            .iter()
            .find(|entry| entry.path_bytes == next.selected_path_bytes)
            .or_else(|| repository.files.first());
        next.selected_abs_path = next_file
            .map(|entry| {
                Path::new(&repository.repo_root)
                    .join(&entry.path)
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_default();
        next.selected_path = next_file
            .map(|entry| entry.path.clone())
            .unwrap_or_default();
        next.selected_path_bytes = next_file
            .map(|entry| entry.path_bytes.clone())
            .unwrap_or_default();
        next.selected_commit = repository
            .commits
            .iter()
            .find(|entry| entry.sha == next.selected_commit)
            .or_else(|| repository.commits.first())
            .map(|entry| entry.sha.clone())
            .unwrap_or_default();
        next.selected_branch = next
            .branch_collection
            .selected_reference(repository, &next.selected_branch);
        next.selected_stash = repository
            .stashes
            .iter()
            .find(|entry| entry.reference == next.selected_stash)
            .or_else(|| repository.stashes.first())
            .map(|entry| entry.reference.clone())
            .unwrap_or_default();
        if !repository
            .files
            .iter()
            .any(|entry| entry.path_bytes == next.confirm_discard && entry.can_discard())
        {
            next.confirm_discard.clear();
        }
        if *controller != next {
            *controller = next;
        }
    }
}

fn settle_commit(
    mut pages: Query<
        (
            &GitState,
            &mut GitPageControllerState,
            &mut GitCommitResultSequence,
        ),
        Changed<GitState>,
    >,
) {
    for (state, mut controller, mut handled) in &mut pages {
        let sequence = state.snapshot.result_sequence;
        if sequence == 0 || sequence == handled.0 {
            continue;
        }
        handled.0 = sequence;
        let Some(result) = state.snapshot.result.as_ref() else {
            continue;
        };
        if result.operation != "commit" {
            continue;
        }
        if result.ok && controller.commit_message.trim() == controller.commit_pending {
            controller.commit_message.clear();
        }
        controller.commit_pending.clear();
    }
}

fn operations(mut pages: OperationPages) {
    for (state, mut controller) in &mut pages.values {
        let Some(repository) = state.repository() else {
            if controller.operations != GitOperationEligibility::default() {
                controller.operations = GitOperationEligibility::default();
            }
            continue;
        };
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
        let next = GitOperationEligibility {
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
        };
        if controller.operations != next {
            controller.operations = next;
        }
    }
}

fn branch_log(
    pages: Query<(Entity, Ref<GitState>, Ref<GitPageControllerState>)>,
    mut commands: Commands,
) {
    for (webview, state, controller) in &pages {
        if !state.is_changed() && !controller.is_changed() {
            continue;
        }
        if controller.focused_panel != GitPanel::Branches || controller.selected_branch.is_empty() {
            continue;
        }
        let Some(repository) = state.repository() else {
            continue;
        };
        if state.branch_log().is_some_and(|event| {
            event.repo_root == repository.repo_root && event.branch == controller.selected_branch
        }) {
            continue;
        }
        commands.trigger(UiInput {
            webview,
            payload: GitBranchLogRequest {
                repo_root: repository.repo_root.clone(),
                branch: controller.selected_branch.clone(),
            },
        });
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

fn ui_state_write(
    trigger: On<UiStateWrite<GitUiState>>,
    mut pages: Query<(
        &mut GitPageControllerState,
        &mut GitDirectoryNavigation,
        &mut PendingBranchCheckout,
        &mut SelectionRevealRevision,
        &mut GitCommitResultSequence,
        &mut GitDiffRevealRanges,
    )>,
    mut states: super::state::GitStates,
    mut commands: Commands,
) {
    let webview = trigger.event().webview();
    if !states.contains(webview) {
        return;
    }
    let Ok((
        mut controller,
        mut directory,
        mut pending,
        mut revision,
        mut commit_result,
        mut revealed,
    )) = pages.get_mut(webview)
    else {
        return;
    };
    let patch = trigger.event().update();
    if let Some(GitPageContext {
        working_directory,
        page_url,
    }) = &patch.context
    {
        let path = crate::GitUrl::parse(page_url)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| working_directory.clone());
        states.reset(webview, path.clone());
        controller.reset(String::new());
        pending.0.clear();
        revision.0 = 0;
        commit_result.0 = 0;
        revealed.0.clear();
        *directory = GitDirectoryNavigation::default();
        commands.trigger(UiInput {
            webview,
            payload: crate::event::GitDirectoryOpenRequest { path },
        });
    }
    if let Some(GitRepositoryPicked { path }) = &patch.repository_picked
        && !path.is_empty()
    {
        *directory = GitDirectoryNavigation::default();
        commands.trigger(UiInput {
            webview,
            payload: crate::event::GitDirectoryOpenRequest { path: path.clone() },
        });
    }
    let Some(GitWorkspaceChanged {
        path,
        branch,
        error,
    }) = &patch.workspace
    else {
        return;
    };
    if !error.is_empty() {
        states.apply_workspace_error(webview, error.clone());
        return;
    }
    if path.is_empty() || states.workspace(webview).as_deref() == Some(path) {
        return;
    }
    states.reset(webview, path.clone());
    controller.reset(branch.clone());
    pending.0.clear();
    revision.0 = 0;
    commit_result.0 = 0;
    revealed.0.clear();
    *directory = GitDirectoryNavigation::default();
    commands.trigger(UiInput {
        webview,
        payload: GitRepositoryRequest { path: path.clone() },
    });
}

fn key_request(
    trigger: On<UiInput<GitKeyRequest>>,
    mut pages: Query<(
        &GitState,
        &mut GitPageControllerState,
        &mut SelectionRevealRevision,
    )>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((state, mut controller, mut revision)) = pages.get_mut(webview) else {
        return;
    };
    let previous = controller.clone();
    dispatch_git_key(
        controller.bypass_change_detection(),
        &mut revision,
        webview,
        &trigger.event().payload,
        state,
        &mut commands,
    );
    if *controller != previous {
        controller.set_changed();
    }
}

fn panel_select_request(
    trigger: On<UiInput<GitPanelSelectRequest>>,
    mut pages: Query<&mut GitPageControllerState>,
) {
    let Ok(mut controller) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    controller.select_panel(trigger.event().payload.panel);
}

fn shortcut_help_request(
    trigger: On<UiInput<GitShortcutHelpRequest>>,
    mut pages: Query<&mut GitPageControllerState>,
) {
    let Ok(mut controller) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    controller.shortcut_help_visible = trigger.event().payload.visible;
}

fn file_select_request(
    trigger: On<UiInput<GitFileSelectRequest>>,
    mut pages: Query<(&GitState, &mut GitPageControllerState)>,
) {
    let Ok((state, mut controller)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    controller.select_file(&trigger.event().payload.path_bytes, repository);
}

fn select_branch_collection(
    trigger: On<UiInput<GitBranchCollectionSelectRequest>>,
    mut pages: Query<(&GitState, &mut GitPageControllerState)>,
) {
    let Ok((state, mut controller)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    controller.select_branch_collection(trigger.event().payload.collection, repository);
}

fn branch_select_request(
    trigger: On<UiInput<GitBranchSelectRequest>>,
    mut pages: Query<(&GitState, &mut GitPageControllerState)>,
) {
    let Ok((state, mut controller)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    controller.select_branch(&trigger.event().payload.reference, repository);
}

fn commit_select_request(
    trigger: On<UiInput<GitCommitSelectRequest>>,
    mut pages: Query<(&GitState, &mut GitPageControllerState)>,
) {
    let Ok((state, mut controller)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    controller.select_commit(&trigger.event().payload.commit, repository);
}

fn stash_select_request(
    trigger: On<UiInput<GitStashSelectRequest>>,
    mut pages: Query<(&GitState, &mut GitPageControllerState)>,
) {
    let Ok((state, mut controller)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    controller.select_stash(&trigger.event().payload.reference, repository);
}

fn discard_file_request(
    trigger: On<UiInput<GitDiscardFileRequest>>,
    mut pages: Query<(&GitState, &mut GitPageControllerState)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((state, mut controller)) = pages.get_mut(webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    if let Some(payload) = controller.discard_file(&trigger.event().payload.path_bytes, repository)
    {
        commands.trigger(UiInput { webview, payload });
    }
}

fn open_branch_prompt(
    trigger: On<UiInput<GitBranchPromptOpenRequest>>,
    mut pages: Query<&mut GitPageControllerState>,
) {
    let Ok(mut controller) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    controller.open_branch_prompt(trigger.event().payload.prompt.clone());
}

fn close_branch_prompt(
    trigger: On<UiInput<GitBranchPromptCloseRequest>>,
    mut pages: Query<&mut GitPageControllerState>,
) {
    let Ok(mut controller) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    controller.branch_prompt = None;
    controller.branch_draft.clear();
}

fn edit_branch_draft(
    trigger: On<UiInput<GitBranchDraftRequest>>,
    mut pages: Query<&mut GitPageControllerState>,
) {
    let Ok(mut controller) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    controller
        .branch_draft
        .clone_from(&trigger.event().payload.draft);
}

fn submit_branch(
    trigger: On<UiInput<GitBranchSubmitRequest>>,
    mut pages: Query<(&GitState, &mut GitPageControllerState)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((state, mut controller)) = pages.get_mut(webview) else {
        return;
    };
    let Some(prompt) = controller.branch_prompt.clone() else {
        return;
    };
    let repo_root = state.snapshot.workspace.clone();
    match prompt {
        GitBranchPrompt::Create { base } => {
            let branch = controller.branch_draft.trim().to_string();
            if branch.is_empty() {
                return;
            }
            commands.trigger(UiInput {
                webview,
                payload: GitCreateBranchRequest {
                    repo_root,
                    branch,
                    start_point: base,
                },
            });
        }
        GitBranchPrompt::Delete { branch } => {
            commands.trigger(UiInput {
                webview,
                payload: GitDeleteBranchRequest { repo_root, branch },
            });
        }
    }
    controller.branch_prompt = None;
    controller.branch_draft.clear();
}

fn edit_commit_draft(
    trigger: On<UiInput<GitCommitDraftRequest>>,
    mut pages: Query<&mut GitPageControllerState>,
) {
    let Ok(mut controller) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    controller
        .commit_message
        .clone_from(&trigger.event().payload.message);
}

fn submit_commit(
    trigger: On<UiInput<GitCommitSubmitRequest>>,
    mut pages: Query<(&GitState, &mut GitPageControllerState)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((state, mut controller)) = pages.get_mut(webview) else {
        return;
    };
    let message = controller.commit_message.trim().to_string();
    if message.is_empty() || !controller.operations.commit || !controller.commit_pending.is_empty()
    {
        return;
    }
    controller.commit_pending.clone_from(&message);
    commands.trigger(UiInput {
        webview,
        payload: GitCommitRequest {
            path: state.snapshot.workspace.clone(),
            message,
        },
    });
}
