use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::page::PageReady;
use vmux_path::PathIdentity;

use crate::event::{
    GitCatalog, GitDiffRevealRequest, GitDiffRow, GitDiffViewport, GitOperationError,
    GitOperationResult, GitRepositoryRequest, GitRepositorySnapshot,
};
#[cfg(test)]
use crate::state::GitCatalogProgress;
use crate::state::{
    CheckedOutBranch, GitCommandLogEntry, GitDiffRowsPatch, GitPageControllerState,
    GitPageSnapshot, GitRepositoryPatch, UiGit, RepositoryName, RepositoryPath,
};

use super::controller::{GitCommitResultSequence, PendingBranchCheckout, SelectionRevealRevision};
use super::directory::GitDirectoryNavigation;
use super::job::RepositoryJob;
use super::job_runner::{GitJob, RepositoryLoads};
use super::watch::{GitWatch, GitWatchSubscription};

type UiGitUpdates = vmux_ecs::UiState<UiGit>;

pub(super) struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_ecs::UiStatePlugin::<UiGit>::default())
            .add_plugins(UiEventPlugin::<(GitRepositoryRequest,)>::default())
            .init_resource::<GitPageCache>()
            .add_observer(page_ready)
            .add_observer(repository_request)
            .add_observer(cache_removed_page)
            .add_systems(
                Update,
                (
                    cleanup_inactive_views,
                    project_diff,
                    sync_selection,
                    publish,
                )
                    .chain()
                    .after(super::controller::ControllerSet::Operations),
            );
    }
}

#[derive(Component, Default)]
#[require(
    UiGitUpdates,
    GitPageControllerState,
    GitDirectoryNavigation,
    PendingBranchCheckout,
    SelectionRevealRevision,
    GitCommitResultSequence,
    GitDiffRevealRanges,
    GitDiffProjectionState,
    GitCatalogRevision,
    GitRepositoryLoadRevision,
    GitRepositoryRevision,
    super::controller::LocationCursor
)]
pub(super) struct GitState {
    pub(super) snapshot: GitPageSnapshot,
}

#[derive(Component, Default)]
pub(super) struct GitCatalogRevision {
    sequence: u64,
    active: [u64; 5],
}

impl GitCatalogRevision {
    pub(super) fn advance(&mut self) {
        self.sequence = self.sequence.wrapping_add(1).max(1);
    }

    pub(super) fn start(&mut self, catalog: GitCatalog) -> u64 {
        self.advance();
        let generation = self.sequence;
        self.active[Self::index(catalog)] = generation;
        generation
    }

    pub(super) fn finish(&mut self, catalog: GitCatalog, generation: u64) -> bool {
        let active = &mut self.active[Self::index(catalog)];
        if *active != generation {
            return false;
        }
        *active = 0;
        self.advance();
        true
    }

    fn index(catalog: GitCatalog) -> usize {
        match catalog {
            GitCatalog::LocalBranches => 0,
            GitCatalog::RemoteBranches => 1,
            GitCatalog::Tags => 2,
            GitCatalog::Commits => 3,
            GitCatalog::Stashes => 4,
        }
    }
}

#[derive(Component, Default)]
pub(super) struct GitRepositoryLoadRevision(u64);

impl GitRepositoryLoadRevision {
    pub(super) fn advance(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1).max(1);
        self.0
    }

    pub(super) fn matches(&self, revision: u64) -> bool {
        self.0 == revision
    }
}

#[derive(Component, Default)]
pub(super) struct GitRepositoryRevision(u64);

impl GitRepositoryRevision {
    fn advance(&mut self) {
        self.0 = self.0.wrapping_add(1).max(1);
    }
}

const GIT_PAGE_CACHE_CAPACITY: usize = 4;

pub(super) struct CachedGitPage {
    pub(super) snapshot: GitPageSnapshot,
    pub(super) controller: GitPageControllerState,
    pub(super) watch: Option<GitWatchSubscription>,
    pub(super) diff: Option<super::diff::GitDiffQuery>,
    pub(super) branch_files: Option<super::diff::GitBranchFilesQuery>,
}

impl CachedGitPage {
    fn capture(
        state: &mut GitState,
        controller: &GitPageControllerState,
        watch: Option<GitWatchSubscription>,
        diff: Option<&super::diff::GitDiffQuery>,
        branch_files: Option<&super::diff::GitBranchFilesQuery>,
    ) -> Option<Self> {
        if state.snapshot.loading || state.snapshot.repository.is_none() {
            return None;
        }
        let mut snapshot = std::mem::take(&mut state.snapshot);
        let cache_diff = !snapshot.diff_loading;
        let cache_branch_files = !snapshot.branch_files_loading;
        snapshot.loading = false;
        snapshot.fetching = false;
        snapshot.diff_loading = false;
        snapshot.branch_files_loading = false;
        snapshot.branch_files_error.clear();
        snapshot.message.clear();
        snapshot.result = None;
        snapshot.result_sequence = 0;
        if let Some(viewport) = snapshot.diff_viewport.as_mut() {
            viewport.error.clear();
        }
        for progress in [
            &mut snapshot.local_branches,
            &mut snapshot.remote_branches,
            &mut snapshot.tags,
            &mut snapshot.commits,
            &mut snapshot.stashes,
        ] {
            progress.loading = false;
            progress.failed = false;
        }
        let mut controller = controller.clone();
        controller.confirm_discard.clear();
        controller.shortcut_help_visible = false;
        controller.branch_prompt = None;
        controller.branch_draft.clear();
        controller.commit_pending.clear();
        controller.operations = Default::default();
        Some(Self {
            snapshot,
            controller,
            watch,
            diff: cache_diff
                .then(|| diff.map(super::diff::GitDiffQuery::parked))
                .flatten(),
            branch_files: cache_branch_files.then(|| branch_files.cloned()).flatten(),
        })
    }

    fn matches(&self, path: &str) -> bool {
        let Some(repository) = self.snapshot.repository.as_ref() else {
            return false;
        };
        let requested = PathIdentity::lexical(path);
        let matches_alias = (!self.snapshot.workspace.is_empty()
            && PathIdentity::lexical(&self.snapshot.workspace) == requested)
            || (!repository.path.is_empty()
                && PathIdentity::lexical(&repository.path) == requested)
            || (!repository.repo_root.is_empty()
                && PathIdentity::lexical(&repository.repo_root) == requested);
        if !matches_alias {
            return false;
        }
        let resolved = PathIdentity::resolve(path);
        PathIdentity::resolve(&repository.path) == resolved
            || PathIdentity::resolve(&repository.repo_root) == resolved
    }

    fn restoring(mut self) -> Self {
        self.snapshot.loading = true;
        self.snapshot.fetching = false;
        self.controller.operations = Default::default();
        self
    }

    fn retained(&self) -> Self {
        Self {
            snapshot: self.snapshot.clone(),
            controller: self.controller.clone(),
            watch: None,
            diff: self.diff.as_ref().map(super::diff::GitDiffQuery::parked),
            branch_files: self.branch_files.clone(),
        }
    }
}

#[derive(Resource, Default)]
pub(super) struct GitPageCache {
    entries: HashMap<String, CachedGitPage>,
    aliases: HashMap<String, String>,
    order: VecDeque<String>,
}

impl GitPageCache {
    fn key(path: &str) -> String {
        PathIdentity::lexical(Path::new(path))
            .into_path_buf()
            .to_string_lossy()
            .into_owned()
    }

    #[cfg(test)]
    fn store(
        &mut self,
        state: &mut GitState,
        controller: &GitPageControllerState,
        watch: Option<GitWatchSubscription>,
    ) {
        self.store_with_queries(state, controller, watch, None, None);
    }

    fn store_with_queries(
        &mut self,
        state: &mut GitState,
        controller: &GitPageControllerState,
        watch: Option<GitWatchSubscription>,
        diff: Option<&super::diff::GitDiffQuery>,
        branch_files: Option<&super::diff::GitBranchFilesQuery>,
    ) {
        let Some(cached) = CachedGitPage::capture(state, controller, watch, diff, branch_files)
        else {
            return;
        };
        let repository = cached.snapshot.repository.as_ref().unwrap();
        let root = Self::key(&repository.repo_root);
        if root.is_empty() {
            return;
        }
        let aliases = [
            cached.snapshot.workspace.clone(),
            repository.path.clone(),
            repository.repo_root.clone(),
        ];
        self.remove_aliases(&root);
        self.order.retain(|entry| entry != &root);
        self.order.push_back(root.clone());
        self.entries.insert(root.clone(), cached);
        for alias in aliases {
            if !alias.is_empty() {
                self.aliases.insert(Self::key(&alias), root.clone());
            }
        }
        while self.order.len() > GIT_PAGE_CACHE_CAPACITY {
            let Some(expired) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&expired);
            self.aliases.retain(|_, root| root != &expired);
        }
    }

    pub(super) fn restore(&mut self, path: &str) -> Option<CachedGitPage> {
        let requested = Self::key(path);
        let root = self.aliases.get(&requested).cloned().unwrap_or(requested);
        if !self.entries.get(&root)?.matches(path) {
            self.aliases.remove(&Self::key(path));
            return None;
        }
        let cached = self.entries.remove(&root)?;
        self.entries.insert(root.clone(), cached.retained());
        self.order.retain(|entry| entry != &root);
        self.order.push_back(root);
        Some(cached.restoring())
    }

    fn remove_aliases(&mut self, root: &str) {
        self.aliases.retain(|_, candidate| candidate != root);
    }
}

const MAX_DIFF_REVEAL_RANGES: usize = 256;

#[derive(Component, Clone, Debug, Default)]
pub(super) struct GitDiffRevealRanges(pub(super) Vec<(usize, usize)>);

impl GitDiffRevealRanges {
    pub(super) fn reveal(
        &mut self,
        viewport: Option<&GitDiffViewport>,
        rows: &[GitDiffRow],
        request: &GitDiffRevealRequest,
    ) -> bool {
        let Some(viewport) = viewport else {
            return false;
        };
        if request.generation != viewport.generation || request.revision != viewport.revision {
            return false;
        }
        let limit = viewport.lines.len();
        let start = (request.start as usize).min(limit);
        let end = (request.end as usize).min(limit);
        if start >= end
            || !rows.iter().any(|row| {
                matches!(
                    row,
                    GitDiffRow::Gap {
                        start: gap_start,
                        end: gap_end,
                        ..
                    } if start >= *gap_start as usize && end <= *gap_end as usize
                )
            })
        {
            return false;
        }
        let mut ranges = self.0.clone();
        ranges.push((start, end));
        ranges.sort_unstable();
        let mut merged = Vec::with_capacity(ranges.len());
        for (start, end) in ranges {
            if let Some((_, previous_end)) = merged.last_mut()
                && start <= *previous_end
            {
                *previous_end = (*previous_end).max(end);
                continue;
            }
            merged.push((start, end));
        }
        if merged == self.0 || merged.len() > MAX_DIFF_REVEAL_RANGES {
            return false;
        }
        self.0 = merged;
        true
    }
}

#[derive(Component, Default)]
pub(super) struct GitDiffProjectionState {
    generation: u64,
    lines: usize,
    revealed: Vec<(usize, usize)>,
}

impl GitDiffProjectionState {
    fn current(&self, snapshot: &GitPageSnapshot, revealed: &[(usize, usize)]) -> bool {
        let generation = snapshot
            .diff_viewport
            .as_ref()
            .map(|viewport| viewport.generation)
            .unwrap_or_default();
        let lines = snapshot
            .diff_viewport
            .as_ref()
            .map(|viewport| viewport.lines.len())
            .unwrap_or_default();
        self.generation == generation && self.lines == lines && self.revealed == revealed
    }

    pub(super) fn capture(&mut self, snapshot: &GitPageSnapshot, revealed: &[(usize, usize)]) {
        self.generation = snapshot
            .diff_viewport
            .as_ref()
            .map(|viewport| viewport.generation)
            .unwrap_or_default();
        self.lines = snapshot
            .diff_viewport
            .as_ref()
            .map(|viewport| viewport.lines.len())
            .unwrap_or_default();
        self.revealed.clear();
        self.revealed.extend_from_slice(revealed);
    }
}

impl GitState {
    pub(super) fn workspace(&self) -> &str {
        &self.snapshot.workspace
    }

    fn reset_workspace(&mut self, workspace: String) {
        let nonce = self.snapshot.nonce;
        self.snapshot = GitPageSnapshot {
            workspace,
            nonce,
            loading: true,
            ..Default::default()
        };
    }

    pub(super) fn diff_revision(&self) -> u32 {
        self.snapshot.nonce
    }

    pub(super) fn repository(&self) -> Option<&GitRepositorySnapshot> {
        self.snapshot.repository.as_ref()
    }

    fn repository_matches(&self, path: &str) -> bool {
        if path.is_empty() {
            return false;
        }
        let requested = PathIdentity::lexical(path);
        if !self.snapshot.workspace.is_empty()
            && PathIdentity::lexical(&self.snapshot.workspace) == requested
        {
            return true;
        }
        let Some(repository) = self.snapshot.repository.as_ref() else {
            return false;
        };
        if !repository.path.is_empty() && PathIdentity::lexical(&repository.path) == requested {
            return true;
        }
        !repository.repo_root.is_empty()
            && PathIdentity::lexical(&repository.repo_root) == requested
    }

    fn repository_loading_at(&self, path: &str) -> bool {
        self.snapshot.loading && self.repository_matches(path)
    }

    fn repository_loading_request(&self, request: &GitRepositoryRequest) -> bool {
        self.snapshot.loading
            && self.snapshot.repository.is_some()
            && self.snapshot.repository_request_id == request.request_id
            && self.repository_matches(&request.path)
    }

    fn accepts_repository_request(&self, request: &GitRepositoryRequest) -> bool {
        self.snapshot.repository_request_id == 0
            || (request.request_id != 0
                && request.request_id >= self.snapshot.repository_request_id)
    }

    fn replace_repository(
        &mut self,
        mut repository: GitRepositorySnapshot,
        refresh_catalogs: bool,
    ) {
        let repository_request_id = self.snapshot.repository_request_id;
        let mut same_repository = false;
        if let Some(previous) = self.snapshot.repository.take()
            && previous.repo_root == repository.repo_root
        {
            same_repository = true;
            repository.commits = previous.commits;
            repository.branches = previous.branches;
            repository.remote_branches = previous.remote_branches;
            repository.tags = previous.tags;
            repository.stashes = previous.stashes;
        }
        self.snapshot.workspace.clone_from(&repository.repo_root);
        self.snapshot.repository_request_id = repository_request_id;
        self.snapshot.repository = Some(repository);
        self.snapshot.nonce = self.snapshot.nonce.wrapping_add(1).max(1);
        if same_repository {
            if refresh_catalogs {
                self.snapshot.refresh_catalogs();
            }
        } else {
            self.snapshot.invalidate_catalogs();
        }
        self.snapshot.loading = false;
        self.snapshot.message.clear();
    }

    fn prime_repository(&mut self, repository: GitRepositorySnapshot) {
        self.snapshot.repository = Some(repository);
        self.snapshot.loading = true;
        self.snapshot.message.clear();
    }
}

#[derive(SystemParam)]
pub(super) struct GitStates<'w, 's> {
    states: Query<'w, 's, &'static mut GitState>,
    load_revisions: Query<'w, 's, &'static mut GitRepositoryLoadRevision>,
    repository_revisions: Query<'w, 's, &'static mut GitRepositoryRevision>,
    catalog_revisions: Query<'w, 's, &'static mut GitCatalogRevision>,
}

impl GitStates<'_, '_> {
    fn advance_load_revision(&mut self, entity: Entity) -> Option<u64> {
        let Ok(mut revision) = self.load_revisions.get_mut(entity) else {
            return None;
        };
        Some(revision.advance())
    }

    pub(super) fn contains(&self, entity: Entity) -> bool {
        self.states.contains(entity)
    }

    pub(super) fn repository_loaded_at(&self, entity: Entity, path: &str) -> bool {
        self.states.get(entity).is_ok_and(|state| {
            !state.snapshot.loading
                && state.snapshot.repository.is_some()
                && state.repository_matches(path)
        })
    }

    pub(super) fn repository_matches(&self, entity: Entity, path: &Path) -> bool {
        self.states
            .get(entity)
            .is_ok_and(|state| state.repository_matches(&path.to_string_lossy()))
    }

    pub(super) fn repository_loading_at(&self, entity: Entity, path: &str) -> bool {
        self.states
            .get(entity)
            .is_ok_and(|state| state.repository_loading_at(path))
    }

    pub(super) fn repository_loading_request(
        &self,
        entity: Entity,
        request: &GitRepositoryRequest,
    ) -> bool {
        self.states
            .get(entity)
            .is_ok_and(|state| state.repository_loading_request(request))
    }

    pub(super) fn repository_requested_at(
        &self,
        entity: Entity,
        path: &Path,
        revision: u64,
    ) -> bool {
        self.states.get(entity).is_ok_and(|state| {
            self.load_revisions
                .get(entity)
                .is_ok_and(|current| current.0 == revision)
                && PathIdentity::lexical(Path::new(&state.snapshot.workspace))
                    == PathIdentity::lexical(path)
        })
    }

    pub(super) fn reset(&mut self, entity: Entity, workspace: String) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.reset_workspace(workspace);
        self.advance_load_revision(entity);
    }

    pub(super) fn restore(&mut self, entity: Entity, snapshot: GitPageSnapshot) -> bool {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return false;
        };
        state.snapshot = snapshot;
        self.advance_load_revision(entity);
        true
    }

    pub(super) fn start_repository(
        &mut self,
        entity: Entity,
        request: &GitRepositoryRequest,
    ) -> Option<u64> {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return None;
        };
        if !state.accepts_repository_request(request) {
            return None;
        }
        state.snapshot.begin_repository(request);
        self.advance_load_revision(entity)
    }

    pub(super) fn set_repository(
        &mut self,
        entity: Entity,
        repository: GitRepositorySnapshot,
        refresh_catalogs: bool,
    ) -> Option<GitRepositoryPatch> {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return None;
        };
        state
            .bypass_change_detection()
            .replace_repository(repository, refresh_catalogs);
        let patch = GitRepositoryPatch::from_snapshot(&state.snapshot);
        if let Ok(mut revision) = self.repository_revisions.get_mut(entity) {
            revision.advance();
        }
        if let Ok(mut revision) = self.catalog_revisions.get_mut(entity) {
            revision.advance();
        }
        Some(patch)
    }

    pub(super) fn prime_repository(&mut self, entity: Entity, repository: GitRepositorySnapshot) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.prime_repository(repository);
    }

    pub(super) fn start_directory(&mut self, entity: Entity, path: &Path) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.snapshot.workspace = path.to_string_lossy().into_owned();
        state.snapshot.repository_request_id = 0;
        state.snapshot.loading = true;
        state.snapshot.message.clear();
        state.snapshot.nonce = state.snapshot.nonce.wrapping_add(1).max(1);
        self.advance_load_revision(entity);
    }

    pub(super) fn finish_directory(&mut self, entity: Entity) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.snapshot.repository = None;
        state.snapshot.loading = false;
        state.snapshot.message.clear();
    }

    pub(super) fn apply_result(
        &mut self,
        entity: Entity,
        event: &GitOperationResult,
    ) -> Option<(String, u64)> {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return None;
        };
        Self::push_log(
            &mut state,
            GitCommandLogEntry {
                operation: event.operation.clone(),
                message: event.message.clone(),
                ok: event.ok,
            },
        );
        if event.operation == "fetch" {
            state.snapshot.fetching = false;
        }
        if event.ok {
            state.snapshot.message.clear();
            if !matches!(
                event.operation.as_str(),
                "stage" | "unstage" | "discard" | "stage all" | "hunk"
            ) {
                state.snapshot.refresh_catalogs();
            }
        } else {
            state.snapshot.message.clone_from(&event.message);
        }
        state.snapshot.nonce = state.snapshot.nonce.wrapping_add(1).max(1);
        state.snapshot.result = Some(event.clone());
        state.snapshot.result_sequence = state.snapshot.result_sequence.wrapping_add(1).max(1);
        let workspace = state.snapshot.workspace.clone();
        self.advance_load_revision(entity)
            .map(|revision| (workspace, revision))
    }

    pub(super) fn apply_error(&mut self, entity: Entity, event: &GitOperationError) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        Self::push_log(
            &mut state,
            GitCommandLogEntry {
                operation: String::new(),
                message: event.message.clone(),
                ok: false,
            },
        );
        state.snapshot.loading = false;
        state.snapshot.fetching = false;
        state.snapshot.message.clone_from(&event.message);
    }

    pub(super) fn fail_catalog(
        &mut self,
        entity: Entity,
        catalog: GitCatalog,
        generation: u64,
        event: &GitOperationError,
    ) {
        let Ok(mut revision) = self.catalog_revisions.get_mut(entity) else {
            return;
        };
        if !revision.finish(catalog, generation) {
            return;
        }
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        let progress = state.snapshot.catalog_mut(catalog);
        progress.loading = false;
        progress.failed = true;
        state.snapshot.message.clone_from(&event.message);
    }

    pub(super) fn fail_branch_files(&mut self, entity: Entity, event: &GitOperationError) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.snapshot.branch_files_loading = false;
        state.snapshot.diff_loading = false;
        state.snapshot.branch_files_error.clone_from(&event.message);
    }

    pub(super) fn fail_diff(&mut self, entity: Entity, generation: u64, event: &GitOperationError) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.snapshot.diff_loading = false;
        state.snapshot.diff_rows.clear();
        state.snapshot.diff_viewport = Some(GitDiffViewport {
            generation,
            revision: 0,
            first_line: 0,
            total_lines: 0,
            has_more: false,
            lines: Vec::new(),
            markers: Vec::new(),
            error: event.message.clone(),
        });
    }

    pub(super) fn fail_repository(&mut self, entity: Entity, event: &GitOperationError) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.snapshot.repository = None;
        state.snapshot.diff_viewport = None;
        state.snapshot.diff_rows.clear();
        state.snapshot.branch_files.clear();
        state.snapshot.branch_diff_paths.clear();
        state.snapshot.branch_files_loading = false;
        state.snapshot.diff_loading = false;
        state.snapshot.loading = false;
        state.snapshot.message.clone_from(&event.message);
    }

    pub(super) fn apply_workspace_error(&mut self, entity: Entity, message: String) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        Self::push_log(
            &mut state,
            GitCommandLogEntry {
                operation: String::new(),
                message: message.clone(),
                ok: false,
            },
        );
        state.snapshot.loading = false;
        state.snapshot.fetching = false;
        state.snapshot.message = message;
    }

    pub(super) fn mark_changed(&mut self, entity: Entity) -> Option<(String, u64)> {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return None;
        };
        let nonce = state.snapshot.nonce.wrapping_add(1).max(1);
        state.bypass_change_detection().snapshot.nonce = nonce;
        let workspace = state.snapshot.workspace.clone();
        if workspace.is_empty() {
            return None;
        }
        self.advance_load_revision(entity)
            .map(|revision| (workspace, revision))
    }

    pub(super) fn refresh_repository(&mut self, entity: Entity) -> Option<(String, u64)> {
        let workspace = self.states.get(entity).ok()?.snapshot.workspace.clone();
        if workspace.is_empty() {
            return None;
        }
        self.advance_load_revision(entity)
            .map(|revision| (workspace, revision))
    }

    fn push_log(state: &mut GitState, entry: GitCommandLogEntry) {
        if state.snapshot.command_log.len() >= 24 {
            state.snapshot.command_log.remove(0);
        }
        state.snapshot.command_log.push(entry);
    }
}

fn project_diff(
    mut pages: Query<(
        Entity,
        &mut GitState,
        Ref<GitDiffRevealRanges>,
        &mut GitDiffProjectionState,
    )>,
    mut ui_state: vmux_ecs::UiStateWriter<UiGit>,
) {
    for (entity, mut state, revealed, mut projection) in &mut pages {
        if !state.is_changed() && !revealed.is_changed() {
            continue;
        }
        if projection.current(&state.snapshot, &revealed.0) {
            continue;
        }
        state
            .bypass_change_detection()
            .snapshot
            .project_diff_rows(&revealed.0, GitPageSnapshot::DIFF_CONTEXT_LINES);
        projection.capture(&state.snapshot, &revealed.0);
        if let Some(viewport) = state.snapshot.diff_viewport.as_ref() {
            ui_state.push(
                entity,
                GitDiffRowsPatch {
                    generation: viewport.generation,
                    revision: viewport.revision,
                    rows: state.snapshot.diff_rows.clone(),
                },
            );
        }
    }
}

fn page_ready(
    trigger: On<UiInput<PageReady>>,
    pages: Query<&vmux_ecs::PageMetadata>,
    states: Query<&GitState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(page) = pages.get(entity) else {
        return;
    };
    let projects = page.url.trim_end_matches('/') == "vmux://projects"
        || page.url.starts_with("vmux://projects/");
    if !crate::GitUrl::is_page(&page.url) && !projects {
        return;
    }
    if let Ok(state) = states.get(entity) {
        commands.trigger(vmux_ecs::UiStateWrite::<UiGit>::from_event(
            entity,
            &state.snapshot,
        ));
        return;
    }
    commands.entity(entity).insert(GitState::default());
}

fn cache_removed_page(
    trigger: On<Remove, GitState>,
    mut pages: Query<(&mut GitState, &GitPageControllerState)>,
    queries: Query<(
        Option<&super::diff::GitDiffQuery>,
        Option<&super::diff::GitBranchFilesQuery>,
    )>,
    jobs: Query<(Entity, &GitJob)>,
    mut watch: Option<NonSendMut<GitWatch>>,
    mut cache: ResMut<GitPageCache>,
    mut commands: Commands,
) {
    let owner = trigger.event_target();
    let subscription = watch
        .as_deref_mut()
        .and_then(|watch| watch.take_page_subscription(owner));
    if let Ok((mut state, controller)) = pages.get_mut(owner) {
        let (diff, branch_files) = queries.get(owner).unwrap_or_default();
        cache.store_with_queries(&mut state, controller, subscription, diff, branch_files);
    }
    commands.entity(owner).try_remove::<(
        super::diff::GitDiffQuery,
        super::diff::GitBranchFilesQuery,
        super::diff::PendingGitDiff,
        super::diff::PendingGitDiffSeek,
        GitDiffProjectionState,
    )>();
    for (entity, job) in &jobs {
        if job.owner() == owner {
            commands.entity(entity).try_despawn();
        }
    }
}

type ChangedGitPages<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static vmux_ecs::PageMetadata),
    (With<GitState>, Changed<vmux_ecs::PageMetadata>),
>;

fn cleanup_inactive_views(pages: ChangedGitPages, mut commands: Commands) {
    for (entity, page) in &pages {
        let projects = page.url.trim_end_matches('/') == "vmux://projects"
            || page.url.starts_with("vmux://projects/");
        if !crate::GitUrl::is_page(&page.url) && !projects {
            commands
                .entity(entity)
                .remove::<(GitState, RepositoryPath, RepositoryName, CheckedOutBranch)>();
        }
    }
}

type ChangedGitSelections<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static GitState,
        Ref<'static, GitRepositoryRevision>,
        Option<&'static RepositoryPath>,
        Option<&'static RepositoryName>,
        Option<&'static CheckedOutBranch>,
    ),
    Or<(Changed<GitState>, Changed<GitRepositoryRevision>)>,
>;

fn sync_selection(pages: ChangedGitSelections, mut commands: Commands) {
    for (entity, state, _, current_path, current_name, current_branch) in &pages {
        let Some(repository) = state.repository() else {
            if current_path.is_some() || current_name.is_some() || current_branch.is_some() {
                commands
                    .entity(entity)
                    .remove::<(RepositoryPath, RepositoryName, CheckedOutBranch)>();
            }
            continue;
        };
        let branch = repository.branch.clone();
        let mut page = commands.entity(entity);
        if current_path.map(|path| path.0.as_str()) != Some(repository.repo_root.as_str()) {
            page.insert(RepositoryPath(repository.repo_root.clone()));
        }
        if current_name.map(|name| name.0.as_str()) != Some(repository.repo_name.as_str()) {
            page.insert(RepositoryName(repository.repo_name.clone()));
        }
        if current_branch.map(|current| current.0.as_str()) != Some(branch.as_str()) {
            page.insert(CheckedOutBranch(branch));
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct RepositoryViews<'w, 's> {
    selections: Query<
        'w,
        's,
        (
            &'static mut GitPageControllerState,
            &'static mut PendingBranchCheckout,
            &'static mut GitCommitResultSequence,
            &'static mut GitDiffRevealRanges,
        ),
    >,
    diff_queries: Query<'w, 's, &'static mut super::diff::GitDiffQuery>,
    branch_queries: Query<'w, 's, &'static mut super::diff::GitBranchFilesQuery>,
}

fn repository_request(
    trigger: On<UiInput<GitRepositoryRequest>>,
    mut views: RepositoryViews,
    watch: Option<NonSendMut<GitWatch>>,
    loads: Res<RepositoryLoads>,
    mut states: GitStates,
    queued_jobs: super::job_runner::QueuedGitJobs<GitJob>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let request = &trigger.event().payload;
    if states.repository_loading_request(webview, request) {
        return;
    }
    let path: PathBuf = request.path.clone().into();
    let cached = loads.cached(&path);
    let Some(revision) = states.start_repository(webview, request) else {
        return;
    };
    if let Some(cached) = cached {
        states.prime_repository(webview, cached);
    }
    if let Ok((mut controller, mut pending, mut result, mut revealed)) =
        views.selections.get_mut(webview)
    {
        controller.reset_for_repository(request.branch.clone());
        controller.apply_url_commit((!request.commit.is_empty()).then(|| request.commit.clone()));
        pending.0.clear();
        result.reset();
        revealed.0.clear();
    }
    if let Ok(mut query) = views.diff_queries.get_mut(webview) {
        query.invalidate();
    }
    if let Ok(mut query) = views.branch_queries.get_mut(webview) {
        query.invalidate();
    }
    commands
        .entity(webview)
        .remove::<(super::diff::PendingGitDiff, super::diff::PendingGitDiffSeek)>();
    queued_jobs.despawn_for(webview, &mut commands);
    if let Some(mut watch) = watch {
        watch.release_subscription(webview);
    }
    commands.spawn((
        GitJob::new(webview),
        super::job::GitForegroundRead,
        RepositoryJob {
            path,
            refresh_catalogs: true,
            revision,
        },
    ));
}

type GitStateQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Ref<'static, GitState>,
        Ref<'static, GitPageControllerState>,
        Ref<'static, GitDirectoryNavigation>,
    ),
>;

fn publish(views: GitStateQuery, mut commands: Commands) {
    for (entity, view, controller, directory) in &views {
        if view.is_changed() {
            commands.trigger(vmux_ecs::UiStateWrite::<UiGit>::from_event(
                entity,
                &view.snapshot,
            ));
        }
        if controller.is_changed() {
            commands.trigger(vmux_ecs::UiStateWrite::<UiGit>::from_event(
                entity,
                &*controller,
            ));
        }
        if directory.is_changed() {
            commands.trigger(vmux_ecs::UiStateWrite::<UiGit>::from_event(
                entity,
                &directory.state(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use super::*;
    use crate::event::{DiffKind, DiffLine, GitDiffRevealRequest, GitDiffRow};
    use crate::host::repository::test_repo;

    fn line(kind: DiffKind, number: u32) -> DiffLine {
        DiffLine {
            kind,
            old_no: Some(number),
            new_no: Some(number),
            hunk: None,
            spans: Vec::new(),
        }
    }

    fn cached_state(path: &str) -> GitState {
        let request = GitRepositoryRequest::at(path);
        let mut repository = GitRepositorySnapshot::loading(&request);
        repository.path = format!("{path}/client");
        repository.repo_root = path.to_string();
        repository.repo_name = "repo".to_string();
        repository.branch = "feature".to_string();
        GitState {
            snapshot: GitPageSnapshot {
                workspace: path.to_string(),
                repository: Some(repository),
                loading: false,
                ..Default::default()
            },
        }
    }

    #[test]
    fn removed_git_page_is_restored_by_repository_alias() {
        let mut app = App::new();
        app.init_resource::<GitPageCache>()
            .add_observer(cache_removed_page);
        let page = app
            .world_mut()
            .spawn((
                cached_state("/tmp/repo"),
                GitPageControllerState {
                    selected_branch: "feature".to_string(),
                    ..Default::default()
                },
            ))
            .id();

        app.world_mut().entity_mut(page).remove::<GitState>();

        let restored = app
            .world_mut()
            .resource_mut::<GitPageCache>()
            .restore("/tmp/repo/client")
            .unwrap();
        assert_eq!(restored.snapshot.workspace, "/tmp/repo");
        assert!(restored.snapshot.loading);
        assert_eq!(restored.controller.selected_branch, "feature");
        assert!(
            app.world_mut()
                .resource_mut::<GitPageCache>()
                .restore("/tmp/repo/client")
                .is_some()
        );
    }

    #[test]
    fn removed_git_page_moves_its_watch_subscription_into_the_cache() {
        let repo = test_repo::init();
        let mut app = App::new();
        app.init_resource::<GitPageCache>()
            .insert_non_send(GitWatch::test())
            .add_observer(cache_removed_page);
        let path = repo.path().to_string_lossy().into_owned();
        let page = app
            .world_mut()
            .spawn((cached_state(&path), GitPageControllerState::default()))
            .id();
        let prepared = GitWatchSubscription::read(repo.path()).unwrap();
        app.world_mut()
            .non_send_mut::<GitWatch>()
            .subscribe_page(page, repo.path(), prepared)
            .unwrap();

        app.world_mut().entity_mut(page).remove::<GitState>();

        let restored = app
            .world_mut()
            .resource_mut::<GitPageCache>()
            .restore(&path)
            .unwrap();
        assert!(restored.watch.is_some());
        assert!(!app.world().non_send::<GitWatch>().has_subscription(page));
    }

    #[test]
    fn removing_git_state_cancels_its_jobs_before_reuse() {
        let mut app = App::new();
        app.init_resource::<GitPageCache>()
            .add_observer(cache_removed_page);
        let page = app
            .world_mut()
            .spawn((cached_state("/tmp/repo"), GitPageControllerState::default()))
            .id();
        let job = app.world_mut().spawn(GitJob::new(page)).id();

        app.world_mut().entity_mut(page).remove::<GitState>();
        app.update();

        assert!(app.world().get_entity(job).is_err());
    }

    #[test]
    fn git_page_cache_drops_transient_work() {
        let mut state = cached_state("/tmp/repo");
        state.snapshot.fetching = true;
        state.snapshot.diff_loading = true;
        state.snapshot.branch_files_loading = true;
        state.snapshot.local_branches.loading = true;
        state.snapshot.local_branches.failed = true;
        state.snapshot.message = "stale error".to_string();
        let controller = GitPageControllerState {
            confirm_discard: b"src/lib.rs".to_vec(),
            shortcut_help_visible: true,
            branch_prompt: Some(crate::state::GitBranchPrompt::Create {
                base: "main".to_string(),
            }),
            branch_draft: "feature".to_string(),
            commit_pending: "pending".to_string(),
            operations: crate::state::GitOperationEligibility {
                push: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut cache = GitPageCache::default();

        cache.store(&mut state, &controller, None);
        let restored = cache.restore("/tmp/repo").unwrap();

        assert!(restored.snapshot.loading);
        assert!(!restored.snapshot.fetching);
        assert!(!restored.snapshot.diff_loading);
        assert!(!restored.snapshot.branch_files_loading);
        assert!(!restored.snapshot.local_branches.loading);
        assert!(!restored.snapshot.local_branches.failed);
        assert!(restored.snapshot.message.is_empty());
        assert!(restored.controller.confirm_discard.is_empty());
        assert!(!restored.controller.shortcut_help_visible);
        assert!(restored.controller.branch_prompt.is_none());
        assert!(restored.controller.branch_draft.is_empty());
        assert!(restored.controller.commit_pending.is_empty());
        assert_eq!(
            restored.controller.operations,
            crate::state::GitOperationEligibility::default()
        );
    }

    #[test]
    fn git_page_cache_drops_incomplete_diff_queries() {
        let mut state = cached_state("/tmp/repo");
        state.snapshot.diff_loading = true;
        state.snapshot.branch_files_loading = true;
        let controller = GitPageControllerState::default();
        let diff = super::super::diff::GitDiffQuery::for_test(7);
        let branch = super::super::diff::GitBranchFilesQuery::for_test(11);
        let mut cache = GitPageCache::default();

        cache.store_with_queries(&mut state, &controller, None, Some(&diff), Some(&branch));
        let restored = cache.restore("/tmp/repo").unwrap();

        assert!(restored.diff.is_none());
        assert!(restored.branch_files.is_none());
    }

    #[test]
    fn repeated_reopens_keep_the_cached_diff() {
        let mut app = App::new();
        app.init_resource::<GitPageCache>()
            .add_observer(cache_removed_page);
        let mut state = cached_state("/tmp/reopen-repo");
        state.snapshot.diff_viewport = Some(crate::event::GitDiffViewport {
            generation: 7,
            revision: 2,
            first_line: 0,
            total_lines: 1,
            has_more: false,
            lines: vec![line(DiffKind::Add, 1)],
            markers: Vec::new(),
            error: String::new(),
        });
        let page = app
            .world_mut()
            .spawn((state, GitPageControllerState::default()))
            .id();
        app.world_mut().entity_mut(page).remove::<GitState>();
        let mut cache = app.world_mut().resource_mut::<GitPageCache>();
        let first = cache.restore("/tmp/reopen-repo").unwrap();
        let reopened = cache.restore("/tmp/reopen-repo").unwrap();

        assert!(first.snapshot.diff_viewport.is_some());
        assert_eq!(
            reopened.snapshot.diff_viewport,
            first.snapshot.diff_viewport
        );
    }

    #[test]
    fn git_page_cache_rejects_a_stale_alias() {
        let mut cache = GitPageCache::default();
        let mut state = cached_state("/tmp/repo");
        let controller = GitPageControllerState::default();
        cache.store(&mut state, &controller, None);
        cache.aliases.insert(
            GitPageCache::key("/tmp/other"),
            GitPageCache::key("/tmp/repo"),
        );

        assert!(cache.restore("/tmp/other").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn git_page_cache_rejects_a_retargeted_symlink() {
        let temporary = tempfile::tempdir().unwrap();
        let first = temporary.path().join("first");
        let second = temporary.path().join("second");
        let link = temporary.path().join("repository");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        symlink(&first, &link).unwrap();
        let first = first.to_string_lossy().into_owned();
        let link_text = link.to_string_lossy().into_owned();
        let mut state = cached_state(&first);
        state.snapshot.workspace = link_text.clone();
        let mut cache = GitPageCache::default();
        cache.store(&mut state, &GitPageControllerState::default(), None);

        std::fs::remove_file(&link).unwrap();
        symlink(&second, &link).unwrap();

        assert!(cache.restore(&link_text).is_none());
    }

    #[test]
    fn git_page_cache_keeps_aliases_for_parallel_pages() {
        let mut cache = GitPageCache::default();
        let mut state = cached_state("/tmp/repo");
        state.snapshot.repository.as_mut().unwrap().path = "/tmp/repo/client".to_string();
        let controller = GitPageControllerState::default();
        cache.store(&mut state, &controller, None);

        assert!(cache.restore("/tmp/repo/client").is_some());
        assert!(cache.restore("/tmp/repo/client").is_some());
        assert!(!cache.aliases.is_empty());
    }

    #[test]
    fn git_page_cache_replaces_aliases_for_the_same_repository() {
        let mut cache = GitPageCache::default();
        let controller = GitPageControllerState::default();
        let mut first = cached_state("/tmp/repo");
        first.snapshot.workspace = "/tmp/repo/first".to_string();
        cache.store(&mut first, &controller, None);

        let mut second = cached_state("/tmp/repo");
        second.snapshot.workspace = "/tmp/repo/second".to_string();
        cache.store(&mut second, &controller, None);

        assert!(
            !cache
                .aliases
                .contains_key(&GitPageCache::key("/tmp/repo/first"))
        );
        assert!(
            cache
                .aliases
                .contains_key(&GitPageCache::key("/tmp/repo/second"))
        );
    }

    #[test]
    fn git_page_cache_evicts_the_oldest_repository() {
        let mut cache = GitPageCache::default();
        for index in 0..=GIT_PAGE_CACHE_CAPACITY {
            let mut state = cached_state(&format!("/tmp/repo-{index}"));
            let controller = GitPageControllerState::default();
            cache.store(&mut state, &controller, None);
        }

        assert!(cache.restore("/tmp/repo-0").is_none());
        assert!(
            cache
                .restore(&format!("/tmp/repo-{GIT_PAGE_CACHE_CAPACITY}"))
                .is_some()
        );
    }

    #[test]
    fn diff_reveal_rejects_stale_or_non_gap_ranges() {
        let viewport = GitDiffViewport {
            generation: 3,
            revision: 5,
            first_line: 0,
            total_lines: 10,
            has_more: false,
            lines: vec![line(DiffKind::Context, 1); 10],
            markers: Vec::new(),
            error: String::new(),
        };
        let rows = [GitDiffRow::Gap {
            start: 2,
            end: 8,
            reveal_start: 2,
            reveal_end: 8,
        }];
        let mut revealed = GitDiffRevealRanges::default();

        assert!(!revealed.reveal(
            Some(&viewport),
            &rows,
            &GitDiffRevealRequest {
                generation: 2,
                revision: 5,
                start: 2,
                end: 4,
            },
        ));
        assert!(!revealed.reveal(
            Some(&viewport),
            &rows,
            &GitDiffRevealRequest {
                generation: 3,
                revision: 5,
                start: 0,
                end: 2,
            },
        ));
        assert!(revealed.0.is_empty());
    }

    #[test]
    fn diff_reveal_merges_adjacent_ranges() {
        let viewport = GitDiffViewport {
            generation: 3,
            revision: 5,
            first_line: 0,
            total_lines: 10,
            has_more: false,
            lines: vec![line(DiffKind::Context, 1); 10],
            markers: Vec::new(),
            error: String::new(),
        };
        let rows = [GitDiffRow::Gap {
            start: 2,
            end: 8,
            reveal_start: 2,
            reveal_end: 8,
        }];
        let mut revealed = GitDiffRevealRanges::default();
        let request = |start, end| GitDiffRevealRequest {
            generation: 3,
            revision: 5,
            start,
            end,
        };

        assert!(revealed.reveal(Some(&viewport), &rows, &request(2, 4)));
        assert!(revealed.reveal(Some(&viewport), &rows, &request(4, 6)));
        assert!(!revealed.reveal(Some(&viewport), &rows, &request(3, 5)));
        assert_eq!(revealed.0, [(2, 6)]);
    }

    #[test]
    fn repository_load_revision_is_not_reused_after_state_recreation() {
        let mut app = App::new();
        app.init_resource::<RepositoryLoads>()
            .add_observer(repository_request);
        let page = app.world_mut().spawn(GitState::default()).id();
        let request = GitRepositoryRequest::at("/tmp/repository");

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: request.clone(),
        });
        app.update();
        let first = app
            .world_mut()
            .query::<&RepositoryJob>()
            .iter(app.world())
            .next()
            .unwrap()
            .revision;

        app.world_mut().entity_mut(page).remove::<GitState>();
        app.world_mut().entity_mut(page).insert(GitState::default());
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: request,
        });
        app.update();

        let revisions = app
            .world_mut()
            .query::<&RepositoryJob>()
            .iter(app.world())
            .map(|job| job.revision)
            .collect::<Vec<_>>();
        assert_eq!(revisions.len(), 1);
        assert!(revisions[0] > first);
    }

    fn app_with_change(line_count: u32, changed: usize) -> (App, Entity) {
        let mut lines = (1..=line_count)
            .map(|number| line(DiffKind::Context, number))
            .collect::<Vec<_>>();
        lines[changed].kind = DiffKind::Add;
        let mut app = App::new();
        app.add_plugins(vmux_ecs::UiStatePlugin::<UiGit>::default())
            .add_systems(Update, project_diff);
        let entity = app
            .world_mut()
            .spawn((
                GitState {
                    snapshot: GitPageSnapshot {
                        diff_viewport: Some(GitDiffViewport {
                            generation: 1,
                            revision: 1,
                            first_line: 0,
                            total_lines: line_count,
                            has_more: false,
                            lines,
                            markers: Vec::new(),
                            error: String::new(),
                        }),
                        ..Default::default()
                    },
                },
                GitDiffRevealRanges::default(),
            ))
            .id();
        app.update();
        (app, entity)
    }

    #[test]
    fn diff_projection_collapses_context_outside_changed_hunks() {
        let (app, entity) = app_with_change(20, 9);
        let state = app.world().get::<GitState>(entity).unwrap();

        assert_eq!(
            state.snapshot.diff_rows.first(),
            Some(&GitDiffRow::Gap {
                start: 0,
                end: 6,
                reveal_start: 0,
                reveal_end: 6,
            })
        );
        assert!(state.snapshot.diff_rows.contains(&GitDiffRow::Line(9)));
    }

    #[test]
    fn diff_projection_reveals_only_the_requested_chunk() {
        let (mut app, entity) = app_with_change(60, 49);
        app.world_mut()
            .get_mut::<GitDiffRevealRanges>(entity)
            .unwrap()
            .0
            .push((0, GitDiffRevealRequest::CHUNK as usize));
        app.update();
        let state = app.world().get::<GitState>(entity).unwrap();

        assert!(state.snapshot.diff_rows.contains(&GitDiffRow::Line(0)));
        assert!(state.snapshot.diff_rows.contains(&GitDiffRow::Gap {
            start: GitDiffRevealRequest::CHUNK,
            end: 46,
            reveal_start: GitDiffRevealRequest::CHUNK,
            reveal_end: GitDiffRevealRequest::CHUNK * 2,
        }));
    }

    #[test]
    fn page_ready_preserves_repository_loading_state() {
        let mut app = App::new();
        app.add_observer(page_ready);
        let entity = app
            .world_mut()
            .spawn((
                vmux_ecs::PageMetadata {
                    url: crate::GIT_PAGE_URL.to_string(),
                    ..Default::default()
                },
                GitState {
                    snapshot: GitPageSnapshot {
                        workspace: "/workspace".to_string(),
                        loading: true,
                        ..Default::default()
                    },
                },
            ))
            .id();

        app.world_mut().trigger(UiInput {
            webview: entity,
            payload: PageReady {},
        });
        app.update();

        let state = app.world().get::<GitState>(entity).unwrap();
        assert_eq!(state.snapshot.workspace, "/workspace");
        assert!(state.snapshot.loading);
    }

    #[test]
    fn repository_matches_requested_path_and_discovered_root_only() {
        let request = GitRepositoryRequest::project(
            "/tmp/repository/project".into(),
            "project".into(),
            "main".into(),
        );
        let mut repository = GitRepositorySnapshot::loading(&request);
        repository.repo_root = "/tmp/repository".into();
        let state = GitState {
            snapshot: GitPageSnapshot {
                workspace: "/tmp/repository".into(),
                repository: Some(repository),
                loading: false,
                ..Default::default()
            },
        };

        assert!(state.repository_matches("/tmp/repository/./project"));
        assert!(state.repository_matches("/tmp/repository"));
        assert!(!state.repository_matches("/tmp/repository/other"));
    }

    #[test]
    fn workspace_reset_preserves_the_repository_epoch() {
        let mut state = GitState {
            snapshot: GitPageSnapshot {
                nonce: 7,
                ..Default::default()
            },
        };

        state.reset_workspace("/tmp/repository".to_string());
        state
            .snapshot
            .begin_repository(&GitRepositoryRequest::at("/tmp/repository"));

        assert_eq!(state.snapshot.nonce, 8);
    }

    #[test]
    fn workspace_reset_is_already_loading_before_repository_summary_arrives() {
        let mut state = GitState::default();
        state.reset_workspace("/tmp/repository".to_string());

        assert!(state.repository_loading_at("/tmp/repository"));
    }

    #[test]
    fn workspace_reset_does_not_coalesce_the_first_repository_request() {
        let mut state = GitState::default();
        state.reset_workspace("/tmp/repository".to_string());
        let request = GitRepositoryRequest::at("/tmp/repository");

        assert!(!state.repository_loading_request(&request));
    }

    #[test]
    fn cached_repository_keeps_the_page_renderable_while_refreshing() {
        let request = GitRepositoryRequest::at("/tmp/repository");
        let mut state = GitState::default();
        state.snapshot.begin_repository(&request);
        let nonce = state.snapshot.nonce;
        let mut repository = GitRepositorySnapshot::loading(&request);
        repository.branch = "main".to_string();

        state.prime_repository(repository);

        assert_eq!(state.snapshot.nonce, nonce);
        assert!(state.snapshot.loading);
        assert_eq!(
            state
                .snapshot
                .repository
                .as_ref()
                .map(|repository| repository.branch.as_str()),
            Some("main")
        );
    }

    #[test]
    fn repository_switch_keeps_invalidated_diff_epochs() {
        let commit = "a".repeat(40);
        let mut app = App::new();
        app.init_resource::<RepositoryLoads>()
            .add_observer(repository_request);
        let page = app
            .world_mut()
            .spawn((
                GitState::default(),
                super::super::diff::GitDiffQuery::for_test(7),
                super::super::diff::GitBranchFilesQuery::for_test(11),
            ))
            .id();

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GitRepositoryRequest::at("/tmp/repository").with_commit(Some(commit.clone())),
        });
        app.update();

        let diff = app
            .world()
            .get::<super::super::diff::GitDiffQuery>(page)
            .unwrap();
        let branch = app
            .world()
            .get::<super::super::diff::GitBranchFilesQuery>(page)
            .unwrap();
        assert!(!diff.accepts(7));
        assert!(!branch.accepts(11, "main"));
        let controller = app.world().get::<GitPageControllerState>(page).unwrap();
        assert_eq!(controller.selected_commit, commit);
        assert_eq!(controller.focused_panel, crate::state::GitPanel::Commits);
    }

    #[test]
    fn repository_switch_cancels_read_and_mutation_jobs() {
        let mut app = App::new();
        app.init_resource::<RepositoryLoads>()
            .add_observer(repository_request);
        let page = app.world_mut().spawn(GitState::default()).id();
        app.world_mut().spawn((
            GitJob::new(page),
            super::super::job::StageJob {
                path: PathBuf::from("file.rs"),
            },
        ));
        app.world_mut().spawn((
            GitJob::new(page),
            RepositoryJob {
                path: PathBuf::from("/tmp/old-repository"),
                refresh_catalogs: true,
                revision: 0,
            },
        ));

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GitRepositoryRequest::at("/tmp/repository"),
        });
        app.update();

        let mut mutations = app.world_mut().query::<&super::super::job::StageJob>();
        assert_eq!(mutations.iter(app.world()).count(), 0);
        let mut loads = app.world_mut().query::<&RepositoryJob>();
        assert_eq!(loads.iter(app.world()).count(), 1);
    }

    #[test]
    fn duplicate_repository_request_queues_one_load() {
        let mut app = App::new();
        app.init_resource::<RepositoryLoads>()
            .add_observer(repository_request);
        let page = app.world_mut().spawn(GitState::default()).id();
        let request = GitRepositoryRequest {
            request_id: 7,
            ..GitRepositoryRequest::at("/tmp/repository")
        };

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: request.clone(),
        });
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: request,
        });
        app.update();

        let mut jobs = app.world_mut().query::<&RepositoryJob>();
        assert_eq!(jobs.iter(app.world()).count(), 1);
    }

    #[test]
    fn stale_repository_request_is_rejected() {
        let mut app = App::new();
        app.init_resource::<RepositoryLoads>()
            .add_observer(repository_request);
        let mut state = GitState::default();
        state.snapshot.repository_request_id = 7;
        state.snapshot.workspace = "/tmp/current".to_string();
        let page = app.world_mut().spawn(state).id();

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GitRepositoryRequest {
                request_id: 6,
                ..GitRepositoryRequest::at("/tmp/stale")
            },
        });
        app.update();

        let state = app.world().get::<GitState>(page).unwrap();
        assert_eq!(state.snapshot.repository_request_id, 7);
        assert_eq!(state.snapshot.workspace, "/tmp/current");
        let mut jobs = app.world_mut().query::<&RepositoryJob>();
        assert_eq!(jobs.iter(app.world()).count(), 0);
    }

    #[test]
    fn project_detail_page_owns_git_state() {
        let mut app = App::new();
        app.add_observer(page_ready)
            .add_systems(Update, cleanup_inactive_views);
        let entity = app
            .world_mut()
            .spawn(vmux_ecs::PageMetadata {
                url: "vmux://projects/github.com/vmux-ai/vmux".into(),
                ..Default::default()
            })
            .id();

        app.world_mut().trigger(UiInput {
            webview: entity,
            payload: PageReady {},
        });
        app.update();

        assert!(app.world().entity(entity).contains::<GitState>());
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<vmux_ecs::PageMetadata>()
            .unwrap()
            .title = "vmux".into();
        app.update();
        assert!(app.world().entity(entity).contains::<GitState>());
    }

    #[test]
    fn inactive_git_page_queues_repository_work_until_activation() {
        let repo = test_repo::init();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<RepositoryLoads>()
            .add_observer(repository_request);
        app.insert_non_send(GitWatch::test());
        let page = app
            .world_mut()
            .spawn((vmux_ecs::PageMetadata::default(), GitState::default()))
            .id();

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GitRepositoryRequest::at(repo.path().to_string_lossy().into_owned()),
        });
        app.update();

        let state = app.world().get::<GitState>(page).unwrap();
        assert_eq!(state.workspace(), repo.path().to_string_lossy());
        assert!(state.snapshot.loading);
        assert!(!app.world().non_send::<GitWatch>().has_subscription(page));
        let mut jobs = app.world_mut().query::<&RepositoryJob>();
        assert_eq!(jobs.iter(app.world()).count(), 1);
    }

    #[test]
    fn repository_status_refresh_advances_diff_epoch_and_preserves_loaded_catalogs() {
        let repo = test_repo::init();
        test_repo::write(repo.path(), "file.txt", "content\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "base"]);
        let repository = GitRepositorySnapshot::load_summary(repo.path())
            .unwrap()
            .snapshot;
        let mut state = GitState {
            snapshot: GitPageSnapshot {
                nonce: 7,
                repository: Some(repository),
                local_branches: GitCatalogProgress {
                    loaded: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        };
        let next = GitRepositorySnapshot::load_summary(repo.path()).unwrap();

        state.replace_repository(next.snapshot, false);

        assert_eq!(state.snapshot.nonce, 8);
        assert!(state.snapshot.local_branches.loaded);
    }

    #[test]
    fn repository_ref_refresh_preserves_initialized_catalogs() {
        let repo = test_repo::init();
        test_repo::write(repo.path(), "file.txt", "content\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "base"]);
        let repository = GitRepositorySnapshot::load_summary(repo.path())
            .unwrap()
            .snapshot;
        let mut state = GitState {
            snapshot: GitPageSnapshot {
                repository: Some(repository),
                local_branches: GitCatalogProgress {
                    initialized: true,
                    loaded: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        };
        let next = GitRepositorySnapshot::load_summary(repo.path()).unwrap();

        state.replace_repository(next.snapshot, true);

        assert!(!state.snapshot.local_branches.loaded);
        assert!(state.snapshot.local_branches.initialized);
    }
}
