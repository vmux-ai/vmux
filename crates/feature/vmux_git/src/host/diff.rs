use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::Arc;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::event::{
    DiffKind, DiffLine, GitBranchFilesNextRequest, GitBranchFilesRetryRequest, GitDiffNextRequest,
    GitDiffRetryRequest, GitDiffRevealRequest, GitLineMarker, GitLineStatus,
};
use crate::state::{
    GitBranchFilesPagePatch, GitBranchFilesPatch, GitPageControllerState, GitPanel, UiGit,
};

use super::GitUpdateSet;
use super::job::{BranchFilesJob, DiffJob, DiffScope, GitForegroundRead};
use super::job_runner::{GitJob, QueuedGitJobs};
use super::repository::{BranchFilesCatalog, BranchFilesPage, DiffSource, GitRepository};

const DIFF_WINDOW_ROWS: u32 = 640;
const BRANCH_PATH_BATCH_SIZE: usize = 4;
const PREFETCH_BATCHES: u8 = 4;
const PREFETCH_ROWS: u32 = DIFF_WINDOW_ROWS * PREFETCH_BATCHES as u32;

pub(super) struct DiffPlugin;

impl Plugin for DiffPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            GitDiffRevealRequest,
            GitDiffNextRequest,
            GitDiffRetryRequest,
            GitBranchFilesNextRequest,
            GitBranchFilesRetryRequest,
        )>::default())
            .add_observer(file_refresh)
            .add_observer(reveal)
            .add_observer(load_next)
            .add_observer(retry)
            .add_observer(load_next_branch_files)
            .add_observer(retry_branch_files)
            .add_systems(
                Update,
                (
                    sync_branch_files,
                    sync_page_targets,
                    start_requests,
                    start_pending_seek,
                )
                    .chain()
                    .in_set(GitUpdateSet::Diff),
            );
    }
}

fn reveal(
    trigger: On<UiInput<GitDiffRevealRequest>>,
    mut states: Query<(
        &mut super::state::GitState,
        &mut super::state::GitDiffRevealRanges,
    )>,
    mut files: super::status::FileGitStates,
) {
    let request = &trigger.event().payload;
    let entity = trigger.event().webview;
    if let Ok((state, mut revealed)) = states.get_mut(entity) {
        revealed.reveal(
            state.snapshot.diff_viewport.as_ref(),
            &state.snapshot.diff_rows,
            request,
        );
        return;
    }
    files.reveal_diff(entity, request);
}

#[derive(Component, Clone, Debug, Default)]
pub struct GitDiffSource {
    pub content: String,
    pub dirty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GitDiffTarget {
    repo_root: PathBuf,
    path: PathBuf,
    path_bytes: Vec<u8>,
    scope: DiffScope,
}

impl GitDiffTarget {
    fn for_page(
        state: &super::state::GitState,
        controller: &GitPageControllerState,
    ) -> Option<Self> {
        state.repository()?;
        let repo_root = PathBuf::from(state.workspace());
        if repo_root.as_os_str().is_empty() {
            return None;
        }
        match controller.focused_panel {
            GitPanel::Branches if !controller.selected_branch.is_empty() => Some(Self {
                repo_root,
                path: PathBuf::new(),
                path_bytes: Vec::new(),
                scope: DiffScope::Branch(controller.selected_branch.clone()),
            }),
            GitPanel::Commits => Some(Self {
                repo_root,
                path: PathBuf::new(),
                path_bytes: Vec::new(),
                scope: DiffScope::Commit(controller.selected_commit.clone()),
            }),
            GitPanel::Stash if !controller.selected_stash.is_empty() => Some(Self {
                repo_root,
                path: PathBuf::new(),
                path_bytes: Vec::new(),
                scope: DiffScope::Stash(controller.selected_stash.clone()),
            }),
            GitPanel::Files if !controller.selected_abs_path.is_empty() => Some(Self {
                repo_root,
                path: PathBuf::from(&controller.selected_abs_path),
                path_bytes: controller.selected_path_bytes.clone(),
                scope: DiffScope::File,
            }),
            _ => None,
        }
    }

    fn for_file(file: &super::status::FileGit) -> Option<Self> {
        let repo_root = file.repo_root()?;
        Some(Self {
            repo_root,
            path: file.path().into(),
            path_bytes: Vec::new(),
            scope: DiffScope::File,
        })
    }
}

#[derive(Component)]
pub(super) struct PendingGitDiff {
    target: GitDiffTarget,
    file: Option<FileDiffIdentity>,
    page_revision: Option<u32>,
}

#[derive(Component)]
pub(super) struct PendingGitDiffSeek {
    path_bytes: Vec<u8>,
}

#[derive(Clone, Copy)]
struct FileDiffIdentity {
    document: u64,
    refresh: u64,
}

#[derive(Component, Clone)]
pub(super) struct GitDiffQuery {
    target: GitDiffTarget,
    generation: u64,
    revision: u64,
    file: Option<FileDiffIdentity>,
    page_revision: Option<u32>,
    branch: Option<BranchDiffProgress>,
    cached_source: Option<DiffSource>,
    prefetch: PrefetchBudget,
    loading: bool,
}

#[derive(Clone, Copy, Default)]
struct PrefetchBudget {
    batches: u8,
    rows: u32,
}

impl PrefetchBudget {
    fn arm(&mut self) {
        if self.batches == 0 || self.rows == 0 {
            self.restart();
        }
    }

    fn restart(&mut self) {
        self.batches = PREFETCH_BATCHES;
        self.rows = PREFETCH_ROWS;
    }

    fn continue_after(&mut self, rows: u32) -> bool {
        self.batches = self.batches.saturating_sub(1);
        self.rows = self.rows.saturating_sub(rows);
        self.batches > 0 && self.rows > 0
    }
}

#[derive(Clone)]
struct BranchDiffProgress {
    catalog: BranchFilesCatalog,
    loaded: HashSet<Vec<u8>>,
    in_flight: HashSet<Vec<u8>>,
    continuations: VecDeque<BranchDiffContinuation>,
    next: usize,
}

#[derive(Clone)]
struct BranchDiffContinuation {
    paths: Vec<Vec<u8>>,
    top_line: u32,
    document: Option<DiffSource>,
}

#[derive(Default)]
struct BranchDiffBatch {
    source: PathBuf,
    merge_base: String,
    paths: Vec<Vec<u8>>,
    document: Option<DiffSource>,
    top_line: u32,
    has_more: bool,
}

impl BranchDiffProgress {
    fn new(catalog: BranchFilesCatalog) -> Self {
        Self {
            catalog,
            loaded: HashSet::new(),
            in_flight: HashSet::new(),
            continuations: VecDeque::new(),
            next: 0,
        }
    }

    fn next(&mut self) -> BranchDiffBatch {
        if let Some(continuation) = self.continuations.pop_front() {
            self.in_flight.extend(continuation.paths.iter().cloned());
            return BranchDiffBatch {
                source: self.catalog.source().to_path_buf(),
                merge_base: self.catalog.merge_base().to_string(),
                paths: continuation.paths,
                document: continuation.document,
                top_line: continuation.top_line,
                has_more: true,
            };
        }
        let mut paths = Vec::with_capacity(BRANCH_PATH_BATCH_SIZE);
        while paths.len() < BRANCH_PATH_BATCH_SIZE {
            self.advance();
            let Some(path) = self.catalog.path(self.next).map(<[u8]>::to_vec) else {
                break;
            };
            self.next += 1;
            if self.in_flight.insert(path.clone()) {
                paths.push(path);
            }
        }
        if paths.is_empty() {
            return BranchDiffBatch::default();
        }
        self.advance();
        BranchDiffBatch {
            source: self.catalog.source().to_path_buf(),
            merge_base: self.catalog.merge_base().to_string(),
            paths,
            document: None,
            top_line: 0,
            has_more: self.has_more(),
        }
    }

    fn seek(&mut self, path: &[u8]) -> Option<BranchDiffBatch> {
        if !self.catalog.contains(path)
            || self.loaded.contains(path)
            || !self.in_flight.insert(path.to_vec())
        {
            return None;
        }
        Some(BranchDiffBatch {
            source: self.catalog.source().to_path_buf(),
            merge_base: self.catalog.merge_base().to_string(),
            paths: vec![path.to_vec()],
            document: None,
            top_line: 0,
            has_more: self.has_more(),
        })
    }

    fn contains(&self, path: &[u8]) -> bool {
        self.loaded.contains(path)
            || self.in_flight.contains(path)
            || self
                .continuations
                .iter()
                .any(|continuation| continuation.paths.iter().any(|candidate| candidate == path))
    }

    fn complete(
        &mut self,
        paths: &[Vec<u8>],
        top_line: u32,
        loaded_lines: u32,
        page_complete: bool,
        succeeded: bool,
        document: Option<DiffSource>,
    ) {
        for path in paths {
            self.in_flight.remove(path);
            if succeeded && page_complete {
                self.loaded.insert(path.clone());
            }
        }
        if !succeeded || !page_complete {
            self.continuations.push_back(BranchDiffContinuation {
                paths: paths.to_vec(),
                top_line: if succeeded {
                    top_line.saturating_add(loaded_lines)
                } else {
                    top_line
                },
                document,
            });
        }
        self.advance();
    }

    fn has_more(&self) -> bool {
        !self.continuations.is_empty()
            || (self.next..self.catalog.len()).any(|index| {
                self.catalog.path(index).is_some_and(|path| {
                    !self.loaded.contains(path) && !self.in_flight.contains(path)
                })
            })
    }

    fn advance(&mut self) {
        while self.next < self.catalog.len()
            && self
                .catalog
                .path(self.next)
                .is_some_and(|path| self.loaded.contains(path) || self.in_flight.contains(path))
        {
            self.next += 1;
        }
    }

    fn park(&mut self) {
        self.in_flight.clear();
        self.next = 0;
        self.advance();
    }
}

#[derive(Component, Clone)]
pub(super) struct GitBranchFilesQuery {
    repository: PathBuf,
    reference: String,
    generation: u64,
    page_revision: u32,
    catalog: BranchFilesCatalog,
    ready: bool,
}

impl GitBranchFilesQuery {
    const PAGE_SIZE: usize = 80;

    pub(super) fn accepts(&self, generation: u64, reference: &str) -> bool {
        self.generation == generation && self.reference == reference
    }

    pub(super) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.ready = false;
    }

    fn current(&self, repository: &Path, reference: &str, page_revision: u32) -> bool {
        self.repository == repository
            && self.reference == reference
            && self.page_revision == page_revision
    }

    pub(super) fn replace(&mut self, catalog: BranchFilesCatalog) -> BranchFilesPage {
        self.catalog = catalog;
        self.ready = true;
        self.catalog.page(0, Self::PAGE_SIZE)
    }

    fn next(&self, offset: usize) -> BranchFilesPage {
        self.catalog.page(offset, Self::PAGE_SIZE)
    }

    #[cfg(test)]
    pub(super) fn for_test(generation: u64) -> Self {
        Self {
            repository: PathBuf::from("/repo"),
            reference: "main".to_string(),
            generation,
            page_revision: 1,
            catalog: BranchFilesCatalog::default(),
            ready: false,
        }
    }
}

impl GitDiffQuery {
    pub(super) fn accepts(&self, generation: u64) -> bool {
        self.generation == generation
    }

    pub(super) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.cached_source = None;
        self.loading = false;
    }

    pub(super) fn parked(&self) -> Self {
        let mut query = self.clone();
        query.loading = false;
        if let Some(branch) = query.branch.as_mut() {
            branch.park();
        }
        query
    }

    pub(super) fn accepts_file(&self, generation: u64, file: &super::status::FileGit) -> bool {
        self.accepts(generation)
            && self
                .file
                .is_some_and(|identity| file.accepts(identity.document, identity.refresh))
    }

    pub(super) fn complete_load(&mut self) -> u64 {
        self.loading = false;
        self.revision = self.revision.wrapping_add(1).max(1);
        self.revision
    }

    pub(super) fn continue_prefetch(&mut self, rows: u32) -> bool {
        self.prefetch.continue_after(rows)
    }

    fn next_branch(&mut self) -> Option<BranchDiffBatch> {
        self.branch.as_mut().map(BranchDiffProgress::next)
    }

    fn seek_branch(&mut self, path: &[u8]) -> Option<BranchDiffBatch> {
        self.branch.as_mut()?.seek(path)
    }

    fn contains_branch_path(&self, path: &[u8]) -> bool {
        self.branch
            .as_ref()
            .is_some_and(|branch| branch.contains(path))
    }

    #[cfg(test)]
    pub(super) fn for_test(generation: u64) -> Self {
        Self {
            target: GitDiffTarget {
                repo_root: PathBuf::from("/repo"),
                path: PathBuf::from("/repo/file.rs"),
                path_bytes: Vec::new(),
                scope: DiffScope::File,
            },
            generation,
            revision: 0,
            file: None,
            page_revision: None,
            branch: None,
            cached_source: None,
            prefetch: PrefetchBudget::default(),
            loading: false,
        }
    }

    pub(super) fn complete_branch(
        &mut self,
        paths: &[Vec<u8>],
        top_line: u32,
        loaded_lines: u32,
        page_complete: bool,
        succeeded: bool,
        source: Option<DiffSource>,
    ) -> bool {
        let Some(branch) = self.branch.as_mut() else {
            return false;
        };
        branch.complete(
            paths,
            top_line,
            loaded_lines,
            page_complete,
            succeeded,
            source,
        );
        branch.has_more()
    }

    pub(super) fn cache_source(&mut self, source: Option<DiffSource>, has_more: bool) {
        if !matches!(self.target.scope, DiffScope::Branch(_)) {
            self.cached_source = has_more.then_some(source).flatten();
        }
    }
}

#[derive(EntityEvent)]
pub(super) struct FileDiffRefresh {
    #[event_target]
    pub(super) entity: Entity,
}

type GitDiffPages<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static vmux_ecs::PageMetadata,
        Mut<'static, super::state::GitState>,
        Ref<'static, GitPageControllerState>,
        &'static mut super::state::GitDiffRevealRanges,
        Ref<'static, super::state::GitRepositoryRevision>,
        Option<Ref<'static, GitBranchFilesQuery>>,
        Option<&'static PendingGitDiff>,
        Option<&'static GitDiffQuery>,
    ),
>;

fn sync_page_targets(
    mut pages: GitDiffPages,
    queued_diffs: QueuedGitJobs<DiffJob>,
    mut commands: Commands,
) {
    for (
        entity,
        page,
        mut state,
        controller,
        mut revealed,
        repository_revision,
        branch_files,
        pending,
        current,
    ) in &mut pages
    {
        if !crate::GitUrl::is_page(&page.url) {
            continue;
        }
        if !state.is_changed()
            && !controller.is_changed()
            && !repository_revision.is_changed()
            && branch_files
                .as_ref()
                .is_none_or(|query| !query.is_changed())
        {
            continue;
        }
        let Some(target) = GitDiffTarget::for_page(&state, &controller) else {
            queued_diffs.despawn_for(entity, &mut commands);
            if current.is_some() || pending.is_some() || state.snapshot.diff_viewport.is_some() {
                state.snapshot.diff_loading = false;
                state.snapshot.diff_viewport = None;
                state.snapshot.diff_rows.clear();
                revealed.0.clear();
            }
            commands
                .entity(entity)
                .remove::<(GitDiffQuery, PendingGitDiff, PendingGitDiffSeek)>();
            continue;
        };
        let page_revision = state.diff_revision();
        let target_changed = current.is_some_and(|current| current.target != target)
            || pending.is_some_and(|pending| pending.target != target);
        if target_changed {
            queued_diffs.despawn_for(entity, &mut commands);
            state.snapshot.diff_loading = true;
            state.snapshot.diff_viewport = None;
            state.snapshot.diff_rows.clear();
            revealed.0.clear();
            commands
                .entity(entity)
                .remove::<(GitDiffQuery, PendingGitDiff, PendingGitDiffSeek)>();
        }
        if let DiffScope::Branch(reference) = &target.scope
            && branch_files.as_ref().is_none_or(|query| {
                !query.ready || !query.current(&target.repo_root, reference, page_revision)
            })
        {
            continue;
        }
        if pending.is_some_and(|pending| {
            pending.target == target && pending.page_revision == Some(page_revision)
        }) || current.is_some_and(|current| {
            current.target == target && current.page_revision == Some(page_revision)
        }) {
            if !controller.selected_branch_path_bytes.is_empty()
                && current.is_some_and(|current| {
                    !current.contains_branch_path(&controller.selected_branch_path_bytes)
                })
            {
                commands.entity(entity).insert(PendingGitDiffSeek {
                    path_bytes: controller.selected_branch_path_bytes.clone(),
                });
            }
            continue;
        }
        commands.entity(entity).insert(PendingGitDiff {
            target,
            file: None,
            page_revision: Some(page_revision),
        });
    }
}

fn sync_branch_files(
    mut pages: Query<(
        Entity,
        &mut super::state::GitState,
        &GitPageControllerState,
        Option<&GitBranchFilesQuery>,
    )>,
    mut ui_state: vmux_ecs::UiStateWriter<UiGit>,
    queued_jobs: QueuedGitJobs<BranchFilesJob>,
    mut commands: Commands,
) {
    for (entity, mut state, controller, current) in &mut pages {
        if controller.focused_panel != GitPanel::Branches || controller.selected_branch.is_empty() {
            continue;
        }
        let repo_root = PathBuf::from(state.workspace());
        if repo_root.as_os_str().is_empty() {
            continue;
        }
        let page_revision = state.diff_revision();
        if current.is_some_and(|current| {
            current.current(&repo_root, &controller.selected_branch, page_revision)
        }) {
            continue;
        }
        let generation = current
            .map(|current| current.generation)
            .unwrap_or_default()
            .wrapping_add(1)
            .max(1);
        let patch = GitBranchFilesPatch {
            reference: controller.selected_branch.clone(),
            base_ref: None,
            diff_paths: None,
            page: None,
            diff_loading: Some(true),
            loading: true,
            error: String::new(),
        };
        state
            .bypass_change_detection()
            .snapshot
            .apply_branch_files_patch(&patch);
        ui_state.push(entity, patch);
        queued_jobs.despawn_for(entity, &mut commands);
        commands.entity(entity).insert(GitBranchFilesQuery {
            repository: repo_root.clone(),
            reference: controller.selected_branch.clone(),
            generation,
            page_revision,
            catalog: BranchFilesCatalog::default(),
            ready: false,
        });
        commands.spawn((
            GitJob::new(entity),
            GitForegroundRead,
            GitRepository::at(repo_root),
            BranchFilesJob {
                reference: controller.selected_branch.clone(),
                generation,
            },
        ));
    }
}

fn load_next_branch_files(
    trigger: On<UiInput<GitBranchFilesNextRequest>>,
    mut pages: Query<(
        &mut super::state::GitState,
        &GitPageControllerState,
        &mut GitBranchFilesQuery,
    )>,
    mut ui_state: vmux_ecs::UiStateWriter<UiGit>,
) {
    let entity = trigger.event().webview;
    let Ok((mut state, controller, query)) = pages.get_mut(entity) else {
        return;
    };
    if state.snapshot.branch_files_loading
        || !state.snapshot.branch_files_has_more
        || query.reference != controller.selected_branch
    {
        return;
    }
    let offset = state.snapshot.branch_files.len();
    let page = query.next(offset);
    let patch = GitBranchFilesPatch {
        reference: query.reference.clone(),
        base_ref: None,
        diff_paths: None,
        page: Some(GitBranchFilesPagePatch {
            offset: u32::try_from(offset).unwrap_or(u32::MAX),
            files: page.files,
            total: page.total,
            has_more: page.has_more,
        }),
        diff_loading: None,
        loading: false,
        error: String::new(),
    };
    state
        .bypass_change_detection()
        .snapshot
        .apply_branch_files_patch(&patch);
    ui_state.push(entity, patch);
}

fn retry_branch_files(
    trigger: On<UiInput<GitBranchFilesRetryRequest>>,
    mut states: Query<&mut super::state::GitState>,
    mut ui_state: vmux_ecs::UiStateWriter<UiGit>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(mut state) = states.get_mut(entity) else {
        return;
    };
    if state.snapshot.branch_files_error.is_empty() {
        return;
    }
    let patch = GitBranchFilesPatch {
        reference: state.snapshot.branch_files_reference.clone(),
        base_ref: None,
        diff_paths: None,
        page: None,
        diff_loading: Some(true),
        loading: false,
        error: String::new(),
    };
    state
        .bypass_change_detection()
        .snapshot
        .apply_branch_files_patch(&patch);
    ui_state.push(entity, patch);
    commands.entity(entity).remove::<GitBranchFilesQuery>();
}

fn load_next(
    trigger: On<UiInput<GitDiffNextRequest>>,
    mut pages: Query<(&super::state::GitState, &mut GitDiffQuery)>,
    sources: Query<&GitDiffSource>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((state, mut query)) = pages.get_mut(entity) else {
        return;
    };
    let Some(viewport) = state.snapshot.diff_viewport.as_ref() else {
        return;
    };
    let request = &trigger.event().payload;
    if request.generation != viewport.generation
        || request.revision != viewport.revision
        || request.generation != query.generation
    {
        return;
    }
    let loaded = viewport
        .first_line
        .saturating_add(u32::try_from(viewport.lines.len()).unwrap_or(u32::MAX));
    if query.loading || !viewport.has_more {
        return;
    }
    let content = sources
        .get(entity)
        .ok()
        .filter(|source| source.dirty)
        .map(|source| source.content.clone());
    query.prefetch.arm();
    query.loading = true;
    let batch = query.next_branch().unwrap_or_default();
    commands.spawn((
        GitJob::new(entity),
        GitForegroundRead,
        GitRepository::at(query.target.repo_root.clone()),
        DiffJob {
            path: query.target.path.clone(),
            scope: query.target.scope.clone(),
            branch_paths: batch.paths,
            cached_source: batch.document.or_else(|| query.cached_source.clone()),
            branch_source: batch.source,
            branch_merge_base: batch.merge_base,
            branch_top_line: batch.top_line,
            branch_has_more: batch.has_more,
            generation: query.generation,
            top_line: loaded,
            rows: DIFF_WINDOW_ROWS,
            content,
        },
    ));
}

fn retry(
    trigger: On<UiInput<GitDiffRetryRequest>>,
    mut pages: Query<(
        &mut super::state::GitState,
        &GitDiffQuery,
        &mut super::state::GitDiffRevealRanges,
    )>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((mut state, query, _)) = pages.get_mut(entity) else {
        return;
    };
    state.snapshot.diff_loading = true;
    commands.entity(entity).insert(PendingGitDiff {
        target: query.target.clone(),
        file: query.file,
        page_revision: query.page_revision,
    });
}

fn start_pending_seek(
    mut pages: Query<(
        Entity,
        &super::state::GitState,
        &mut GitDiffQuery,
        &PendingGitDiffSeek,
    )>,
    mut commands: Commands,
) {
    for (entity, state, mut query, pending) in &mut pages {
        if query.loading || !matches!(query.target.scope, DiffScope::Branch(_)) {
            continue;
        }
        let loaded = state
            .snapshot
            .diff_viewport
            .as_ref()
            .map(|viewport| {
                viewport
                    .first_line
                    .saturating_add(u32::try_from(viewport.lines.len()).unwrap_or(u32::MAX))
            })
            .unwrap_or_default();
        let batch = query.seek_branch(&pending.path_bytes);
        commands.entity(entity).remove::<PendingGitDiffSeek>();
        let Some(batch) = batch else {
            continue;
        };
        query.prefetch.restart();
        query.loading = true;
        commands.spawn((
            GitJob::new(entity),
            GitForegroundRead,
            GitRepository::at(query.target.repo_root.clone()),
            DiffJob {
                path: query.target.path.clone(),
                scope: query.target.scope.clone(),
                branch_paths: batch.paths,
                cached_source: batch.document.or_else(|| query.cached_source.clone()),
                branch_source: batch.source,
                branch_merge_base: batch.merge_base,
                branch_top_line: batch.top_line,
                branch_has_more: batch.has_more,
                generation: query.generation,
                top_line: loaded,
                rows: DIFF_WINDOW_ROWS,
                content: None,
            },
        ));
    }
}

fn file_refresh(
    trigger: On<FileDiffRefresh>,
    files: Query<&super::status::FileGit>,
    mut commands: Commands,
) {
    let Ok(file) = files.get(trigger.entity) else {
        return;
    };
    let Some(target) = GitDiffTarget::for_file(file) else {
        return;
    };
    let (document, refresh) = file.identity();
    commands.entity(trigger.entity).insert(PendingGitDiff {
        target,
        file: Some(FileDiffIdentity { document, refresh }),
        page_revision: None,
    });
}

type PendingDiffRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PendingGitDiff,
        Option<&'static GitDiffQuery>,
        Option<&'static GitDiffSource>,
        Option<&'static GitBranchFilesQuery>,
    ),
>;

fn start_requests(
    pending: PendingDiffRequests,
    mut pages: Query<(
        &mut super::state::GitState,
        &mut super::state::GitDiffRevealRanges,
    )>,
    mut files: super::status::FileGitStates,
    queued_diffs: QueuedGitJobs<DiffJob>,
    mut commands: Commands,
) {
    for (entity, pending, current, source, branch_files) in &pending {
        let target_changed = current.is_none_or(|current| current.target != pending.target);
        let mut generation = current
            .map(|current| current.generation)
            .unwrap_or_default();
        if let Ok((mut page, mut revealed)) = pages.get_mut(entity) {
            if let Some(viewport) = page.snapshot.diff_viewport.as_ref() {
                generation = generation.max(viewport.generation);
            }
            page.snapshot.diff_loading = true;
            if target_changed {
                page.snapshot.diff_viewport = None;
                page.snapshot.diff_rows.clear();
                revealed.0.clear();
            }
        } else if files.contains(entity) {
            files.start_diff(entity, target_changed);
        } else {
            commands.entity(entity).remove::<PendingGitDiff>();
            continue;
        }
        generation = generation.wrapping_add(1).max(1);
        let target = pending.target.clone();
        let content = source
            .filter(|source| source.dirty)
            .map(|source| source.content.clone());
        queued_diffs.despawn_for(entity, &mut commands);
        let branch = match &target.scope {
            DiffScope::Branch(_) => {
                branch_files.map(|query| BranchDiffProgress::new(query.catalog.clone()))
            }
            DiffScope::File | DiffScope::Commit(_) | DiffScope::Stash(_) => None,
        };
        let mut query = GitDiffQuery {
            target: target.clone(),
            generation,
            revision: 0,
            file: pending.file,
            page_revision: pending.page_revision,
            branch,
            cached_source: None,
            prefetch: PrefetchBudget::default(),
            loading: true,
        };
        query.prefetch.restart();
        let batch = query.next_branch().unwrap_or_default();
        commands
            .entity(entity)
            .remove::<PendingGitDiff>()
            .insert(query);
        commands.spawn((
            GitJob::new(entity),
            GitForegroundRead,
            GitRepository::at(target.repo_root),
            DiffJob {
                path: target.path,
                scope: target.scope,
                branch_paths: batch.paths,
                cached_source: batch.document,
                branch_source: batch.source,
                branch_merge_base: batch.merge_base,
                branch_top_line: batch.top_line,
                branch_has_more: batch.has_more,
                generation,
                top_line: 0,
                rows: DIFF_WINDOW_ROWS,
                content,
            },
        ));
    }
}

pub(super) struct GitDiffMarkers(Vec<GitLineMarker>);

impl GitDiffMarkers {
    pub(super) fn from_lines(lines: &[DiffLine]) -> Self {
        let mut markers = HashMap::new();
        let mut hunk_kinds = HashMap::<u32, (bool, bool)>::new();
        for line in lines {
            let Some(hunk) = line.hunk else {
                continue;
            };
            let kinds = hunk_kinds.entry(hunk).or_default();
            match line.kind {
                DiffKind::Add => kinds.0 = true,
                DiffKind::Remove => kinds.1 = true,
                _ => {}
            }
        }
        let mut replacement_lines = HashSet::new();
        let mut index = 0;
        while index < lines.len() {
            if matches!(lines[index].kind, DiffKind::Context | DiffKind::Staged)
                || lines[index].hunk.is_some()
            {
                index += 1;
                continue;
            }
            let start = index;
            while index < lines.len()
                && !matches!(lines[index].kind, DiffKind::Context | DiffKind::Staged)
                && lines[index].hunk.is_none()
            {
                index += 1;
            }
            let range = start..index;
            let has_add = range
                .clone()
                .any(|line| matches!(lines[line].kind, DiffKind::Add));
            let has_remove = range
                .clone()
                .any(|line| matches!(lines[line].kind, DiffKind::Remove));
            if has_add && has_remove {
                replacement_lines.extend(range);
            }
        }

        for (index, line) in lines.iter().enumerate() {
            match line.kind {
                DiffKind::Add => {
                    let Some(line_number) = line.new_no else {
                        continue;
                    };
                    let modified = line
                        .hunk
                        .and_then(|hunk| hunk_kinds.get(&hunk))
                        .is_some_and(|(added, removed)| *added && *removed)
                        || replacement_lines.contains(&index);
                    Self::insert(
                        &mut markers,
                        line_number,
                        if modified {
                            GitLineStatus::Modified
                        } else {
                            GitLineStatus::Added
                        },
                    );
                }
                DiffKind::Remove => {
                    let next = lines[index + 1..].iter().find_map(|next| next.new_no);
                    let previous = lines[..index]
                        .iter()
                        .rev()
                        .find_map(|previous| previous.new_no);
                    if let Some(line_number) = next.or(previous) {
                        Self::insert(&mut markers, line_number, GitLineStatus::Deleted);
                    }
                }
                DiffKind::Staged => {
                    if let Some(line_number) = line.new_no {
                        Self::insert(&mut markers, line_number, GitLineStatus::Staged);
                    }
                }
                DiffKind::Context | DiffKind::Hunk => {}
            }
        }
        let mut markers = markers
            .into_iter()
            .map(|(line, status)| GitLineMarker { line, status })
            .collect::<Vec<_>>();
        markers.sort_by_key(|marker| marker.line);
        Self(markers)
    }

    pub(super) fn into_inner(self) -> Vec<GitLineMarker> {
        self.0
    }

    fn insert(markers: &mut HashMap<u32, GitLineStatus>, line: u32, status: GitLineStatus) {
        let priority = |status| match status {
            GitLineStatus::Staged => 0,
            GitLineStatus::Deleted => 1,
            GitLineStatus::Added => 2,
            GitLineStatus::Modified => 3,
        };
        match markers.get(&line) {
            Some(current) if priority(*current) >= priority(status) => {}
            _ => {
                markers.insert(line, status);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{GitDiffViewport, StyledSpan};
    use crate::host::repository::test_repo;
    use crate::host::runtime::GitRuntime;

    fn line(kind: DiffKind, no: u32) -> DiffLine {
        DiffLine {
            kind,
            old_no: Some(no),
            new_no: Some(no),
            hunk: None,
            spans: Vec::<StyledSpan>::new(),
        }
    }

    #[test]
    fn a_new_diff_request_replaces_the_queued_request() {
        let repo = tempfile::tempdir().unwrap();
        let target = GitDiffTarget {
            repo_root: repo.path().to_path_buf(),
            path: PathBuf::new(),
            path_bytes: Vec::new(),
            scope: DiffScope::Branch("feature".into()),
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, start_requests);
        let page = app
            .world_mut()
            .spawn((
                super::super::state::GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        diff_viewport: Some(GitDiffViewport {
                            generation: 1,
                            revision: 1,
                            first_line: 0,
                            total_lines: 1,
                            has_more: false,
                            lines: vec![line(DiffKind::Context, 1)],
                            markers: Vec::new(),
                            error: String::new(),
                        }),
                        diff_rows: vec![crate::event::GitDiffRow::Line(0)],
                        ..Default::default()
                    },
                },
                PendingGitDiff {
                    target: target.clone(),
                    file: None,
                    page_revision: Some(2),
                },
            ))
            .id();
        let queued = app
            .world_mut()
            .spawn((
                GitJob::new(page),
                GitRepository::at(repo.path()),
                DiffJob {
                    path: PathBuf::new(),
                    scope: target.scope,
                    branch_paths: Vec::new(),
                    cached_source: None,
                    branch_source: PathBuf::new(),
                    branch_merge_base: String::new(),
                    branch_top_line: 0,
                    branch_has_more: false,
                    generation: 1,
                    top_line: 0,
                    rows: DIFF_WINDOW_ROWS,
                    content: None,
                },
            ))
            .id();

        app.update();

        assert!(!app.world().entities().contains(queued));
        let state = app
            .world()
            .get::<super::super::state::GitState>(page)
            .unwrap();
        assert!(state.snapshot.diff_viewport.is_none());
        assert!(state.snapshot.diff_rows.is_empty());
        let mut jobs = app
            .world_mut()
            .query_filtered::<&DiffJob, With<GitForegroundRead>>();
        assert_eq!(jobs.iter(app.world()).count(), 1);
    }

    #[test]
    fn branch_change_clears_and_cancels_the_previous_diff_before_files_are_ready() {
        let repo = test_repo::init();
        let repo_root = repo.path().to_path_buf();
        let request =
            crate::event::GitRepositoryRequest::at(repo.path().to_string_lossy().into_owned());
        let mut repository = crate::event::GitRepositorySnapshot::loading(&request);
        repository.repo_root = repo.path().to_string_lossy().into_owned();
        repository.path.clone_from(&repository.repo_root);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<GitRuntime>()
            .add_systems(Update, sync_page_targets);
        let page = app
            .world_mut()
            .spawn((
                vmux_ecs::PageMetadata {
                    url: crate::GitUrl::from_path(repo.path()).unwrap(),
                    ..Default::default()
                },
                super::super::state::GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        workspace: repository.repo_root.clone(),
                        repository: Some(repository),
                        diff_viewport: Some(GitDiffViewport {
                            generation: 1,
                            revision: 1,
                            first_line: 0,
                            total_lines: 1,
                            has_more: false,
                            lines: vec![line(DiffKind::Context, 1)],
                            markers: Vec::new(),
                            error: String::new(),
                        }),
                        diff_rows: vec![crate::event::GitDiffRow::Line(0)],
                        nonce: 1,
                        ..Default::default()
                    },
                },
                GitPageControllerState {
                    focused_panel: GitPanel::Branches,
                    selected_branch: "new".to_string(),
                    ..Default::default()
                },
                super::super::state::GitDiffRevealRanges::default(),
                GitDiffQuery {
                    target: GitDiffTarget {
                        repo_root: repo_root.clone(),
                        path: PathBuf::new(),
                        path_bytes: Vec::new(),
                        scope: DiffScope::Branch("old".to_string()),
                    },
                    generation: 1,
                    revision: 1,
                    file: None,
                    page_revision: Some(1),
                    branch: None,
                    cached_source: None,
                    prefetch: PrefetchBudget::default(),
                    loading: true,
                },
            ))
            .id();
        let job = app
            .world_mut()
            .spawn((
                GitJob::new(page),
                GitRepository::at(repo_root),
                DiffJob {
                    path: PathBuf::new(),
                    scope: DiffScope::Branch("old".to_string()),
                    branch_paths: Vec::new(),
                    cached_source: None,
                    branch_source: PathBuf::new(),
                    branch_merge_base: String::new(),
                    branch_top_line: 0,
                    branch_has_more: false,
                    generation: 1,
                    top_line: 0,
                    rows: DIFF_WINDOW_ROWS,
                    content: None,
                },
            ))
            .id();

        app.update();

        let state = app
            .world()
            .get::<super::super::state::GitState>(page)
            .unwrap();
        assert!(state.snapshot.diff_loading);
        assert!(state.snapshot.diff_viewport.is_none());
        assert!(state.snapshot.diff_rows.is_empty());
        assert!(app.world().get::<GitDiffQuery>(page).is_none());
        assert!(app.world().get_entity(job).is_err());
    }

    #[test]
    fn repository_refresh_keeps_the_same_target_visible_until_replacement_starts() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "file.rs", "fn main() {}\n");
        let request =
            crate::event::GitRepositoryRequest::at(repo.path().to_string_lossy().into_owned());
        let mut repository = crate::event::GitRepositorySnapshot::loading(&request);
        repository.repo_root = repo.path().to_string_lossy().into_owned();
        repository.path.clone_from(&repository.repo_root);
        let target = GitDiffTarget {
            repo_root: repo.path().to_path_buf(),
            path: file.clone(),
            path_bytes: b"file.rs".to_vec(),
            scope: DiffScope::File,
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<GitRuntime>()
            .add_systems(Update, sync_page_targets);
        let page = app
            .world_mut()
            .spawn((
                vmux_ecs::PageMetadata {
                    url: crate::GitUrl::from_path(repo.path()).unwrap(),
                    ..Default::default()
                },
                super::super::state::GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        workspace: repository.repo_root.clone(),
                        repository: Some(repository),
                        diff_viewport: Some(GitDiffViewport {
                            generation: 1,
                            revision: 1,
                            first_line: 0,
                            total_lines: 1,
                            has_more: false,
                            lines: vec![line(DiffKind::Context, 1)],
                            markers: Vec::new(),
                            error: String::new(),
                        }),
                        diff_rows: vec![crate::event::GitDiffRow::Line(0)],
                        nonce: 2,
                        ..Default::default()
                    },
                },
                GitPageControllerState {
                    focused_panel: GitPanel::Files,
                    selected_abs_path: file.to_string_lossy().into_owned(),
                    selected_path_bytes: b"file.rs".to_vec(),
                    ..Default::default()
                },
                super::super::state::GitDiffRevealRanges::default(),
                GitDiffQuery {
                    target: target.clone(),
                    generation: 1,
                    revision: 1,
                    file: None,
                    page_revision: Some(1),
                    branch: None,
                    cached_source: None,
                    prefetch: PrefetchBudget::default(),
                    loading: false,
                },
            ))
            .id();

        app.update();

        let state = app
            .world()
            .get::<super::super::state::GitState>(page)
            .unwrap();
        assert!(state.snapshot.diff_viewport.is_some());
        assert_eq!(state.snapshot.diff_rows.len(), 1);
        assert!(app.world().get::<GitDiffQuery>(page).is_some());
        let pending = app.world().get::<PendingGitDiff>(page).unwrap();
        assert_eq!(pending.target, target);
        assert_eq!(pending.page_revision, Some(2));
    }

    #[test]
    fn branch_diff_progress_loads_files_in_pages_without_repeating_a_seek() {
        let paths = (0..20)
            .map(|index| format!("file-{index}.rs").into_bytes())
            .collect();
        let mut progress = BranchDiffProgress::new(BranchFilesCatalog::for_paths(paths));

        let first = progress.next();
        assert_eq!(first.paths.len(), BRANCH_PATH_BATCH_SIZE);
        assert_eq!(first.paths.first(), Some(&b"file-0.rs".to_vec()));
        assert_eq!(
            first.paths.last(),
            Some(&format!("file-{}.rs", BRANCH_PATH_BATCH_SIZE - 1).into_bytes())
        );
        assert!(first.has_more);
        assert!(progress.seek(b"file-19.rs").is_some());
        assert!(progress.seek(b"file-19.rs").is_none());
        progress.complete(&first.paths, first.top_line, 40, true, true, None);
        let second = progress.next();

        let expected = (BRANCH_PATH_BATCH_SIZE..(BRANCH_PATH_BATCH_SIZE * 2).min(19))
            .map(|index| format!("file-{index}.rs").into_bytes())
            .collect::<Vec<_>>();
        assert_eq!(second.paths, expected);
    }

    #[test]
    fn failed_branch_diff_page_remains_retryable() {
        let paths = (0..10)
            .map(|index| format!("file-{index}.rs").into_bytes())
            .collect();
        let mut progress = BranchDiffProgress::new(BranchFilesCatalog::for_paths(paths));

        let first = progress.next();
        progress.complete(&first.paths, first.top_line, 0, false, false, None);
        let retry = progress.next();

        assert_eq!(retry.paths, first.paths);
        assert_eq!(retry.top_line, first.top_line);
        assert!(retry.has_more);
    }

    #[test]
    fn completed_file_diff_releases_cached_source_lines() {
        let mut query = GitDiffQuery::for_test(1);
        let lines = Arc::new(vec![line(DiffKind::Add, 1); 20]);

        query.cache_source(
            Some(DiffSource::shared_source_lines(Arc::clone(&lines))),
            true,
        );
        assert!(query.cached_source.is_some());
        query.cache_source(Some(DiffSource::shared_source_lines(lines)), false);
        assert!(query.cached_source.is_none());
    }

    #[test]
    fn large_branch_file_continues_before_the_next_file() {
        let paths = (0..17)
            .map(|index| format!("file-{index}.lock").into_bytes())
            .collect::<Vec<_>>();
        let mut progress = BranchDiffProgress::new(BranchFilesCatalog::for_paths(paths));

        let first = progress.next();
        let lines = Arc::new(vec![line(DiffKind::Add, 1); 920]);
        progress.complete(
            &first.paths,
            first.top_line,
            800,
            false,
            true,
            Some(DiffSource::shared_source_lines(Arc::clone(&lines))),
        );
        let seek = progress.seek(b"file-16.lock").unwrap();
        progress.complete(
            &seek.paths,
            seek.top_line,
            400,
            false,
            true,
            Some(DiffSource::shared_source_lines(Arc::clone(&lines))),
        );
        let continuation = progress.next();

        assert_eq!(continuation.paths, first.paths);
        assert_eq!(continuation.top_line, 800);
        assert!(continuation.document.is_some());

        progress.complete(
            &continuation.paths,
            continuation.top_line,
            120,
            true,
            true,
            continuation.document,
        );
        let next = progress.next();

        assert_eq!(next.paths, [b"file-16.lock".to_vec()]);
        assert_eq!(next.top_line, 400);
    }

    #[test]
    fn branch_files_reload_when_the_repository_revision_changes() {
        let repo = tempfile::tempdir().unwrap();
        let existing = crate::event::GitBranchFileEntry {
            path: "src/lib.rs".to_string(),
            path_bytes: b"src/lib.rs".to_vec(),
            previous_path: None,
            previous_path_bytes: None,
            change: crate::event::GitPathChange::Modified,
        };
        let mut app = App::new();
        app.add_plugins(vmux_ecs::UiStatePlugin::<UiGit>::default())
            .add_systems(Update, sync_branch_files);
        let page = app
            .world_mut()
            .spawn((
                super::super::state::GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        workspace: repo.path().to_string_lossy().into_owned(),
                        nonce: 1,
                        branch_files: vec![existing.clone()],
                        branch_files_reference: "feature".to_string(),
                        branch_files_total: 1,
                        ..Default::default()
                    },
                },
                GitPageControllerState {
                    focused_panel: GitPanel::Branches,
                    selected_branch: "feature".to_string(),
                    ..Default::default()
                },
            ))
            .id();

        app.update();
        let mut foreground_jobs = app
            .world_mut()
            .query_filtered::<Entity, (With<BranchFilesJob>, With<GitForegroundRead>)>();
        assert_eq!(foreground_jobs.iter(app.world()).count(), 1);
        assert_eq!(
            app.world()
                .get::<GitBranchFilesQuery>(page)
                .map(|query| (query.generation, query.page_revision)),
            Some((1, 1))
        );
        assert_eq!(
            app.world()
                .get::<super::super::state::GitState>(page)
                .unwrap()
                .snapshot
                .branch_files,
            vec![existing.clone()]
        );

        app.world_mut()
            .get_mut::<super::super::state::GitState>(page)
            .unwrap()
            .snapshot
            .nonce = 2;
        app.update();

        assert_eq!(
            app.world()
                .get::<GitBranchFilesQuery>(page)
                .map(|query| (query.generation, query.page_revision)),
            Some((2, 2))
        );
        assert_eq!(
            app.world()
                .get::<super::super::state::GitState>(page)
                .unwrap()
                .snapshot
                .branch_files,
            vec![existing]
        );
        let mut foreground_jobs = app
            .world_mut()
            .query_filtered::<Entity, (With<BranchFilesJob>, With<GitForegroundRead>)>();
        assert_eq!(foreground_jobs.iter(app.world()).count(), 1);
    }

    #[test]
    fn markers_classify_modified_added_and_deleted_lines() {
        let lines = vec![
            DiffLine {
                kind: DiffKind::Remove,
                old_no: Some(2),
                new_no: None,
                hunk: None,
                spans: Vec::new(),
            },
            DiffLine {
                kind: DiffKind::Add,
                old_no: None,
                new_no: Some(2),
                hunk: None,
                spans: Vec::new(),
            },
            line(DiffKind::Context, 3),
            DiffLine {
                kind: DiffKind::Add,
                old_no: None,
                new_no: Some(8),
                hunk: Some(1),
                spans: Vec::new(),
            },
            DiffLine {
                kind: DiffKind::Remove,
                old_no: Some(12),
                new_no: None,
                hunk: Some(2),
                spans: Vec::new(),
            },
            line(DiffKind::Context, 12),
        ];

        let markers = GitDiffMarkers::from_lines(&lines).into_inner();

        assert!(markers.contains(&GitLineMarker {
            line: 2,
            status: GitLineStatus::Modified,
        }));
        assert!(markers.contains(&GitLineMarker {
            line: 8,
            status: GitLineStatus::Added,
        }));
        assert!(markers.contains(&GitLineMarker {
            line: 12,
            status: GitLineStatus::Deleted,
        }));
    }

    #[test]
    fn a_new_target_supersedes_the_previous_diff_generation() {
        let mut app = App::new();
        app.add_systems(Update, start_requests);
        let entity = app
            .world_mut()
            .spawn((
                super::super::state::GitState::default(),
                PendingGitDiff {
                    target: GitDiffTarget {
                        repo_root: "/repo".into(),
                        path: "/repo/a.rs".into(),
                        path_bytes: Vec::new(),
                        scope: DiffScope::File,
                    },
                    file: None,
                    page_revision: None,
                },
            ))
            .id();

        app.update();
        assert!(
            app.world()
                .get::<GitDiffQuery>(entity)
                .is_some_and(|query| query.accepts(1))
        );

        app.world_mut().entity_mut(entity).insert(PendingGitDiff {
            target: GitDiffTarget {
                repo_root: "/repo".into(),
                path: "/repo/b.rs".into(),
                path_bytes: Vec::new(),
                scope: DiffScope::File,
            },
            file: None,
            page_revision: None,
        });
        app.update();

        let query = app.world().get::<GitDiffQuery>(entity).unwrap();
        assert!(query.accepts(2));
        assert!(!query.accepts(1));
        assert_eq!(query.target.path, PathBuf::from("/repo/b.rs"));
    }

    #[test]
    fn restored_viewport_generation_is_not_reused_by_a_fresh_query() {
        let mut app = App::new();
        app.add_systems(Update, start_requests);
        let entity = app
            .world_mut()
            .spawn((
                super::super::state::GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        diff_viewport: Some(GitDiffViewport {
                            generation: 7,
                            revision: 3,
                            first_line: 0,
                            total_lines: 0,
                            has_more: false,
                            lines: Vec::new(),
                            markers: Vec::new(),
                            error: String::new(),
                        }),
                        ..Default::default()
                    },
                },
                PendingGitDiff {
                    target: GitDiffTarget {
                        repo_root: "/repo".into(),
                        path: "/repo/a.rs".into(),
                        path_bytes: Vec::new(),
                        scope: DiffScope::File,
                    },
                    file: None,
                    page_revision: None,
                },
            ))
            .id();

        app.update();

        assert!(
            app.world()
                .get::<GitDiffQuery>(entity)
                .is_some_and(|query| query.accepts(8))
        );
    }

    #[test]
    fn stale_next_request_cannot_prefetch_a_replaced_diff() {
        let mut app = App::new();
        app.add_observer(load_next);
        let page = app
            .world_mut()
            .spawn((
                super::super::state::GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        diff_viewport: Some(GitDiffViewport {
                            generation: 2,
                            revision: 4,
                            first_line: 0,
                            total_lines: 1000,
                            has_more: true,
                            lines: vec![line(DiffKind::Context, 1)],
                            markers: Vec::new(),
                            error: String::new(),
                        }),
                        ..Default::default()
                    },
                },
                GitDiffQuery::for_test(2),
            ))
            .id();

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GitDiffNextRequest {
                generation: 1,
                revision: 4,
            },
        });
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<DiffJob>>()
                .iter(app.world())
                .count(),
            0
        );

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GitDiffNextRequest {
                generation: 2,
                revision: 4,
            },
        });
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<DiffJob>>()
                .iter(app.world())
                .count(),
            1
        );
    }

    #[test]
    fn file_diff_query_rejects_previous_document_and_refresh() {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let file = super::super::status::FileGit::new("/repo/a.rs", 7);
        let mut query = GitDiffQuery {
            target: GitDiffTarget {
                repo_root: "/repo".into(),
                path: "/repo/a.rs".into(),
                path_bytes: Vec::new(),
                scope: DiffScope::File,
            },
            generation: 3,
            revision: 0,
            file: Some(FileDiffIdentity {
                document: 7,
                refresh: 0,
            }),
            page_revision: None,
            branch: None,
            cached_source: None,
            prefetch: PrefetchBudget::default(),
            loading: false,
        };

        assert!(query.accepts_file(3, &file));

        query.file.as_mut().unwrap().refresh = 1;
        assert!(!query.accepts_file(3, &file));

        let replacement = super::super::status::FileGit::new("/repo/a.rs", 8);
        assert!(!query.accepts_file(3, &replacement));
    }
}
