use std::path::Path;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::winit::EventLoopProxyWrapper;
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
    GitStashSelectRequest, GitUnstageRequest,
};
use crate::state::{
    GitBranchCollection, GitBranchPrompt, GitOperationEligibility, GitPageContext,
    GitPageControllerState, GitPanel, GitRepositoryPicked, GitSelectionReveal, GitUiState,
    GitWorkspaceChanged,
};

use super::directory::GitDirectoryNavigation;
use super::runtime::{GitRuntime, GitTask};
use super::state::{CachedGitPage, GitDiffRevealRanges, GitPageCache, GitState};

pub(super) struct ControllerPlugin;

impl Plugin for ControllerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GitRuntime>()
            .add_plugins(UiEventPlugin::<(
                GitKeyRequest,
                GitPanelSelectRequest,
                GitShortcutHelpRequest,
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
            .add_observer(reset_location)
            .add_observer(key_request)
            .add_observer(panel_select_request)
            .add_observer(shortcut_help_request)
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
                    (
                        start_repository_probes,
                        bevy::ecs::schedule::ApplyDeferred,
                        poll_repository_probes,
                    )
                        .chain()
                        .before(ControllerSet::Reconcile),
                    (
                        reconcile,
                        apply_location,
                        select_default,
                        settle_commit,
                        sync_url,
                    )
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

#[derive(Component, Default)]
pub(super) struct LocationCursor {
    document: String,
    branch: String,
    fragment: String,
    applied: bool,
}

impl LocationCursor {
    fn apply(&mut self, url: &str, state: &GitState, controller: &mut GitPageControllerState) {
        let fragment = vmux_api::page_location::PageLocationUiState::from_url(url).fragment;
        let document = url.split('#').next().unwrap_or(url);
        if self.fragment != fragment
            || self.document != document
            || self.branch != controller.selected_branch
        {
            self.document = document.to_string();
            self.branch.clone_from(&controller.selected_branch);
            self.fragment = fragment;
            self.applied = false;
        }
        if self.applied {
            return;
        }
        if self.fragment.is_empty() || !self.fragment.starts_with("diff-") {
            self.applied = true;
            return;
        }
        if controller.focused_panel != GitPanel::Branches
            || state.snapshot.branch_files_reference != controller.selected_branch
            || controller.selected_branch.is_empty()
        {
            return;
        }
        let mut path = None;
        for entry in &state.snapshot.branch_files {
            if crate::GitUrl::file_hash(&entry.path_bytes) == self.fragment {
                path = Some(entry.path_bytes.clone());
                break;
            }
        }
        if path.is_none() {
            for candidate in &state.snapshot.branch_diff_paths {
                if crate::GitUrl::file_hash(candidate.as_bytes()) == self.fragment {
                    path = Some(candidate.as_bytes().to_vec());
                    break;
                }
            }
        }
        if let Some(path) = path {
            if controller.selected_branch_path_bytes != path {
                controller.select_branch_file(
                    &path,
                    &state.snapshot.branch_files,
                    &state.snapshot.branch_diff_paths,
                );
            }
            self.applied = true;
        } else if !state.snapshot.branch_files_loading {
            self.applied = true;
        }
    }
}

fn apply_location(
    mut pages: Query<(
        &vmux_ecs::PageMetadata,
        &GitState,
        &mut LocationCursor,
        &mut GitPageControllerState,
    )>,
) {
    for (metadata, state, mut cursor, mut controller) in &mut pages {
        let fragment = metadata
            .url
            .split_once('#')
            .map(|(_, fragment)| fragment)
            .unwrap_or_default();
        if cursor.applied
            && cursor.fragment == fragment
            && cursor.document == metadata.url.split('#').next().unwrap_or(&metadata.url)
            && cursor.branch == controller.selected_branch
        {
            continue;
        }
        cursor.apply(&metadata.url, state, &mut controller);
    }
}

fn reset_location(event: On<Add, GitState>, mut commands: Commands) {
    commands
        .entity(event.event_target())
        .insert(LocationCursor::default());
}

#[derive(Component)]
struct PendingRepositoryProbe {
    path: String,
}

#[derive(Component)]
struct RepositoryProbeTask {
    path: String,
    task: GitTask<bool>,
}

impl GitCommitResultSequence {
    pub(super) fn reset(&mut self) {
        self.0 = 0;
    }
}

impl GitPageControllerState {
    fn apply_url(&mut self, value: &str) -> bool {
        let Some(panel) = crate::GitUrl::panel(value) else {
            return false;
        };
        let previous = (
            self.focused_panel,
            self.selected_branch.clone(),
            self.selected_commit.clone(),
            self.selected_stash.clone(),
            self.selected_abs_path.clone(),
        );
        self.select_panel(panel);
        let selection = crate::GitUrl::selection(value).unwrap_or_default();
        match panel {
            GitPanel::Branches => {
                if !selection.is_empty() && self.selected_branch != selection {
                    self.selected_branch = selection;
                    self.selected_branch_path.clear();
                    self.selected_branch_path_bytes.clear();
                }
            }
            GitPanel::Commits => {
                self.apply_url_commit((!selection.is_empty()).then_some(selection))
            }
            GitPanel::Stash => self.selected_stash = selection,
            GitPanel::Files => {}
        }
        previous
            != (
                self.focused_panel,
                self.selected_branch.clone(),
                self.selected_commit.clone(),
                self.selected_stash.clone(),
                self.selected_abs_path.clone(),
            )
    }

    fn select_panel(&mut self, panel: GitPanel) {
        self.focused_panel = panel;
        if panel != GitPanel::Commits {
            self.selected_commit.clear();
        }
    }

    pub(super) fn apply_url_commit(&mut self, selected_commit: Option<String>) {
        let Some(selected_commit) = selected_commit.filter(|commit| Self::is_object_id(commit))
        else {
            self.selected_commit.clear();
            return;
        };
        self.selected_commit = selected_commit;
        self.focused_panel = GitPanel::Commits;
    }

    pub(super) fn selected_commit_is_object_id(&self) -> bool {
        Self::is_object_id(&self.selected_commit)
    }

    fn is_object_id(value: &str) -> bool {
        matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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
        if !self.branch_collection.contains(repository, reference) {
            return false;
        }
        self.select_panel(GitPanel::Branches);
        self.selected_branch = reference.to_string();
        self.selected_branch_collection = self.branch_collection;
        self.selected_branch_path.clear();
        self.selected_branch_path_bytes.clear();
        true
    }

    fn select_branch_file(
        &mut self,
        path_bytes: &[u8],
        files: &[crate::event::GitBranchFileEntry],
        expected_paths: &[String],
    ) -> bool {
        let path = match files.iter().find(|entry| entry.path_bytes == path_bytes) {
            Some(entry) => entry.path.clone(),
            None => {
                let Some(path) = expected_paths
                    .iter()
                    .find(|path| path.as_bytes() == path_bytes)
                else {
                    return false;
                };
                path.clone()
            }
        };
        self.select_panel(GitPanel::Branches);
        self.selected_branch_path = path;
        self.selected_branch_path_bytes = path_bytes.to_vec();
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

    fn default_commit<'a>(&self, repository: &'a GitRepositorySnapshot) -> Option<&'a str> {
        if self.focused_panel != GitPanel::Commits || !self.selected_commit.is_empty() {
            return None;
        }
        Some(repository.commits.first()?.sha.as_str())
    }

    fn select_branch_diff(&mut self, repository: &GitRepositorySnapshot) {
        self.branch_collection = GitBranchCollection::Local;
        self.selected_branch_collection = GitBranchCollection::Local;
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

type OperationPage<'a> = (
    &'a GitState,
    Ref<'a, super::state::GitRepositoryRevision>,
    &'a mut GitPageControllerState,
);
type ChangedOperationPage = Or<(
    Changed<GitState>,
    Changed<super::state::GitRepositoryRevision>,
    Changed<GitPageControllerState>,
)>;

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
        (GitPanel::Branches, "c") if controller.operations.checkout_branch => {
            let Some(checkout) = controller
                .selected_branch_collection
                .checkout(repository, &controller.selected_branch)
            else {
                return;
            };
            if checkout.start_point.is_empty() {
                commands.trigger(UiInput {
                    webview,
                    payload: ProjectActivateRequest {
                        path: repository.repo_root.clone(),
                        branch: checkout.branch,
                        checkout: checkout.checkout,
                        pane_id: None,
                    },
                });
            } else {
                commands.trigger(UiInput {
                    webview,
                    payload: GitCreateBranchRequest {
                        repo_root: repository.repo_root.clone(),
                        branch: checkout.branch,
                        start_point: checkout.start_point,
                    },
                });
            }
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

type Sources<'w, 's> = Query<
    'w,
    's,
    (
        &'static GitState,
        Ref<'static, super::state::GitRepositoryRevision>,
        &'static mut GitPageControllerState,
    ),
    Or<(
        Changed<GitState>,
        Changed<super::state::GitRepositoryRevision>,
    )>,
>;

fn reconcile(mut pages: Sources) {
    for (state, _, mut controller) in &mut pages {
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
            && !next.selected_commit_is_object_id()
            && !repository
                .commits
                .iter()
                .any(|entry| entry.sha == next.selected_commit)
        {
            next.selected_commit.clear();
        }
        if !next
            .selected_branch_collection
            .contains(repository, &next.selected_branch)
        {
            match GitBranchCollection::containing(repository, &next.selected_branch) {
                Some(collection) => next.selected_branch_collection = collection,
                None if next.selected_branch.is_empty() || state.snapshot.references_complete() => {
                    next.selected_branch = GitBranchCollection::Local
                        .selected_reference(repository, &next.selected_branch);
                    next.selected_branch_collection = GitBranchCollection::Local;
                }
                None => {}
            }
        }
        let retained_branch_path = state.snapshot.branch_files_reference == next.selected_branch
            && state
                .snapshot
                .branch_diff_paths
                .iter()
                .any(|path| path.as_bytes() == next.selected_branch_path_bytes);
        let mut next_branch_file = if state.snapshot.branch_files_reference == next.selected_branch
        {
            state
                .snapshot
                .branch_files
                .iter()
                .find(|entry| entry.path_bytes == next.selected_branch_path_bytes)
        } else {
            None
        };
        if next_branch_file.is_none()
            && !retained_branch_path
            && state.snapshot.branch_files_reference == next.selected_branch
        {
            next_branch_file = state.snapshot.branch_files.first();
        }
        if let Some(entry) = next_branch_file {
            next.selected_branch_path.clone_from(&entry.path);
            next.selected_branch_path_bytes
                .clone_from(&entry.path_bytes);
        } else if !retained_branch_path {
            next.selected_branch_path.clear();
            next.selected_branch_path_bytes.clear();
        }
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

type DefaultCommitPages<'w, 's> = Query<
    'w,
    's,
    (&'static GitState, &'static mut GitPageControllerState),
    Or<(
        Changed<GitState>,
        Changed<super::state::GitRepositoryRevision>,
        Changed<super::state::GitCatalogRevision>,
        Changed<GitPageControllerState>,
    )>,
>;

fn select_default(mut pages: DefaultCommitPages) {
    for (state, mut controller) in &mut pages {
        if state.snapshot.loading || !state.snapshot.commits.loaded {
            continue;
        }
        let Some(repository) = state.repository() else {
            continue;
        };
        let Some(commit) = controller.default_commit(repository) else {
            continue;
        };
        controller.select_commit(commit, repository);
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
    for (state, _, mut controller) in &mut pages.values {
        if state.snapshot.loading {
            if controller.operations != GitOperationEligibility::default() {
                controller.operations = GitOperationEligibility::default();
            }
            continue;
        }
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
        let branch = if controller.selected_branch_collection == GitBranchCollection::Local
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
        let checkout = controller
            .selected_branch_collection
            .checkout(repository, &controller.selected_branch);
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
            checkout_branch: checkout.is_some_and(|checkout| !checkout.current),
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
        let selection = match controller.focused_panel {
            GitPanel::Branches => Some(controller.selected_branch.as_str()),
            GitPanel::Commits => Some(controller.selected_commit.as_str()),
            GitPanel::Stash => Some(controller.selected_stash.as_str()),
            GitPanel::Files => None,
        };
        let Some(url) = crate::GitUrl::from_view(
            Path::new(&repository.repo_root),
            controller.focused_panel,
            selection,
        ) else {
            continue;
        };
        let url = vmux_api::page_location::PageLocationUiState::from_url(&page.url).url(&url);
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
    mut cache: Option<ResMut<GitPageCache>>,
    mut watch: Option<NonSendMut<super::watch::GitWatch>>,
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
        let path = working_directory.clone();
        if !states.repository_loaded_at(webview, &path)
            && !states.repository_loading_at(webview, &path)
        {
            let mut restored = false;
            if let Some(cached) = cache.as_deref_mut().and_then(|cache| cache.restore(&path)) {
                let CachedGitPage {
                    mut snapshot,
                    controller: mut cached_controller,
                    watch: cached_watch,
                    diff,
                    branch_files,
                } = cached;
                let changed_selection = cached_controller.apply_url(page_url);
                if changed_selection {
                    snapshot.clear_diff();
                }
                if states.restore(webview, snapshot) {
                    *controller = cached_controller;
                    if let Some(diff) = diff.filter(|_| !changed_selection) {
                        commands.entity(webview).insert(diff);
                    }
                    if let Some(branch_files) = branch_files.filter(|_| !changed_selection) {
                        commands.entity(webview).insert(branch_files);
                    }
                    if let Some((workspace, refresh_revision)) = states.refresh_repository(webview)
                    {
                        if let (Some(watch), Some(subscription)) =
                            (watch.as_deref_mut(), cached_watch)
                        {
                            let _ =
                                watch.subscribe_page(webview, Path::new(&workspace), subscription);
                        }
                        commands.spawn((
                            super::job_runner::GitJob::new(webview),
                            super::job::GitForegroundRead,
                            super::job::RepositoryJob {
                                path: workspace.into(),
                                refresh_catalogs: true,
                                revision: refresh_revision,
                            },
                        ));
                    }
                    restored = true;
                }
            }
            if restored {
                commands
                    .entity(webview)
                    .remove::<(PendingRepositoryProbe, RepositoryProbeTask)>();
                controller.apply_url(page_url);
                pending.0.clear();
                revision.0 = 0;
                commit_result.0 = 0;
                revealed.0.clear();
                *directory = GitDirectoryNavigation::default();
            }
            if !restored {
                states.reset(webview, path.clone());
                controller.reset_for_repository(String::new());
                controller.apply_url(page_url);
                pending.0.clear();
                revision.0 = 0;
                commit_result.0 = 0;
                revealed.0.clear();
                *directory = GitDirectoryNavigation::default();
                commands
                    .entity(webview)
                    .remove::<RepositoryProbeTask>()
                    .insert(PendingRepositoryProbe { path });
            }
        } else if controller.apply_url(page_url) {
            states.clear_diff(webview);
            commands
                .entity(webview)
                .remove::<(super::diff::GitDiffQuery, super::diff::GitBranchFilesQuery)>();
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
    controller.reset_for_repository(branch.clone());
    pending.0.clear();
    revision.0 = 0;
    commit_result.0 = 0;
    revealed.0.clear();
    *directory = GitDirectoryNavigation::default();
    commands.trigger(UiInput {
        webview,
        payload: GitRepositoryRequest::project(path.clone(), String::new(), branch.clone()),
    });
}

fn start_repository_probes(
    pending: Query<(Entity, &PendingRepositoryProbe), Without<RepositoryProbeTask>>,
    runtime: Res<GitRuntime>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (entity, pending) in &pending {
        let path = pending.path.clone();
        let probe_path = path.clone();
        let Some(task) = runtime.spawn(wake.clone(), move || {
            super::repository::GitRepository::has_repository(Path::new(&probe_path))
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<PendingRepositoryProbe>()
            .insert(RepositoryProbeTask { path, task });
    }
}

fn poll_repository_probes(
    mut probes: Query<(Entity, &mut RepositoryProbeTask, &GitPageControllerState)>,
    states: Query<&GitState>,
    mut commands: Commands,
) {
    for (entity, mut probe, controller) in &mut probes {
        let Some(result) = probe.task.poll() else {
            continue;
        };
        commands.entity(entity).remove::<RepositoryProbeTask>();
        let Ok(has_repository) = result else {
            continue;
        };
        let Ok(state) = states.get(entity) else {
            continue;
        };
        if state.workspace() != probe.path {
            continue;
        }
        if has_repository {
            let selected_commit = controller
                .selected_commit_is_object_id()
                .then(|| controller.selected_commit.clone());
            commands.trigger(UiInput {
                webview: entity,
                payload: GitRepositoryRequest::at(probe.path.clone()).with_commit(selected_commit),
            });
        } else {
            commands.trigger(UiInput {
                webview: entity,
                payload: crate::event::GitDirectoryOpenRequest {
                    path: probe.path.clone(),
                },
            });
        }
    }
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
        &state.snapshot.branch_diff_paths,
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
) {
    let Ok((state, mut controller)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    let reference = &trigger.event().payload.reference;
    controller.select_branch(reference, repository);
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

    #[test]
    fn a_location_waits_for_the_catalog_and_applies_only_once() {
        let mut cursor = LocationCursor::default();
        let mut state = GitState::default();
        let mut controller = GitPageControllerState {
            focused_panel: GitPanel::Branches,
            selected_branch: "feature".to_string(),
            ..Default::default()
        };
        let url = format!("git://repository#{}", crate::GitUrl::file_hash(b"late.rs"));
        cursor.apply(&url, &state, &mut controller);
        assert!(controller.selected_branch_path_bytes.is_empty());
        state.snapshot.branch_files_reference = "feature".to_string();
        state.snapshot.branch_diff_paths = vec!["late.rs".to_string(), "manual.rs".to_string()];
        cursor.apply(&url, &state, &mut controller);
        assert_eq!(controller.selected_branch_path_bytes, b"late.rs");
        controller.select_branch_file(b"manual.rs", &[], &state.snapshot.branch_diff_paths);
        cursor.apply(&url, &state, &mut controller);
        assert_eq!(controller.selected_branch_path_bytes, b"manual.rs");
        cursor.apply("git://repository", &state, &mut controller);
        cursor.apply(&url, &state, &mut controller);
        assert_eq!(controller.selected_branch_path_bytes, b"late.rs");
    }

    #[derive(Resource, Default)]
    struct InitialRequests {
        repositories: Vec<GitRepositoryRequest>,
        directories: Vec<String>,
    }

    impl InitialRequests {
        fn settle(app: &mut App) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                app.update();
                let requests = app.world().resource::<Self>();
                if !requests.repositories.is_empty() || !requests.directories.is_empty() {
                    return;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
        }

        fn repository(trigger: On<UiInput<GitRepositoryRequest>>, mut requests: ResMut<Self>) {
            requests.repositories.push(trigger.event().payload.clone());
        }

        fn directory(
            trigger: On<UiInput<crate::event::GitDirectoryOpenRequest>>,
            mut requests: ResMut<Self>,
        ) {
            requests
                .directories
                .push(trigger.event().payload.path.clone());
        }
    }

    struct CommitSelectionFixture {
        app: App,
        page: Entity,
    }

    impl CommitSelectionFixture {
        fn new(panel: GitPanel, selected: Option<&str>) -> Self {
            let path = "/tmp/commit-selection";
            let request = GitRepositoryRequest::at(path);
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ControllerPlugin));
            let page = app
                .world_mut()
                .spawn((
                    GitState {
                        snapshot: crate::state::GitPageSnapshot {
                            workspace: path.to_string(),
                            repository: Some(GitRepositorySnapshot {
                                path: path.to_string(),
                                repo_root: path.to_string(),
                                branch: "main".to_string(),
                                ..GitRepositorySnapshot::loading(&request)
                            }),
                            loading: false,
                            ..Default::default()
                        },
                    },
                    GitPageControllerState {
                        focused_panel: panel,
                        selected_commit: selected.unwrap_or_default().to_string(),
                        ..Default::default()
                    },
                    vmux_ecs::PageMetadata {
                        url: crate::GitUrl::from_view(Path::new(path), panel, selected).unwrap(),
                        ..Default::default()
                    },
                ))
                .id();
            for _ in 0..3 {
                app.update();
            }
            Self { app, page }
        }

        fn catalog(&mut self, shas: &[&str], loaded: bool) {
            let mut commits = Vec::new();
            for sha in shas {
                commits.push(crate::event::GitCommitEntry {
                    sha: sha.to_string(),
                    short_sha: sha.chars().take(7).collect(),
                    summary: "commit".to_string(),
                    author: "author".to_string(),
                    date: "2026-11-08".to_string(),
                    body: String::new(),
                    references: String::new(),
                });
            }
            let mut state = self.app.world_mut().get_mut::<GitState>(self.page).unwrap();
            let snapshot = &mut state.bypass_change_detection().snapshot;
            snapshot.repository.as_mut().unwrap().commits = commits;
            snapshot.commits.loaded = loaded;
            self.app
                .world_mut()
                .get_mut::<super::super::state::GitCatalogRevision>(self.page)
                .unwrap()
                .advance();
            self.app.update();
        }

        fn select_panel(&mut self, panel: GitPanel) {
            self.app.world_mut().trigger(UiInput {
                webview: self.page,
                payload: GitPanelSelectRequest { panel },
            });
            self.app.update();
        }

        fn assert_commit(&self, selected: &str) {
            let controller = self
                .app
                .world()
                .get::<GitPageControllerState>(self.page)
                .unwrap();
            assert_eq!(controller.selected_commit, selected);
            let metadata = self
                .app
                .world()
                .get::<vmux_ecs::PageMetadata>(self.page)
                .unwrap();
            let parsed = url::Url::parse(&metadata.url).unwrap();
            assert_eq!(
                parsed
                    .query_pairs()
                    .find(|(key, _)| key == "commit")
                    .map(|(_, value)| value.into_owned()),
                (!selected.is_empty()).then(|| selected.to_string())
            );
        }
    }

    #[test]
    fn commit_catalog_arrival_selects_latest_and_updates_the_url() {
        let old = "a".repeat(40);
        let latest = "b".repeat(40);
        let mut fixture = CommitSelectionFixture::new(GitPanel::Commits, None);
        fixture.catalog(&[&old], false);
        fixture.assert_commit("");
        fixture.catalog(&[&latest, &old], true);
        fixture.assert_commit(&latest);
    }

    #[test]
    fn switching_to_commits_selects_latest_from_an_existing_catalog() {
        let latest = "a".repeat(40);
        let old = "b".repeat(40);
        let mut fixture = CommitSelectionFixture::new(GitPanel::Files, None);
        fixture.catalog(&[&latest, &old], true);
        fixture.assert_commit("");
        fixture.select_panel(GitPanel::Commits);
        fixture.assert_commit(&latest);
    }

    #[test]
    fn explicit_commit_outside_head_history_survives_catalog_completion() {
        let head = "a".repeat(40);
        let selected = "b".repeat(40);
        let mut fixture = CommitSelectionFixture::new(GitPanel::Commits, Some(&selected));
        fixture.catalog(&[&head], true);
        fixture.assert_commit(&selected);
        fixture
            .app
            .world_mut()
            .get_mut::<GitState>(fixture.page)
            .unwrap()
            .set_changed();
        fixture.app.update();
        fixture.assert_commit(&selected);
    }

    #[test]
    fn url_commit_selects_a_commit_without_locking_other_panels() {
        let mut controller = GitPageControllerState::default();
        let commit = "a".repeat(40);

        controller.apply_url_commit(Some(commit.clone()));
        assert_eq!(controller.focused_panel, GitPanel::Commits);
        assert_eq!(controller.selected_commit, commit);

        controller.select_panel(GitPanel::Files);
        controller.apply_url_commit(None);
        assert_eq!(controller.focused_panel, GitPanel::Files);
        assert!(controller.selected_commit.is_empty());
    }

    #[test]
    fn automatic_commit_lookup_requires_a_complete_object_id() {
        let mut controller = GitPageControllerState::default();

        controller.apply_url_commit(Some("abc123".to_string()));
        assert!(controller.selected_commit.is_empty());

        controller.apply_url_commit(Some("a".repeat(40)));
        assert!(controller.selected_commit_is_object_id());

        controller.apply_url_commit(Some("g".repeat(40)));
        assert!(!controller.selected_commit_is_object_id());
    }

    #[test]
    fn repository_operations_are_disabled_during_revalidation() {
        let request = GitRepositoryRequest::at("/tmp/repository");
        let mut app = App::new();
        app.add_systems(Update, operations);
        let page = app
            .world_mut()
            .spawn((
                GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        repository: Some(GitRepositorySnapshot::loading(&request)),
                        loading: true,
                        ..Default::default()
                    },
                },
                GitPageControllerState {
                    operations: GitOperationEligibility {
                        stage_all: true,
                        push: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .get::<GitPageControllerState>(page)
                .unwrap()
                .operations,
            GitOperationEligibility::default()
        );
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
        assert_eq!(
            controller.selected_branch_collection,
            GitBranchCollection::Local
        );
        assert_eq!(controller.selected_branch, "vmx-226-session-management");
        assert_eq!(
            controller.selected_branch_path,
            "crates/feature/vmux_session/src/ui.rs"
        );
    }

    #[test]
    fn selecting_a_local_branch_only_changes_the_viewed_reference() {
        let mut app = App::new();
        app.add_observer(branch_select_request);
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

        let controller = app.world().get::<GitPageControllerState>(page).unwrap();
        assert_eq!(controller.focused_panel, GitPanel::Branches);
        assert_eq!(controller.selected_branch, "feature");
        assert_eq!(
            controller.selected_branch_collection,
            GitBranchCollection::Local
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

        app.world_mut()
            .trigger(UiStateWrite::<GitUiState>::from_event(
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
    fn bare_git_page_opens_repository_from_working_directory() {
        let repository = tempfile::tempdir().unwrap();
        std::fs::create_dir(repository.path().join(".git")).unwrap();
        let path = repository.path().to_string_lossy().into_owned();
        let commit = "a".repeat(40);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ControllerPlugin))
            .init_resource::<InitialRequests>()
            .add_observer(InitialRequests::repository)
            .add_observer(InitialRequests::directory);
        let webview = app.world_mut().spawn_empty().id();

        app.world_mut()
            .trigger(UiStateWrite::<GitUiState>::from_event(
                webview,
                &GitPageContext {
                    working_directory: path.clone(),
                    page_url: format!("git://?commit={commit}"),
                },
            ));
        InitialRequests::settle(&mut app);

        let requests = app.world().resource::<InitialRequests>();
        assert_eq!(requests.repositories.len(), 1);
        assert_eq!(requests.repositories[0].path, path);
        assert_eq!(requests.repositories[0].commit, commit);
        assert!(requests.directories.is_empty());
    }

    #[test]
    fn bare_git_page_opens_directory_when_working_directory_is_not_a_repository() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_string_lossy().into_owned();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ControllerPlugin))
            .init_resource::<InitialRequests>()
            .add_observer(InitialRequests::repository)
            .add_observer(InitialRequests::directory);
        let webview = app.world_mut().spawn_empty().id();

        app.world_mut()
            .trigger(UiStateWrite::<GitUiState>::from_event(
                webview,
                &GitPageContext {
                    working_directory: path.clone(),
                    page_url: "git://".to_string(),
                },
            ));
        InitialRequests::settle(&mut app);

        let requests = app.world().resource::<InitialRequests>();
        assert!(requests.repositories.is_empty());
        assert_eq!(requests.directories, vec![path]);
    }

    #[test]
    fn matching_workspace_update_does_not_supersede_an_explicit_repository_load() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ControllerPlugin));
        let mut request = GitRepositoryRequest::project(
            "/tmp/repository/project".into(),
            "project".into(),
            "main".into(),
        );
        request.request_id = 7;
        let webview = app
            .world_mut()
            .spawn(GitState {
                snapshot: crate::state::GitPageSnapshot::loading(&request, 1),
            })
            .id();

        app.world_mut()
            .trigger(UiStateWrite::<GitUiState>::from_event(
                webview,
                &GitWorkspaceChanged {
                    path: "/tmp/repository/./project".into(),
                    branch: "main".into(),
                    error: String::new(),
                },
            ));
        app.update();

        let state = app.world().get::<GitState>(webview).unwrap();
        assert_eq!(state.workspace(), "/tmp/repository/project");
        assert_eq!(state.snapshot.repository_request_id, 7);
        assert!(state.snapshot.loading);
    }

    #[test]
    fn different_workspace_update_supersedes_an_explicit_repository_load() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ControllerPlugin));
        let mut request = GitRepositoryRequest::project(
            "/tmp/repository/project".into(),
            "project".into(),
            "main".into(),
        );
        request.request_id = 7;
        let webview = app
            .world_mut()
            .spawn(GitState {
                snapshot: crate::state::GitPageSnapshot::loading(&request, 1),
            })
            .id();

        app.world_mut()
            .trigger(UiStateWrite::<GitUiState>::from_event(
                webview,
                &GitWorkspaceChanged {
                    path: "/tmp/other".into(),
                    branch: "other".into(),
                    error: String::new(),
                },
            ));
        app.update();

        let state = app.world().get::<GitState>(webview).unwrap();
        assert_eq!(state.workspace(), "/tmp/other");
        assert_eq!(state.snapshot.repository_request_id, 0);
        assert!(state.snapshot.loading);
    }

    #[test]
    fn cache_restore_does_not_skip_a_workspace_update_in_the_same_patch() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            super::super::state::StatePlugin,
            ControllerPlugin,
        ))
        .init_resource::<super::super::job_runner::RepositoryLoads>();
        let request = GitRepositoryRequest::at("/tmp/cached");
        let source = app
            .world_mut()
            .spawn((
                GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        workspace: "/tmp/cached".to_string(),
                        repository: Some(GitRepositorySnapshot {
                            path: "/tmp/cached".to_string(),
                            repo_root: "/tmp/cached".to_string(),
                            repo_name: "cached".to_string(),
                            branch: "main".to_string(),
                            ..GitRepositorySnapshot::loading(&request)
                        }),
                        loading: false,
                        ..Default::default()
                    },
                },
                GitPageControllerState::for_repository("main".to_string()),
            ))
            .id();
        app.world_mut().entity_mut(source).remove::<GitState>();
        let target = app.world_mut().spawn(GitState::default()).id();

        app.world_mut()
            .trigger(UiStateWrite::<GitUiState>::from_event(
                target,
                &crate::state::GitUiStatePatch {
                    context: Some(GitPageContext {
                        working_directory: "/tmp/cached".to_string(),
                        page_url: String::new(),
                    }),
                    workspace: Some(GitWorkspaceChanged {
                        path: "/tmp/other".to_string(),
                        branch: "other".to_string(),
                        error: String::new(),
                    }),
                    ..Default::default()
                },
            ));
        app.update();

        let state = app.world().get::<GitState>(target).unwrap();
        assert_eq!(state.workspace(), "/tmp/other");
        assert!(state.snapshot.loading);
        assert_eq!(
            app.world()
                .get::<GitPageControllerState>(target)
                .unwrap()
                .selected_branch,
            "other"
        );
    }

    #[test]
    fn cached_repository_keeps_the_checkout_when_a_legacy_url_names_another_branch() {
        for branch in ["main", "feature/ui"] {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                super::super::state::StatePlugin,
                ControllerPlugin,
            ))
            .init_resource::<super::super::job_runner::RepositoryLoads>();
            let path = "/tmp/cached";
            let request = GitRepositoryRequest::at(path);
            let mut controller = GitPageControllerState::for_repository("main".into());
            controller.focused_panel = GitPanel::Branches;
            let source = app
                .world_mut()
                .spawn((
                    GitState {
                        snapshot: crate::state::GitPageSnapshot {
                            workspace: path.into(),
                            repository: Some(GitRepositorySnapshot {
                                path: path.into(),
                                repo_root: path.into(),
                                branch: "main".into(),
                                ..GitRepositorySnapshot::loading(&request)
                            }),
                            branch_files_reference: "main".into(),
                            branch_diff_paths: vec!["old.txt".into()],
                            loading: false,
                            ..Default::default()
                        },
                    },
                    controller,
                ))
                .id();
            app.world_mut().entity_mut(source).remove::<GitState>();
            let target = app.world_mut().spawn(GitState::default()).id();
            let mut url = url::Url::parse("vmux://git/").unwrap();
            url.path_segments_mut().unwrap().clear().extend([
                "branch-changes",
                "file:///tmp/cached",
                branch,
            ]);
            app.world_mut()
                .trigger(UiStateWrite::<GitUiState>::from_event(
                    target,
                    &GitPageContext {
                        working_directory: path.into(),
                        page_url: url.to_string(),
                    },
                ));

            let restored = app.world().get::<GitPageControllerState>(target).unwrap();
            assert_eq!(restored.focused_panel, GitPanel::Branches);
            assert_eq!(restored.selected_branch, "main");
            let state = app.world().get::<GitState>(target).unwrap();
            assert_eq!(state.snapshot.branch_diff_paths, ["old.txt"]);
        }
    }

    #[test]
    fn navigation_context_restores_each_git_view_including_back_to_changes() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ControllerPlugin));
        let path = "/tmp/repository";
        let request = GitRepositoryRequest::at(path);
        let webview = app
            .world_mut()
            .spawn(GitState {
                snapshot: crate::state::GitPageSnapshot {
                    workspace: path.into(),
                    repository: Some(GitRepositorySnapshot {
                        path: path.into(),
                        repo_root: path.into(),
                        branch: "main".into(),
                        ..GitRepositorySnapshot::loading(&request)
                    }),
                    loading: false,
                    ..Default::default()
                },
            })
            .id();
        let commit = "a".repeat(40);
        for (panel, selection) in [
            (GitPanel::Branches, "feature/ui"),
            (GitPanel::Commits, commit.as_str()),
            (GitPanel::Stash, "stash@{1}"),
            (GitPanel::Files, ""),
        ] {
            let url = crate::GitUrl::from_view(Path::new(path), panel, Some(selection)).unwrap();
            app.world_mut()
                .trigger(UiStateWrite::<GitUiState>::from_event(
                    webview,
                    &GitPageContext {
                        working_directory: path.into(),
                        page_url: url,
                    },
                ));
            app.update();
            let controller = app.world().get::<GitPageControllerState>(webview).unwrap();
            assert_eq!(controller.focused_panel, panel);
            match panel {
                GitPanel::Branches => assert_eq!(controller.selected_branch, "main"),
                GitPanel::Commits => assert_eq!(controller.selected_commit, selection),
                GitPanel::Stash => assert_eq!(controller.selected_stash, selection),
                GitPanel::Files => assert!(controller.selected_commit.is_empty()),
            }
        }
        let complete = crate::state::GitCatalogProgress {
            loaded: true,
            ..Default::default()
        };
        {
            let mut state = app.world_mut().get_mut::<GitState>(webview).unwrap();
            state.snapshot.local_branches = complete;
            state.snapshot.remote_branches = complete;
            state.snapshot.tags = complete;
        }
        app.update();
        assert_eq!(
            app.world()
                .get::<GitPageControllerState>(webview)
                .unwrap()
                .selected_branch,
            "main"
        );
    }

    #[test]
    fn cache_restore_reuses_the_watch_and_revalidates_in_background() {
        let repo = super::super::repository::test_repo::init();
        let path = repo.path().to_string_lossy().into_owned();
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            super::super::state::StatePlugin,
            ControllerPlugin,
        ))
        .init_resource::<super::super::job_runner::RepositoryLoads>()
        .insert_non_send(super::super::watch::GitWatch::test());
        let request = GitRepositoryRequest::at(path.clone());
        let source = app
            .world_mut()
            .spawn((
                GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        workspace: path.clone(),
                        repository: Some(GitRepositorySnapshot {
                            path: path.clone(),
                            repo_root: path.clone(),
                            repo_name: "cached".to_string(),
                            branch: "main".to_string(),
                            ..GitRepositorySnapshot::loading(&request)
                        }),
                        loading: false,
                        ..Default::default()
                    },
                },
                GitPageControllerState::for_repository("main".to_string()),
            ))
            .id();
        let prepared = super::super::watch::GitWatchSubscription::read(repo.path()).unwrap();
        app.world_mut()
            .non_send_mut::<super::super::watch::GitWatch>()
            .subscribe_page(source, repo.path(), prepared)
            .unwrap();
        app.world_mut().entity_mut(source).remove::<GitState>();
        let target = app
            .world_mut()
            .spawn((GitState::default(), vmux_ecs::PageTransition))
            .id();

        app.world_mut()
            .trigger(UiStateWrite::<GitUiState>::from_event(
                target,
                &crate::state::GitUiStatePatch {
                    context: Some(GitPageContext {
                        working_directory: path.clone(),
                        page_url: String::new(),
                    }),
                    ..Default::default()
                },
            ));
        app.update();

        assert!(
            app.world()
                .get::<GitState>(target)
                .unwrap()
                .snapshot
                .loading
        );
        assert!(
            app.world()
                .non_send::<super::super::watch::GitWatch>()
                .has_subscription(target)
        );
        let mut repository_jobs = app.world_mut().query::<&super::super::job::RepositoryJob>();
        let jobs = repository_jobs
            .iter(app.world())
            .map(|job| (job.path.clone(), job.refresh_catalogs, job.revision))
            .collect::<Vec<_>>();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].0, repo.path());
        assert!(jobs[0].1);
        assert!(jobs[0].2 > 0);
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
