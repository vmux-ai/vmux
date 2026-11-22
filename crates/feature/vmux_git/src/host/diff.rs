use std::collections::HashSet;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::Arc;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::event::{
    GitBranchFilesNextRequest, GitBranchFilesRetryRequest, GitDiffNextRequest, GitDiffRetryRequest,
    GitDiffRevealRequest, GitDiffSeekRequest,
};
use crate::state::{
    GitBranchFilesPagePatch, GitBranchFilesPatch, GitPageControllerState, GitPanel, GitUiState,
};

use super::GitUpdateSet;
use super::job::{BranchFilesJob, DiffJob, DiffScope, GitForegroundRead};
use super::job_runner::{GitJob, QueuedGitJobs};
use super::repository::{BranchFilesCatalog, BranchFilesPage, DiffSource, GitRepository};

pub(super) struct DiffPlugin;

impl Plugin for DiffPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            GitDiffRevealRequest,
            GitDiffNextRequest,
            GitDiffSeekRequest,
            GitDiffRetryRequest,
            GitBranchFilesNextRequest,
            GitBranchFilesRetryRequest,
        )>::default())
            .add_observer(seed)
            .add_observer(cleanup_branch_continuations)
            .add_observer(file_refresh)
            .add_observer(reveal)
            .add_observer(load_next)
            .add_observer(seek)
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
            GitPanel::Commits if !controller.selected_commit.is_empty() => Some(Self {
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
    generation: u64,
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
    cached_source: Option<DiffSource>,
    loading: bool,
}

#[derive(Component, Clone, Copy, Default)]
struct Generation(u64);

impl Generation {
    fn observe(&mut self, value: u64) {
        self.0 = self.0.max(value);
    }

    fn advance(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1).max(1);
        self.0
    }
}

fn seed(
    event: On<Add, GitDiffQuery>,
    queries: Query<(&GitDiffQuery, Option<&Generation>)>,
    mut commands: Commands,
) {
    let entity = event.event_target();
    let Ok((query, current)) = queries.get(entity) else {
        return;
    };
    let mut generation = current.copied().unwrap_or_default();
    generation.observe(query.generation);
    commands.entity(entity).insert(generation);
}

#[derive(Component, Clone, Copy, Default)]
pub(super) struct PrefetchBudget {
    batches: u8,
    rows: u32,
}

impl PrefetchBudget {
    const WINDOW_ROWS: u32 = 640;
    const BATCHES: u8 = 4;
    const ROWS: u32 = Self::WINDOW_ROWS * Self::BATCHES as u32;

    fn fresh() -> Self {
        Self {
            batches: Self::BATCHES,
            rows: Self::ROWS,
        }
    }

    fn arm(&mut self) {
        if self.batches == 0 || self.rows == 0 {
            self.restart();
        }
    }

    fn restart(&mut self) {
        self.batches = Self::BATCHES;
        self.rows = Self::ROWS;
    }

    pub(super) fn continue_after(&mut self, rows: u32) -> bool {
        self.batches = self.batches.saturating_sub(1);
        self.rows = self.rows.saturating_sub(rows);
        self.batches > 0 && self.rows > 0
    }
}

#[derive(Component, Clone)]
pub(super) struct BranchDiffCursor {
    catalog: BranchFilesCatalog,
    loaded: HashSet<Vec<u8>>,
    in_flight: HashSet<Vec<u8>>,
    next: usize,
    next_continuation: u64,
}

#[derive(Component, Clone, Default)]
pub(super) struct BranchDiffPriority {
    path: Option<Vec<u8>>,
    nearby: Option<usize>,
}

#[derive(Component, Clone, Copy)]
pub(super) struct BranchDiffContinuationOf(Entity);

#[derive(Component, Clone)]
pub(super) struct BranchDiffContinuation {
    order: u64,
    paths: Vec<Vec<u8>>,
    top_line: u32,
    document: Option<DiffSource>,
}

impl BranchDiffCursor {
    const PATH_BATCH_SIZE: usize = 4;

    fn new(catalog: BranchFilesCatalog) -> Self {
        Self {
            catalog,
            loaded: HashSet::new(),
            in_flight: HashSet::new(),
            next: 0,
            next_continuation: 0,
        }
    }

    fn path_for_label(&self, label: &str) -> Option<Vec<u8>> {
        for index in 0..self.catalog.len() {
            let path = self.catalog.path(index)?;
            if String::from_utf8_lossy(path) == label {
                return Some(path.to_vec());
            }
        }
        None
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

    fn has_unloaded(&self) -> bool {
        for index in self.next..self.catalog.len() {
            if self
                .catalog
                .path(index)
                .is_some_and(|path| !self.loaded.contains(path) && !self.in_flight.contains(path))
            {
                return true;
            }
        }
        false
    }

    fn parked(&self) -> Self {
        let mut cursor = self.clone();
        cursor.in_flight.clear();
        cursor.next = 0;
        cursor.advance();
        cursor
    }

    fn take_catalog_paths(&mut self) -> Vec<Vec<u8>> {
        let mut paths = Vec::with_capacity(Self::PATH_BATCH_SIZE);
        while paths.len() < Self::PATH_BATCH_SIZE {
            self.advance();
            let Some(path) = self.catalog.path(self.next).map(<[u8]>::to_vec) else {
                break;
            };
            self.next += 1;
            if self.in_flight.insert(path.clone()) {
                paths.push(path);
            }
        }
        self.advance();
        paths
    }

    fn take_initial(&mut self, priority: &mut BranchDiffPriority) -> (Vec<Vec<u8>>, bool) {
        if let Some(path) = priority.path.clone()
            && let Some(index) = self.catalog.position(&path)
            && !self.loaded.contains(&path)
            && self.in_flight.insert(path.clone())
        {
            priority.nearby = Some(index.saturating_add(1));
            self.advance();
            return (vec![path], self.has_unloaded());
        }
        let paths = self.take_catalog_paths();
        let has_more = !paths.is_empty() && self.has_unloaded();
        (paths, has_more)
    }

    fn allocate_continuation_order(&mut self) -> u64 {
        self.next_continuation = self.next_continuation.wrapping_add(1).max(1);
        self.next_continuation
    }
}

#[derive(SystemParam)]
pub(super) struct BranchDiffs<'w, 's> {
    cursors: Query<'w, 's, &'static mut BranchDiffCursor>,
    priorities: Query<'w, 's, &'static mut BranchDiffPriority>,
    continuations: Query<
        'w,
        's,
        (
            Entity,
            &'static BranchDiffContinuationOf,
            &'static BranchDiffContinuation,
        ),
    >,
}

impl BranchDiffs<'_, '_> {
    fn has_continuations(&self, owner: Entity, excluding: Option<Entity>) -> bool {
        for (entity, parent, _) in &self.continuations {
            if parent.0 == owner && Some(entity) != excluding {
                return true;
            }
        }
        false
    }

    fn next_continuation(&self, owner: Entity) -> Option<(Entity, BranchDiffContinuation)> {
        let mut best: Option<(Entity, &BranchDiffContinuation)> = None;
        for (entity, parent, continuation) in &self.continuations {
            if parent.0 != owner {
                continue;
            }
            if best.is_none_or(|(_, current)| continuation.order < current.order) {
                best = Some((entity, continuation));
            }
        }
        best.map(|(entity, continuation)| (entity, continuation.clone()))
    }

    fn matching_continuation(
        &self,
        owner: Entity,
        path: &[u8],
    ) -> Option<(Entity, BranchDiffContinuation)> {
        let mut best: Option<(Entity, &BranchDiffContinuation)> = None;
        for (entity, parent, continuation) in &self.continuations {
            if parent.0 != owner || !continuation.paths.iter().any(|candidate| candidate == path) {
                continue;
            }
            if best.is_none_or(|(_, current)| continuation.order < current.order) {
                best = Some((entity, continuation));
            }
        }
        best.map(|(entity, continuation)| (entity, continuation.clone()))
    }

    fn seek_job(
        &mut self,
        owner: Entity,
        query: &GitDiffQuery,
        path: &[u8],
        top_line: u32,
        commands: &mut Commands,
    ) -> Option<DiffJob> {
        let matched = self.matching_continuation(owner, path);
        let has_continuations = self.has_continuations(owner, None);
        let mut cursor = self.cursors.get_mut(owner).ok()?;
        let mut priority = self.priorities.get_mut(owner).ok()?;
        let index = cursor.catalog.position(path)?;
        if cursor.loaded.contains(path) || !cursor.in_flight.insert(path.to_vec()) {
            return None;
        }
        priority.path = Some(path.to_vec());
        priority.nearby = Some(index.saturating_add(1));
        if let Some((entity, continuation)) = matched {
            commands.entity(entity).despawn();
            cursor.in_flight.extend(continuation.paths.iter().cloned());
            return Some(query.diff_job(
                continuation.paths,
                continuation.document,
                cursor.catalog.source().to_path_buf(),
                cursor.catalog.merge_base().to_string(),
                continuation.top_line,
                true,
                top_line,
                None,
            ));
        }
        let has_more = cursor.has_unloaded() || has_continuations;
        Some(query.diff_job(
            vec![path.to_vec()],
            None,
            cursor.catalog.source().to_path_buf(),
            cursor.catalog.merge_base().to_string(),
            0,
            has_more,
            top_line,
            None,
        ))
    }

    fn next_nearby_job(
        &mut self,
        owner: Entity,
        query: &GitDiffQuery,
        top_line: u32,
        commands: &mut Commands,
    ) -> Option<DiffJob> {
        let mut index = self.priorities.get(owner).ok()?.nearby?;
        loop {
            let next_path = self
                .cursors
                .get(owner)
                .ok()?
                .catalog
                .path(index)
                .map(<[u8]>::to_vec);
            let Some(path) = next_path else {
                if let Ok(mut priority) = self.priorities.get_mut(owner) {
                    priority.nearby = None;
                }
                return None;
            };
            index += 1;
            if let Ok(mut priority) = self.priorities.get_mut(owner) {
                priority.nearby = Some(index);
            }
            if let Some(job) = self.seek_job(owner, query, &path, top_line, commands) {
                return Some(job);
            }
        }
    }

    fn next_job(
        &mut self,
        owner: Entity,
        query: &GitDiffQuery,
        top_line: u32,
        content: Option<String>,
        commands: &mut Commands,
    ) -> DiffJob {
        if let Some(path) = self
            .priorities
            .get(owner)
            .ok()
            .and_then(|priority| priority.path.clone())
            && let Some(job) = self.seek_job(owner, query, &path, top_line, commands)
        {
            return job;
        }
        if let Some(job) = self.next_nearby_job(owner, query, top_line, commands) {
            return job;
        }
        if let Some((entity, continuation)) = self.next_continuation(owner)
            && let Ok(mut cursor) = self.cursors.get_mut(owner)
        {
            commands.entity(entity).despawn();
            cursor.in_flight.extend(continuation.paths.iter().cloned());
            return query.diff_job(
                continuation.paths,
                continuation.document,
                cursor.catalog.source().to_path_buf(),
                cursor.catalog.merge_base().to_string(),
                continuation.top_line,
                true,
                top_line,
                content,
            );
        }
        let has_continuations = self.has_continuations(owner, None);
        if let Ok(mut cursor) = self.cursors.get_mut(owner) {
            let paths = cursor.take_catalog_paths();
            if !paths.is_empty() {
                let has_more = cursor.has_unloaded() || has_continuations;
                return query.diff_job(
                    paths,
                    None,
                    cursor.catalog.source().to_path_buf(),
                    cursor.catalog.merge_base().to_string(),
                    0,
                    has_more,
                    top_line,
                    content,
                );
            }
        }
        query.diff_job(
            Vec::new(),
            None,
            PathBuf::new(),
            String::new(),
            0,
            false,
            top_line,
            content,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn complete(
        &mut self,
        owner: Entity,
        paths: &[Vec<u8>],
        top_line: u32,
        loaded_lines: u32,
        page_complete: bool,
        succeeded: bool,
        document: Option<DiffSource>,
        commands: &mut Commands,
    ) -> bool {
        let queued = !succeeded || !page_complete;
        let has_unloaded = {
            let Ok(mut cursor) = self.cursors.get_mut(owner) else {
                return false;
            };
            for path in paths {
                cursor.in_flight.remove(path);
                if succeeded && page_complete {
                    cursor.loaded.insert(path.clone());
                }
            }
            if page_complete
                && succeeded
                && let Ok(mut priority) = self.priorities.get_mut(owner)
                && priority
                    .path
                    .as_ref()
                    .is_some_and(|path| cursor.loaded.contains(path))
            {
                priority.path = None;
            }
            if queued {
                let order = cursor.allocate_continuation_order();
                commands.spawn((
                    Name::new("git.branch.diff.continuation"),
                    BranchDiffContinuationOf(owner),
                    BranchDiffContinuation {
                        order,
                        paths: paths.to_vec(),
                        top_line: if succeeded {
                            top_line.saturating_add(loaded_lines)
                        } else {
                            top_line
                        },
                        document,
                    },
                ));
            }
            cursor.advance();
            cursor.has_unloaded()
        };
        queued || has_unloaded || self.has_continuations(owner, None)
    }

    pub(super) fn clear_continuations(&self, owner: Entity, commands: &mut Commands) {
        for (entity, parent, _) in &self.continuations {
            if parent.0 == owner {
                commands.entity(entity).despawn();
            }
        }
    }

    pub(super) fn park_to(&self, from: Entity, to: Entity, commands: &mut Commands) {
        if let Ok(cursor) = self.cursors.get(from) {
            commands.entity(to).insert(cursor.parked());
        }
        if let Ok(priority) = self.priorities.get(from) {
            commands.entity(to).insert(priority.clone());
        }
        for (_, parent, continuation) in &self.continuations {
            if parent.0 != from {
                continue;
            }
            commands.spawn((
                Name::new("git.branch.diff.continuation"),
                BranchDiffContinuationOf(to),
                continuation.clone(),
            ));
        }
    }
}

fn cleanup_branch_continuations(
    trigger: On<Remove, BranchDiffCursor>,
    continuations: Query<(Entity, &BranchDiffContinuationOf)>,
    mut commands: Commands,
) {
    let owner = trigger.event_target();
    for (entity, parent) in &continuations {
        if parent.0 == owner {
            commands.entity(entity).try_despawn();
        }
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

    #[allow(clippy::too_many_arguments)]
    fn diff_job(
        &self,
        branch_paths: Vec<Vec<u8>>,
        document: Option<DiffSource>,
        branch_source: PathBuf,
        branch_merge_base: String,
        branch_top_line: u32,
        branch_has_more: bool,
        top_line: u32,
        content: Option<String>,
    ) -> DiffJob {
        DiffJob {
            path: self.target.path.clone(),
            scope: self.target.scope.clone(),
            branch_paths,
            cached_source: document.or_else(|| self.cached_source.clone()),
            branch_source,
            branch_merge_base,
            branch_top_line,
            branch_has_more,
            generation: self.generation,
            top_line,
            rows: PrefetchBudget::WINDOW_ROWS,
            content,
        }
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
            cached_source: None,
            loading: false,
        }
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

#[allow(clippy::type_complexity)]
fn sync_page_targets(
    mut pages: Query<(
        Entity,
        &vmux_ecs::PageMetadata,
        Mut<super::state::GitState>,
        Ref<GitPageControllerState>,
        &mut super::state::GitDiffRevealRanges,
        Ref<super::state::GitRepositoryRevision>,
        Option<Ref<GitBranchFilesQuery>>,
        Option<&PendingGitDiff>,
        Option<&GitDiffQuery>,
        Option<&BranchDiffCursor>,
        Option<&BranchDiffPriority>,
    )>,
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
        cursor,
        priority,
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
            queued_diffs.cancel_for(entity, &mut commands);
            if current.is_some() || pending.is_some() || state.snapshot.diff_viewport.is_some() {
                state.snapshot.diff_loading = false;
                state.snapshot.diff_viewport = None;
                state.snapshot.diff_rows.clear();
                revealed.0.clear();
            }
            commands.entity(entity).remove::<(
                GitDiffQuery,
                PendingGitDiff,
                PendingGitDiffSeek,
                BranchDiffCursor,
                BranchDiffPriority,
                PrefetchBudget,
            )>();
            continue;
        };
        let page_revision = state.diff_revision();
        let target_changed = current.is_some_and(|current| current.target != target)
            || pending.is_some_and(|pending| pending.target != target);
        if target_changed {
            queued_diffs.cancel_for(entity, &mut commands);
            state.snapshot.diff_loading = true;
            state.snapshot.diff_viewport = None;
            state.snapshot.diff_rows.clear();
            revealed.0.clear();
            commands.entity(entity).remove::<(
                GitDiffQuery,
                PendingGitDiff,
                PendingGitDiffSeek,
                BranchDiffCursor,
                BranchDiffPriority,
                PrefetchBudget,
            )>();
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
            if let Some(query) = current {
                let selected = &controller.selected_branch_path_bytes;
                let has_priority = priority.is_some_and(|priority| priority.path.is_some());
                let contains_selected = cursor
                    .is_some_and(|cursor| cursor.loaded.contains(selected))
                    || priority.is_some_and(|priority| priority.path.as_deref() == Some(selected));
                let selection_changed = controller.is_changed() || !has_priority;
                if !selected.is_empty() && selection_changed && !contains_selected {
                    commands.entity(entity).insert(PendingGitDiffSeek {
                        path_bytes: selected.clone(),
                        generation: query.generation,
                    });
                }
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
        commands.trigger(vmux_ecs::UiStateWrite::<GitUiState>::from_event(
            entity, &patch,
        ));
        queued_jobs.cancel_for(entity, &mut commands);
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
    mut commands: Commands,
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
    commands.trigger(vmux_ecs::UiStateWrite::<GitUiState>::from_event(
        entity, &patch,
    ));
}

fn retry_branch_files(
    trigger: On<UiInput<GitBranchFilesRetryRequest>>,
    mut states: Query<&mut super::state::GitState>,
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
    commands.trigger(vmux_ecs::UiStateWrite::<GitUiState>::from_event(
        entity, &patch,
    ));
    commands.entity(entity).remove::<GitBranchFilesQuery>();
}

#[allow(clippy::type_complexity)]
fn load_next(
    trigger: On<UiInput<GitDiffNextRequest>>,
    mut pages: Query<(
        &super::state::GitState,
        &mut GitDiffQuery,
        Option<&mut PrefetchBudget>,
        Option<&PendingGitDiffSeek>,
    )>,
    sources: Query<&GitDiffSource>,
    mut branch_diffs: BranchDiffs,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((state, mut query, budget, pending)) = pages.get_mut(entity) else {
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
    if query.loading || !viewport.has_more || pending.is_some() {
        return;
    }
    let content = sources
        .get(entity)
        .ok()
        .filter(|source| source.dirty)
        .map(|source| source.content.clone());
    match budget {
        Some(mut budget) => budget.arm(),
        None => {
            commands.entity(entity).insert(PrefetchBudget::fresh());
        }
    }
    query.loading = true;
    let job = branch_diffs.next_job(entity, &query, loaded, content, &mut commands);
    commands.spawn((
        GitJob::new(entity),
        GitForegroundRead,
        GitRepository::at(query.target.repo_root.clone()),
        job,
    ));
}

fn seek(
    trigger: On<UiInput<GitDiffSeekRequest>>,
    pages: Query<(&super::state::GitState, &GitDiffQuery, &BranchDiffCursor)>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((state, query, cursor)) = pages.get(entity) else {
        return;
    };
    let Some(viewport) = state.snapshot.diff_viewport.as_ref() else {
        return;
    };
    let request = &trigger.event().payload;
    if request.generation != query.generation
        || request.generation != viewport.generation
        || request.revision > viewport.revision
    {
        return;
    }
    let Some(path_bytes) = cursor.path_for_label(&request.path) else {
        return;
    };
    if cursor.loaded.contains(&path_bytes) {
        return;
    }
    commands.entity(entity).insert(PendingGitDiffSeek {
        path_bytes,
        generation: query.generation,
    });
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

#[allow(clippy::type_complexity)]
fn start_pending_seek(
    mut pages: Query<(
        Entity,
        &super::state::GitState,
        &mut GitDiffQuery,
        Option<&mut PrefetchBudget>,
        &PendingGitDiffSeek,
    )>,
    mut branch_diffs: BranchDiffs,
    mut commands: Commands,
) {
    for (entity, state, mut query, budget, pending) in &mut pages {
        if pending.generation != query.generation
            || !matches!(query.target.scope, DiffScope::Branch(_))
        {
            commands.entity(entity).remove::<PendingGitDiffSeek>();
            continue;
        }
        if query.loading {
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
        let job = branch_diffs.seek_job(entity, &query, &pending.path_bytes, loaded, &mut commands);
        commands.entity(entity).remove::<PendingGitDiffSeek>();
        let Some(job) = job else {
            continue;
        };
        match budget {
            Some(mut budget) => budget.restart(),
            None => {
                commands.entity(entity).insert(PrefetchBudget::fresh());
            }
        }
        query.loading = true;
        commands.spawn((
            GitJob::new(entity),
            GitForegroundRead,
            GitRepository::at(query.target.repo_root.clone()),
            job,
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

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn start_requests(
    pending: Query<(
        Entity,
        &PendingGitDiff,
        Option<&GitDiffQuery>,
        Option<&GitDiffSource>,
        Option<&GitBranchFilesQuery>,
    )>,
    controllers: Query<&GitPageControllerState>,
    generations: Query<&Generation>,
    mut pages: Query<(
        &mut super::state::GitState,
        &mut super::state::GitDiffRevealRanges,
    )>,
    mut files: super::status::FileGitStates,
    queued_diffs: QueuedGitJobs<DiffJob>,
    branch_diffs: BranchDiffs,
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
        let mut sequence = generations.get(entity).copied().unwrap_or_default();
        sequence.observe(generation);
        generation = sequence.advance();
        let target = pending.target.clone();
        let content = source
            .filter(|source| source.dirty)
            .map(|source| source.content.clone());
        queued_diffs.cancel_for(entity, &mut commands);
        branch_diffs.clear_continuations(entity, &mut commands);
        let query = GitDiffQuery {
            target: target.clone(),
            generation,
            revision: 0,
            file: pending.file,
            page_revision: pending.page_revision,
            cached_source: None,
            loading: true,
        };
        let mut job = query.diff_job(
            Vec::new(),
            None,
            PathBuf::new(),
            String::new(),
            0,
            false,
            0,
            content,
        );
        let mut owner = commands.entity(entity);
        owner
            .remove::<(PendingGitDiff, PendingGitDiffSeek)>()
            .insert((query, sequence, PrefetchBudget::fresh()));
        if matches!(target.scope, DiffScope::Branch(_))
            && let Some(branch_files) = branch_files
        {
            let mut cursor = BranchDiffCursor::new(branch_files.catalog.clone());
            let mut priority = BranchDiffPriority::default();
            if let Ok(controller) = controllers.get(entity)
                && cursor
                    .catalog
                    .contains(&controller.selected_branch_path_bytes)
            {
                priority.path = Some(controller.selected_branch_path_bytes.clone());
            }
            let (paths, has_more) = cursor.take_initial(&mut priority);
            job.branch_paths = paths;
            job.branch_source = cursor.catalog.source().to_path_buf();
            job.branch_merge_base = cursor.catalog.merge_base().to_string();
            job.branch_has_more = has_more;
            owner.insert((cursor, priority));
        } else {
            owner.remove::<(BranchDiffCursor, BranchDiffPriority)>();
        }
        commands.spawn((
            GitJob::new(entity),
            GitForegroundRead,
            GitRepository::at(target.repo_root),
            job,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{DiffKind, DiffLine, GitDiffViewport, StyledSpan};
    use crate::host::repository::test_repo;
    use crate::host::runtime::GitRuntime;

    struct BranchReads {
        app: App,
        page: Entity,
    }

    impl BranchReads {
        fn line(kind: DiffKind, no: u32) -> DiffLine {
            DiffLine {
                kind,
                old_no: Some(no),
                new_no: Some(no),
                hunk: None,
                spans: Vec::<StyledSpan>::new(),
            }
        }

        fn new() -> Self {
            let paths = (0..5)
                .map(|index| format!("file-{index}.rs").into_bytes())
                .collect();
            Self::with_paths(paths, true)
        }

        fn with_paths(paths: Vec<Vec<u8>>, seed_partial_first: bool) -> Self {
            let catalog = BranchFilesCatalog::for_paths(paths);
            let mut cursor = BranchDiffCursor::new(catalog.clone());
            let mut priority = BranchDiffPriority::default();
            let mut first_paths = Vec::new();
            if seed_partial_first {
                let (initial, _) = cursor.take_initial(&mut priority);
                first_paths = initial;
            }
            let mut query = GitDiffQuery::for_test(1);
            query.target.path = PathBuf::new();
            query.target.scope = DiffScope::Branch("feature".to_string());
            query.page_revision = Some(1);
            query.revision = 4;
            let request = crate::event::GitRepositoryRequest::project(
                "/repo".to_string(),
                "repo".to_string(),
                "feature".to_string(),
            );
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_ecs::UiStatePlugin::<GitUiState>::default(),
                DiffPlugin,
            ));
            let page = app
                .world_mut()
                .spawn((
                    vmux_ecs::PageMetadata {
                        url: crate::GIT_PAGE_URL.to_string(),
                        ..Default::default()
                    },
                    super::super::state::GitState {
                        snapshot: crate::state::GitPageSnapshot {
                            workspace: "/repo".to_string(),
                            repository: Some(crate::event::GitRepositorySnapshot::loading(
                                &request,
                            )),
                            nonce: 1,
                            diff_viewport: Some(GitDiffViewport {
                                generation: 1,
                                revision: 4,
                                first_line: 0,
                                total_lines: 800,
                                has_more: true,
                                lines: vec![Self::line(DiffKind::Add, 1); 800],
                                markers: Vec::new(),
                                error: String::new(),
                            }),
                            ..Default::default()
                        },
                    },
                    query,
                    PrefetchBudget::fresh(),
                    cursor,
                    priority,
                    GitBranchFilesQuery {
                        repository: PathBuf::from("/repo"),
                        reference: "feature".to_string(),
                        generation: 1,
                        page_revision: 1,
                        catalog,
                        ready: true,
                    },
                ))
                .id();
            app.world_mut()
                .get_mut::<GitPageControllerState>(page)
                .unwrap()
                .selected_branch = "feature".to_string();
            let mut reads = Self { app, page };
            if seed_partial_first {
                reads.complete(
                    &first_paths,
                    0,
                    800,
                    false,
                    true,
                    Some(DiffSource::shared_source_lines(Arc::new(vec![
                        Self::line(
                            DiffKind::Add,
                            1
                        );
                        920
                    ]))),
                );
            }
            reads
        }

        fn complete(
            &mut self,
            paths: &[Vec<u8>],
            top_line: u32,
            loaded_lines: u32,
            page_complete: bool,
            succeeded: bool,
            document: Option<DiffSource>,
        ) -> bool {
            let page = self.page;
            let mut state = bevy::ecs::system::SystemState::<(BranchDiffs, Commands)>::new(
                self.app.world_mut(),
            );
            let (mut diffs, mut commands) = state.get_mut(self.app.world_mut()).unwrap();
            let has_more = diffs.complete(
                page,
                paths,
                top_line,
                loaded_lines,
                page_complete,
                succeeded,
                document,
                &mut commands,
            );
            state.apply(self.app.world_mut());
            has_more
        }

        fn take_next_job(&mut self) -> DiffJob {
            let page = self.page;
            let query = self.app.world().get::<GitDiffQuery>(page).unwrap().clone();
            let mut state = bevy::ecs::system::SystemState::<(BranchDiffs, Commands)>::new(
                self.app.world_mut(),
            );
            let (mut diffs, mut commands) = state.get_mut(self.app.world_mut()).unwrap();
            let job = diffs.next_job(page, &query, 0, None, &mut commands);
            state.apply(self.app.world_mut());
            job
        }

        fn seek_job(&mut self, path: &[u8]) -> Option<DiffJob> {
            let page = self.page;
            let query = self.app.world().get::<GitDiffQuery>(page).unwrap().clone();
            let mut state = bevy::ecs::system::SystemState::<(BranchDiffs, Commands)>::new(
                self.app.world_mut(),
            );
            let (mut diffs, mut commands) = state.get_mut(self.app.world_mut()).unwrap();
            let job = diffs.seek_job(page, &query, path, 0, &mut commands);
            state.apply(self.app.world_mut());
            job
        }

        fn continuations(&mut self) -> Vec<BranchDiffContinuation> {
            let mut query = self.app.world_mut().query::<&BranchDiffContinuation>();
            let mut items = query.iter(self.app.world()).cloned().collect::<Vec<_>>();
            items.sort_by_key(|item| item.order);
            items
        }

        fn seek(&mut self, generation: u64, revision: u64, path: &str) {
            self.app.world_mut().trigger(UiInput {
                webview: self.page,
                payload: GitDiffSeekRequest {
                    generation,
                    revision,
                    path: path.to_string(),
                },
            });
            self.app.world_mut().flush();
        }

        fn next(&mut self) {
            self.app.world_mut().trigger(UiInput {
                webview: self.page,
                payload: GitDiffNextRequest {
                    generation: 1,
                    revision: 4,
                },
            });
            self.app.world_mut().flush();
        }

        fn paths(&mut self) -> Vec<Vec<u8>> {
            let mut query = self.app.world_mut().query::<&DiffJob>();
            let mut paths = Vec::new();
            for job in query.iter(self.app.world()) {
                paths.extend(job.branch_paths.clone());
            }
            paths
        }
    }

    #[test]
    fn visible_file_seek_wins_over_background_prefetch_without_changing_selection() {
        let mut reads = BranchReads::new();
        reads.seek(1, 4, "file-4.rs");
        reads.next();

        reads.app.update();

        assert_eq!(reads.paths(), [b"file-4.rs".to_vec()]);
        assert!(
            reads
                .app
                .world()
                .get::<GitPageControllerState>(reads.page)
                .unwrap()
                .selected_branch_path_bytes
                .is_empty()
        );
        let query = reads.app.world().get::<GitDiffQuery>(reads.page).unwrap();
        assert_eq!(query.generation, 1);
        let continuations = reads.continuations();
        let continuation = continuations.first().unwrap();
        assert_eq!(
            continuation.paths,
            [
                b"file-0.rs".to_vec(),
                b"file-1.rs".to_vec(),
                b"file-2.rs".to_vec(),
                b"file-3.rs".to_vec()
            ]
        );
        assert_eq!(continuation.top_line, 800);
    }

    #[test]
    fn replaced_future_or_unknown_visible_file_requests_do_not_schedule_reads() {
        for (generation, revision, path) in [
            (0, 4, "file-4.rs"),
            (1, 5, "file-4.rs"),
            (1, 4, "missing.rs"),
        ] {
            let mut reads = BranchReads::new();
            reads.seek(generation, revision, path);
            reads.app.update();
            assert!(reads.paths().is_empty());
            assert!(
                reads
                    .app
                    .world()
                    .get::<PendingGitDiffSeek>(reads.page)
                    .is_none()
            );
        }
    }

    #[test]
    fn visible_file_demand_survives_a_completed_background_revision() {
        let mut reads = BranchReads::new();
        reads.seek(1, 3, "file-4.rs");
        reads.app.update();

        assert_eq!(reads.paths(), [b"file-4.rs".to_vec()]);
    }

    #[test]
    fn waiting_seek_runs_after_the_active_batch_before_another_prefetch() {
        let mut reads = BranchReads::new();
        reads
            .app
            .world_mut()
            .get_mut::<GitDiffQuery>(reads.page)
            .unwrap()
            .loading = true;
        reads.seek(1, 4, "file-4.rs");
        reads.next();
        reads.app.update();
        assert!(reads.paths().is_empty());

        reads
            .app
            .world_mut()
            .get_mut::<GitDiffQuery>(reads.page)
            .unwrap()
            .complete_load();
        reads.next();
        reads.app.update();

        assert_eq!(reads.paths(), [b"file-4.rs".to_vec()]);
        assert!(
            reads
                .app
                .world()
                .get::<PendingGitDiffSeek>(reads.page)
                .is_none()
        );
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
        app.add_plugins((
            MinimalPlugins,
            vmux_ecs::UiStatePlugin::<GitUiState>::default(),
            DiffPlugin,
        ));
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
                            lines: vec![BranchReads::line(DiffKind::Context, 1)],
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
                    rows: PrefetchBudget::WINDOW_ROWS,
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
        app.add_plugins((
            MinimalPlugins,
            vmux_ecs::UiStatePlugin::<GitUiState>::default(),
            DiffPlugin,
        ))
        .init_resource::<GitRuntime>();
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
                            lines: vec![BranchReads::line(DiffKind::Context, 1)],
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
                    cached_source: None,
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
                    rows: PrefetchBudget::WINDOW_ROWS,
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
        app.add_plugins((
            MinimalPlugins,
            vmux_ecs::UiStatePlugin::<GitUiState>::default(),
            DiffPlugin,
        ))
        .init_resource::<GitRuntime>();
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
                            lines: vec![BranchReads::line(DiffKind::Context, 1)],
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
                    cached_source: None,
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
        let query = app.world().get::<GitDiffQuery>(page).unwrap();
        assert_eq!(query.target, target);
        assert_eq!(query.page_revision, Some(2));
    }

    #[test]
    fn branch_diff_progress_loads_files_in_pages_without_repeating_a_seek() {
        let paths = (0..20)
            .map(|index| format!("file-{index}.rs").into_bytes())
            .collect();
        let mut reads = BranchReads::with_paths(paths, false);

        let first = reads.take_next_job();
        assert_eq!(first.branch_paths.len(), BranchDiffCursor::PATH_BATCH_SIZE);
        assert_eq!(first.branch_paths.first(), Some(&b"file-0.rs".to_vec()));
        assert_eq!(
            first.branch_paths.last(),
            Some(&format!("file-{}.rs", BranchDiffCursor::PATH_BATCH_SIZE - 1).into_bytes())
        );
        assert!(first.branch_has_more);
        assert!(reads.seek_job(b"file-19.rs").is_some());
        assert!(reads.seek_job(b"file-19.rs").is_none());
        reads.complete(
            &first.branch_paths,
            first.branch_top_line,
            40,
            true,
            true,
            None,
        );
        let second = reads.take_next_job();

        let expected = (BranchDiffCursor::PATH_BATCH_SIZE
            ..(BranchDiffCursor::PATH_BATCH_SIZE * 2).min(19))
            .map(|index| format!("file-{index}.rs").into_bytes())
            .collect::<Vec<_>>();
        assert_eq!(second.branch_paths, expected);
    }

    #[test]
    fn failed_branch_diff_page_remains_retryable() {
        let paths = (0..10)
            .map(|index| format!("file-{index}.rs").into_bytes())
            .collect();
        let mut reads = BranchReads::with_paths(paths, false);

        let first = reads.take_next_job();
        reads.complete(
            &first.branch_paths,
            first.branch_top_line,
            0,
            false,
            false,
            None,
        );
        assert_eq!(reads.continuations().len(), 1);
        let retry = reads.take_next_job();

        assert_eq!(retry.branch_paths, first.branch_paths);
        assert_eq!(retry.branch_top_line, first.branch_top_line);
        assert!(retry.branch_has_more);
        assert!(reads.continuations().is_empty());
    }

    #[test]
    fn completed_file_diff_releases_cached_source_lines() {
        let mut query = GitDiffQuery::for_test(1);
        let lines = Arc::new(vec![BranchReads::line(DiffKind::Add, 1); 20]);

        query.cache_source(
            Some(DiffSource::shared_source_lines(Arc::clone(&lines))),
            true,
        );
        assert!(query.cached_source.is_some());
        query.cache_source(Some(DiffSource::shared_source_lines(lines)), false);
        assert!(query.cached_source.is_none());
    }

    #[test]
    fn selected_branch_file_continues_before_older_batches_without_losing_them() {
        let paths = (0..17)
            .map(|index| format!("file-{index}.lock").into_bytes())
            .collect::<Vec<_>>();
        let mut reads = BranchReads::with_paths(paths, false);

        let first = reads.take_next_job();
        let lines = Arc::new(vec![BranchReads::line(DiffKind::Add, 1); 920]);
        reads.complete(
            &first.branch_paths,
            first.branch_top_line,
            800,
            false,
            true,
            Some(DiffSource::shared_source_lines(Arc::clone(&lines))),
        );
        let seek = reads.seek_job(b"file-16.lock").unwrap();
        reads.complete(
            &seek.branch_paths,
            seek.branch_top_line,
            400,
            false,
            true,
            Some(DiffSource::shared_source_lines(Arc::clone(&lines))),
        );
        let continuation = reads.take_next_job();

        assert_eq!(continuation.branch_paths, [b"file-16.lock".to_vec()]);
        assert_eq!(continuation.branch_top_line, 400);
        assert!(continuation.cached_source.is_some());

        reads.complete(
            &continuation.branch_paths,
            continuation.branch_top_line,
            520,
            true,
            true,
            continuation.cached_source,
        );
        let next = reads.take_next_job();

        assert_eq!(next.branch_paths, first.branch_paths);
        assert_eq!(next.branch_top_line, 800);
    }

    #[test]
    fn prefetch_follows_the_visible_file_before_resuming_an_earlier_batch() {
        let mut paths = Vec::new();
        for index in 0..8 {
            paths.push(format!("file-{index}.rs").into_bytes());
        }
        let mut reads = BranchReads::with_paths(paths, false);
        let first = reads.take_next_job();
        reads.complete(&first.branch_paths, 0, 640, false, true, None);
        let visible = reads.seek_job(b"file-4.rs").unwrap();
        reads.complete(&visible.branch_paths, 0, 20, true, true, None);

        for index in 5..8 {
            let nearby = reads.take_next_job();
            assert_eq!(
                nearby.branch_paths,
                [format!("file-{index}.rs").into_bytes()]
            );
            reads.complete(&nearby.branch_paths, 0, 20, true, true, None);
        }

        let earlier = reads.take_next_job();
        assert_eq!(earlier.branch_paths, first.branch_paths);
        assert_eq!(earlier.branch_top_line, 640);
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
        app.add_plugins((
            MinimalPlugins,
            vmux_ecs::UiStatePlugin::<GitUiState>::default(),
            DiffPlugin,
        ));
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
    fn a_new_target_supersedes_the_previous_diff_generation() {
        for restored in [false, true] {
            for discarded in [false, true] {
                let mut app = App::new();
                app.add_plugins((
                    MinimalPlugins,
                    vmux_ecs::UiStatePlugin::<GitUiState>::default(),
                    DiffPlugin,
                ));
                let mut entity = app
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
                let previous = app.world().get::<GitDiffQuery>(entity).unwrap().generation;
                if restored {
                    let cached = app.world().get::<GitDiffQuery>(entity).unwrap().clone();
                    entity = app
                        .world_mut()
                        .spawn((super::super::state::GitState::default(), cached))
                        .id();
                    app.update();
                }
                if discarded {
                    app.world_mut().entity_mut(entity).remove::<GitDiffQuery>();
                }
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
                assert!(
                    query.generation > previous,
                    "restored={restored} discarded={discarded}"
                );
                assert!(
                    !query.accepts(previous),
                    "restored={restored} discarded={discarded}"
                );
                assert_eq!(query.target.path, PathBuf::from("/repo/b.rs"));
            }
        }
    }

    #[test]
    fn restored_viewport_generation_is_not_reused_by_a_fresh_query() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_ecs::UiStatePlugin::<GitUiState>::default(),
            DiffPlugin,
        ));
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
        app.add_plugins((
            MinimalPlugins,
            vmux_ecs::UiStatePlugin::<GitUiState>::default(),
            DiffPlugin,
        ));
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
                            lines: vec![BranchReads::line(DiffKind::Context, 1)],
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
            cached_source: None,
            loading: false,
        };

        assert!(query.accepts_file(3, &file));

        query.file.as_mut().unwrap().refresh = 1;
        assert!(!query.accepts_file(3, &file));

        let replacement = super::super::status::FileGit::new("/repo/a.rs", 8);
        assert!(!query.accepts_file(3, &replacement));
    }
}
