use std::path::Path;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::UiStateWrite;
use vmux_ecs::event::space::ProjectActivateRequest;

use crate::event::{
    GitAmendRequest, GitBranchCollectionSelectRequest, GitBranchDiffSelectRequest,
    GitBranchDraftRequest, GitBranchFileSelectRequest, GitBranchPromptCloseRequest,
    GitBranchPromptOpenRequest, GitBranchSelectRequest, GitBranchSubmitRequest,
    GitCheckoutCommitRequest, GitCherryPickRequest, GitCommitDraftRequest, GitCommitRequest,
    GitCommitSelectRequest, GitCommitSubmitRequest, GitCreateBranchRequest, GitDeleteBranchRequest,
    GitDiscardFileRequest, GitDiscardRequest, GitFastForwardRequest, GitFileSelectRequest,
    GitKeyRequest, GitMergeRequest, GitPanelSelectRequest, GitRebaseRequest, GitRepositoryRequest,
    GitRepositorySnapshot, GitRevertRequest, GitShortcutHelpRequest, GitStageAllRequest,
    GitStageRequest, GitStashDropRequest, GitStashPopRequest, GitStashPushRequest,
    GitStashSelectRequest, GitUnstageRequest, HeaderDropdownRequest,
};
use crate::state::{
    GitBranchCollection, GitBranchPrompt, GitOperationEligibility, GitPageContext,
    GitPageControllerState, GitPanel, GitRepositoryPicked, GitSelectionReveal, GitWorkspaceChanged,
    HeaderDropdown, UiGit,
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
            HeaderDropdownRequest,
            GitFileSelectRequest,
            GitBranchFileSelectRequest,
            GitBranchCollectionSelectRequest,
            GitBranchSelectRequest,
            GitBranchDiffSelectRequest,
            GitCommitSelectRequest,
            GitStashSelectRequest,
            GitDiscardFileRequest,
        )>::default())
            .add_observer(ui_state_write)
            .add_observer(key_request)
            .add_observer(panel_select_request)
            .add_observer(shortcut_help_request)
            .add_observer(header_dropdown_request)
            .add_observer(file_select_request)
            .add_observer(branch_file_select_request)
            .add_observer(select_branch_collection)
            .add_observer(branch_select_request)
            .add_observer(branch_diff_select_request)
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
                (ControllerSet::Reconcile, ControllerSet::Operations)
                    .chain()
                    .after(super::GitUpdateSet::Jobs),
            )
            .add_systems(
                Update,
                (
                    (reconcile, settle_commit, sync_url)
                        .chain()
                        .in_set(ControllerSet::Reconcile),
                    operations.in_set(ControllerSet::Operations),
                ),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum ControllerSet {
    Reconcile,
    Operations,
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
        if panel != GitPanel::Commits {
            self.selected_commit.clear();
        }
    }

    fn apply_url_commit(&mut self, selected_commit: Option<String>) {
        let Some(selected_commit) = selected_commit else {
            self.selected_commit.clear();
            return;
        };
        self.selected_commit = selected_commit;
        self.focused_panel = GitPanel::Commits;
    }

    pub(super) fn selected_commit_is_object_id(&self) -> bool {
        matches!(self.selected_commit.len(), 40 | 64)
            && self
                .selected_commit
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
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
        self.select_panel(GitPanel::Files);
        self.selected_path.clone_from(&entry.path);
        self.selected_path_bytes.clone_from(&entry.path_bytes);
        self.selected_abs_path = Path::new(&repository.repo_root)
            .join(&entry.path)
            .to_string_lossy()
            .into_owned();
        true
    }

    fn select_branch_collection(&mut self, collection: GitBranchCollection) {
        self.branch_collection = collection;
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
        self.select_panel(GitPanel::Branches);
        self.selected_branch = reference.to_string();
        self.selected_branch_path.clear();
        self.selected_branch_path_bytes.clear();
        true
    }

    fn select_branch_file(
        &mut self,
        path_bytes: &[u8],
        files: &[crate::event::GitBranchFileEntry],
    ) -> bool {
        let Some(entry) = files.iter().find(|entry| entry.path_bytes == path_bytes) else {
            return false;
        };
        self.select_panel(GitPanel::Branches);
        self.selected_branch_path.clone_from(&entry.path);
        self.selected_branch_path_bytes
            .clone_from(&entry.path_bytes);
        true
    }

    fn select_commit(&mut self, commit: &str, repository: &GitRepositorySnapshot) -> bool {
        if !repository.commits.iter().any(|entry| entry.sha == commit) {
            return false;
        }
        self.select_panel(GitPanel::Commits);
        self.selected_commit = commit.to_string();
        true
    }

    fn select_branch_diff(&mut self, repository: &GitRepositorySnapshot) {
        self.branch_collection = GitBranchCollection::Local;
        self.selected_branch.clone_from(&repository.branch);
        self.selected_branch_path.clear();
        self.selected_branch_path_bytes.clear();
        self.select_panel(GitPanel::Branches);
    }

    fn select_stash(&mut self, reference: &str, repository: &GitRepositorySnapshot) -> bool {
        if !repository
            .stashes
            .iter()
            .any(|entry| entry.reference == reference)
        {
            return false;
        }
        self.select_panel(GitPanel::Stash);
        self.selected_stash = reference.to_string();
        true
    }

    fn move_selection(
        &mut self,
        direction: SelectionDirection,
        state: &GitState,
        revision: &mut SelectionRevealRevision,
    ) -> Option<GitSelectionReveal> {
        let repository = state.repository()?;
        let id = match self.focused_panel {
            GitPanel::Files => self.move_file_selection(direction, repository),
            GitPanel::Branches => {
                self.move_branch_file_selection(direction, &state.snapshot.branch_files)
            }
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

    fn move_branch_file_selection(
        &mut self,
        direction: SelectionDirection,
        files: &[crate::event::GitBranchFileEntry],
    ) -> Option<String> {
        let len = files.len();
        if len == 0 {
            return None;
        }
        let current = files
            .iter()
            .position(|entry| entry.path_bytes == self.selected_branch_path_bytes)
            .unwrap_or(direction.fallback(len));
        let index = direction.index(current, len);
        self.selected_branch_path.clone_from(&files[index].path);
        self.selected_branch_path_bytes
            .clone_from(&files[index].path_bytes);
        Some(files[index].row_id())
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
        if let Some(effect) = controller.move_selection(direction, state, revision) {
            commands.trigger(UiStateWrite::<UiGit>::from_event(webview, &effect));
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
        if state.snapshot.commits.loaded
            && !state.snapshot.commits.has_more
            && !repository
                .commits
                .iter()
                .any(|entry| entry.sha == next.selected_commit)
        {
            next.selected_commit.clear();
        }
        next.selected_branch = next
            .branch_collection
            .selected_reference(repository, &next.selected_branch);
        let next_branch_file = if state.snapshot.branch_files_reference == next.selected_branch {
            state
                .snapshot
                .branch_files
                .iter()
                .find(|entry| entry.path_bytes == next.selected_branch_path_bytes)
                .or_else(|| state.snapshot.branch_files.first())
        } else {
            None
        };
        next.selected_branch_path = next_branch_file
            .map(|entry| entry.path.clone())
            .unwrap_or_default();
        next.selected_branch_path_bytes = next_branch_file
            .map(|entry| entry.path_bytes.clone())
            .unwrap_or_default();
        if state.snapshot.stashes.loaded {
            next.selected_stash = repository
                .stashes
                .iter()
                .find(|entry| entry.reference == next.selected_stash)
                .or_else(|| repository.stashes.first())
                .map(|entry| entry.reference.clone())
                .unwrap_or_default();
        }
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
        let branch = if controller.branch_collection == GitBranchCollection::Local
            && state.snapshot.local_branches.loaded
        {
            repository
                .branches
                .iter()
                .find(|branch| branch.name == controller.selected_branch)
        } else {
            None
        };
        let commit = state.snapshot.commits.loaded
            && repository
                .commits
                .iter()
                .any(|entry| entry.sha == controller.selected_commit);
        let stash = state.snapshot.stashes.loaded
            && repository
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

fn sync_url(
    mut pages: Query<
        (
            &GitState,
            &GitPageControllerState,
            &mut vmux_ecs::PageMetadata,
        ),
        Changed<GitPageControllerState>,
    >,
) {
    for (state, controller, mut page) in &mut pages {
        if !crate::GitUrl::is_page(&page.url) {
            continue;
        }
        let Some(repository) = state.repository() else {
            continue;
        };
        let commit = if controller.focused_panel == GitPanel::Commits {
            Some(controller.selected_commit.as_str())
        } else {
            None
        };
        let Some(url) = crate::GitUrl::from_path_with_commit(
            Path::new(&repository.repo_root),
            commit.filter(|commit| !commit.is_empty()),
        ) else {
            continue;
        };
        if page.url != url {
            page.url = url;
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

fn ui_state_write(
    trigger: On<UiStateWrite<UiGit>>,
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
        commands.entity(webview).insert(GitState::default());
        commands.trigger(trigger.event().clone());
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
        let selected_commit = crate::GitUrl::commit(page_url);
        if !states.repository_loaded_at(webview, &path) {
            states.reset(webview, path.clone());
            controller.reset(String::new());
            controller.apply_url_commit(selected_commit);
            pending.0.clear();
            revision.0 = 0;
            commit_result.0 = 0;
            revealed.0.clear();
            *directory = GitDirectoryNavigation::default();
            commands.trigger(UiInput {
                webview,
                payload: crate::event::GitDirectoryOpenRequest { path },
            });
        } else {
            controller.apply_url_commit(selected_commit);
        }
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
    if path.is_empty()
        || states.repository_loaded_at(webview, path)
        || states.repository_loading_at(webview, path)
    {
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

fn header_dropdown_request(
    trigger: On<UiInput<HeaderDropdownRequest>>,
    mut pages: Query<&mut HeaderDropdown>,
) {
    let Ok(mut dropdown) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    dropdown.set_if_neq(trigger.event().payload.open);
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

fn branch_file_select_request(
    trigger: On<UiInput<GitBranchFileSelectRequest>>,
    mut pages: Query<(&GitState, &mut GitPageControllerState)>,
) {
    let Ok((state, mut controller)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    controller.select_branch_file(
        &trigger.event().payload.path_bytes,
        &state.snapshot.branch_files,
    );
}

fn select_branch_collection(
    trigger: On<UiInput<GitBranchCollectionSelectRequest>>,
    mut pages: Query<&mut GitPageControllerState>,
) {
    let Ok(mut controller) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    controller.select_branch_collection(trigger.event().payload.collection);
}

fn branch_select_request(
    trigger: On<UiInput<GitBranchSelectRequest>>,
    mut pages: Query<(&GitState, &mut GitPageControllerState)>,
    mut commands: Commands,
) {
    let Ok((state, mut controller)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    let reference = &trigger.event().payload.reference;
    let activate = controller.branch_collection == GitBranchCollection::Local;
    if !controller.select_branch(reference, repository) || !activate {
        return;
    }
    let Some(branch) = repository
        .branches
        .iter()
        .find(|branch| branch.name == *reference)
    else {
        return;
    };
    commands.trigger(UiInput {
        webview: trigger.event().webview,
        payload: ProjectActivateRequest {
            path: repository.repo_root.clone(),
            branch: branch.name.clone(),
            checkout: branch.checkout.clone(),
            pane_id: None,
        },
    });
}

fn branch_diff_select_request(
    trigger: On<UiInput<GitBranchDiffSelectRequest>>,
    mut pages: Query<(&GitState, &mut GitPageControllerState)>,
) {
    let Ok((state, mut controller)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    controller.select_branch_diff(repository);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct ProjectActivations(Vec<ProjectActivateRequest>);

    fn capture_project_activation(
        trigger: On<UiInput<ProjectActivateRequest>>,
        mut activations: ResMut<ProjectActivations>,
    ) {
        activations.0.push(trigger.event().payload.clone());
    }

    #[test]
    fn url_commit_selects_a_commit_without_locking_other_panels() {
        let mut controller = GitPageControllerState::default();

        controller.apply_url_commit(Some("abc123".to_string()));
        assert_eq!(controller.focused_panel, GitPanel::Commits);
        assert_eq!(controller.selected_commit, "abc123");

        controller.select_panel(GitPanel::Files);
        controller.apply_url_commit(None);
        assert_eq!(controller.focused_panel, GitPanel::Files);
        assert!(controller.selected_commit.is_empty());
    }

    #[test]
    fn automatic_commit_lookup_requires_a_complete_object_id() {
        let mut controller = GitPageControllerState::default();

        controller.apply_url_commit(Some("abc123".to_string()));
        assert!(!controller.selected_commit_is_object_id());

        controller.apply_url_commit(Some("a".repeat(40)));
        assert!(controller.selected_commit_is_object_id());

        controller.apply_url_commit(Some("g".repeat(40)));
        assert!(!controller.selected_commit_is_object_id());
    }

    #[test]
    fn browsing_branch_collections_preserves_the_selected_reference() {
        let mut controller = GitPageControllerState {
            selected_branch: "vmx-226-session-management".to_string(),
            selected_branch_path: "crates/feature/vmux_session/src/ui.rs".to_string(),
            selected_branch_path_bytes: b"crates/feature/vmux_session/src/ui.rs".to_vec(),
            ..Default::default()
        };

        controller.select_branch_collection(GitBranchCollection::Tags);

        assert_eq!(controller.branch_collection, GitBranchCollection::Tags);
        assert_eq!(controller.selected_branch, "vmx-226-session-management");
        assert_eq!(
            controller.selected_branch_path,
            "crates/feature/vmux_session/src/ui.rs"
        );
    }

    #[test]
    fn selecting_a_local_branch_activates_its_workspace() {
        let mut app = App::new();
        app.init_resource::<ProjectActivations>()
            .add_observer(branch_select_request)
            .add_observer(capture_project_activation);
        let page = app
            .world_mut()
            .spawn((
                GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        repository: Some(GitRepositorySnapshot {
                            path: "/repo".to_string(),
                            repo_root: "/repo".to_string(),
                            repo_name: "repo".to_string(),
                            is_worktree: false,
                            branch: "main".to_string(),
                            base_ref: "main".to_string(),
                            upstream: String::new(),
                            ahead: 0,
                            behind: 0,
                            files: Vec::new(),
                            commits: Vec::new(),
                            branches: vec![crate::event::GitBranchEntry {
                                name: "feature".to_string(),
                                current: false,
                                upstream: String::new(),
                                checkout: String::new(),
                                short_sha: "abc123".to_string(),
                                ahead: 0,
                                behind: 0,
                                changed_files: 0,
                                insertions: 0,
                                deletions: 0,
                            }],
                            remote_branches: Vec::new(),
                            tags: Vec::new(),
                            stashes: Vec::new(),
                        }),
                        ..Default::default()
                    },
                },
                GitPageControllerState::default(),
            ))
            .id();

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GitBranchSelectRequest {
                reference: "feature".to_string(),
            },
        });
        app.update();

        assert_eq!(
            app.world().resource::<ProjectActivations>().0,
            [ProjectActivateRequest {
                path: "/repo".to_string(),
                branch: "feature".to_string(),
                checkout: String::new(),
                pane_id: None,
            }]
        );
    }

    #[test]
    fn header_dropdown_selection_keeps_only_one_open() {
        let mut app = App::new();
        app.add_plugins(ControllerPlugin);
        let page = app.world_mut().spawn(HeaderDropdown::Repository).id();

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: HeaderDropdownRequest {
                open: HeaderDropdown::Branch,
            },
        });
        app.update();

        assert_eq!(
            app.world().get::<HeaderDropdown>(page),
            Some(&HeaderDropdown::Branch)
        );
    }

    #[test]
    fn repository_summary_preserves_url_commit_until_catalog_loads() {
        let mut app = App::new();
        app.add_systems(Update, reconcile);
        let entity = app
            .world_mut()
            .spawn((
                GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        repository: Some(GitRepositorySnapshot {
                            path: "/tmp/repository".to_string(),
                            repo_root: "/tmp/repository".to_string(),
                            repo_name: "repository".to_string(),
                            is_worktree: false,
                            branch: "main".to_string(),
                            base_ref: String::new(),
                            upstream: String::new(),
                            ahead: 0,
                            behind: 0,
                            files: Vec::new(),
                            commits: Vec::new(),
                            branches: Vec::new(),
                            remote_branches: Vec::new(),
                            tags: Vec::new(),
                            stashes: Vec::new(),
                        }),
                        ..Default::default()
                    },
                },
                GitPageControllerState {
                    selected_commit: "abc123".to_string(),
                    focused_panel: GitPanel::Commits,
                    ..Default::default()
                },
            ))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .get::<GitPageControllerState>(entity)
                .unwrap()
                .selected_commit,
            "abc123"
        );
    }

    #[test]
    fn context_received_before_page_state_is_replayed_after_initialization() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ControllerPlugin));
        let webview = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(UiStateWrite::<UiGit>::from_event(
            webview,
            &GitPageContext {
                working_directory: "/tmp/repository".to_string(),
                page_url: String::new(),
            },
        ));
        app.update();

        let state = app.world().get::<GitState>(webview).unwrap();
        assert_eq!(state.workspace(), "/tmp/repository");
    }

    #[test]
    fn repository_state_does_not_turn_a_project_page_into_git() {
        let mut app = App::new();
        app.add_systems(Update, sync_url);
        let page = app
            .world_mut()
            .spawn((
                GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        repository: Some(GitRepositorySnapshot {
                            path: "/tmp/repository".to_string(),
                            repo_root: "/tmp/repository".to_string(),
                            repo_name: "repository".to_string(),
                            is_worktree: false,
                            branch: "main".to_string(),
                            base_ref: String::new(),
                            upstream: String::new(),
                            ahead: 0,
                            behind: 0,
                            files: Vec::new(),
                            commits: Vec::new(),
                            branches: Vec::new(),
                            remote_branches: Vec::new(),
                            tags: Vec::new(),
                            stashes: Vec::new(),
                        }),
                        ..Default::default()
                    },
                },
                GitPageControllerState::default(),
                vmux_ecs::PageMetadata {
                    url: "vmux://projects/".to_string(),
                    ..Default::default()
                },
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<vmux_ecs::PageMetadata>(page).unwrap().url,
            "vmux://projects/"
        );
    }
}
