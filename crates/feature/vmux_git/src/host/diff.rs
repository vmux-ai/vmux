use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::event::{
    DiffKind, DiffLine, GitBranchFilesNextRequest, GitBranchFilesRetryRequest, GitDiffNextRequest,
    GitDiffRetryRequest, GitDiffRevealRequest, GitLineMarker, GitLineStatus,
};
use crate::state::{GitPageControllerState, GitPanel};

use super::GitUpdateSet;
use super::job::{BranchFilesJob, DiffJob, DiffScope};
use super::job_runner::{GitJob, QueuedGitJobs};
use super::repository::{BranchFilesCatalog, BranchFilesPage, GitRepository};

const DIFF_WINDOW_ROWS: u32 = 800;

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
            .add_observer(retry_diff)
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
    if let Ok((_, mut revealed)) = states.get_mut(entity) {
        let range = (request.start as usize, request.end as usize);
        if range.0 < range.1 && !revealed.0.contains(&range) {
            revealed.0.push(range);
        }
        return;
    }
    files.reveal_diff(entity, request.start, request.end);
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
            GitPanel::Files | GitPanel::Stash if !controller.selected_abs_path.is_empty() => {
                Some(Self {
                    repo_root,
                    path: PathBuf::from(&controller.selected_abs_path),
                    path_bytes: controller.selected_path_bytes.clone(),
                    scope: DiffScope::File,
                })
            }
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
struct PendingGitDiff {
    target: GitDiffTarget,
    file: Option<FileDiffIdentity>,
    page_revision: Option<u32>,
}

#[derive(Component)]
struct PendingGitDiffSeek {
    path_bytes: Vec<u8>,
}

#[derive(Clone, Copy)]
struct FileDiffIdentity {
    document: u64,
    refresh: u64,
}

#[derive(Component)]
pub(super) struct GitDiffQuery {
    target: GitDiffTarget,
    generation: u64,
    file: Option<FileDiffIdentity>,
    page_revision: Option<u32>,
    branch: Option<BranchDiffProgress>,
}

#[derive(Clone)]
struct BranchDiffProgress {
    paths: Vec<Vec<u8>>,
    loaded: HashSet<Vec<u8>>,
    in_flight: HashSet<Vec<u8>>,
    next: usize,
}

impl BranchDiffProgress {
    const PAGE_SIZE: usize = 2;

    fn new(paths: Vec<Vec<u8>>) -> Self {
        Self {
            paths,
            loaded: HashSet::new(),
            in_flight: HashSet::new(),
            next: 0,
        }
    }

    fn next(&mut self) -> (Vec<Vec<u8>>, bool) {
        let mut paths = Vec::new();
        while self.next < self.paths.len() && paths.len() < Self::PAGE_SIZE {
            let path = self.paths[self.next].clone();
            self.next += 1;
            if !self.loaded.contains(&path) && self.in_flight.insert(path.clone()) {
                paths.push(path);
            }
        }
        self.advance();
        (paths, self.next < self.paths.len())
    }

    fn seek(&mut self, path: &[u8]) -> Option<(Vec<Vec<u8>>, bool)> {
        if !self.paths.iter().any(|candidate| candidate == path)
            || self.loaded.contains(path)
            || !self.in_flight.insert(path.to_vec())
        {
            return None;
        }
        Some((vec![path.to_vec()], self.has_more()))
    }

    fn contains(&self, path: &[u8]) -> bool {
        self.loaded.contains(path) || self.in_flight.contains(path)
    }

    fn complete(&mut self, paths: &[Vec<u8>], succeeded: bool) {
        for path in paths {
            self.in_flight.remove(path);
            if succeeded {
                self.loaded.insert(path.clone());
            }
        }
        if !succeeded {
            self.next = 0;
        }
        self.advance();
    }

    fn has_more(&self) -> bool {
        self.paths[self.next.min(self.paths.len())..]
            .iter()
            .any(|path| !self.loaded.contains(path) && !self.in_flight.contains(path))
    }

    fn advance(&mut self) {
        while self.next < self.paths.len()
            && (self.loaded.contains(&self.paths[self.next])
                || self.in_flight.contains(&self.paths[self.next]))
        {
            self.next += 1;
        }
    }
}

#[derive(Component)]
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
}

impl GitDiffQuery {
    pub(super) fn accepts(&self, generation: u64) -> bool {
        self.generation == generation
    }

    pub(super) fn accepts_file(&self, generation: u64, file: &super::status::FileGit) -> bool {
        self.accepts(generation)
            && self
                .file
                .is_some_and(|identity| file.accepts(identity.document, identity.refresh))
    }

    fn next_branch(&mut self) -> Option<(Vec<Vec<u8>>, bool)> {
        self.branch.as_mut().map(BranchDiffProgress::next)
    }

    fn seek_branch(&mut self, path: &[u8]) -> Option<(Vec<Vec<u8>>, bool)> {
        self.branch.as_mut()?.seek(path)
    }

    fn contains_branch_path(&self, path: &[u8]) -> bool {
        self.branch
            .as_ref()
            .is_some_and(|branch| branch.contains(path))
    }

    pub(super) fn complete_branch(&mut self, paths: &[Vec<u8>], succeeded: bool) -> bool {
        let Some(branch) = self.branch.as_mut() else {
            return false;
        };
        branch.complete(paths, succeeded);
        branch.has_more()
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
        Ref<'static, super::state::GitState>,
        Ref<'static, GitPageControllerState>,
        Option<Ref<'static, GitBranchFilesQuery>>,
        Option<&'static PendingGitDiff>,
        Option<&'static GitDiffQuery>,
    ),
>;

fn sync_page_targets(pages: GitDiffPages, mut commands: Commands) {
    for (entity, page, state, controller, branch_files, pending, current) in &pages {
        if !crate::GitUrl::is_page(&page.url) {
            continue;
        }
        if !state.is_changed()
            && !controller.is_changed()
            && branch_files
                .as_ref()
                .is_none_or(|query| !query.is_changed())
        {
            continue;
        }
        let Some(target) = GitDiffTarget::for_page(&state, &controller) else {
            commands
                .entity(entity)
                .remove::<(PendingGitDiff, PendingGitDiffSeek)>();
            continue;
        };
        let page_revision = state.diff_revision();
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
        if state.snapshot.branch_files_reference == controller.selected_branch
            && current.is_some_and(|current| {
                current.current(&repo_root, &controller.selected_branch, page_revision)
            })
        {
            continue;
        }
        let generation = current
            .map(|current| current.generation)
            .unwrap_or_default()
            .wrapping_add(1)
            .max(1);
        let reference_changed = state.snapshot.branch_files_reference != controller.selected_branch;
        if reference_changed {
            state.snapshot.branch_files.clear();
            state.snapshot.branch_files_total = 0;
            state.snapshot.branch_files_has_more = false;
            state.snapshot.diff_viewport = None;
            state.snapshot.diff_rows.clear();
            state.snapshot.diff_loading = true;
        }
        state
            .snapshot
            .branch_files_reference
            .clone_from(&controller.selected_branch);
        state.snapshot.branch_files_loading = true;
        state.snapshot.branch_files_error.clear();
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
    state.snapshot.branch_files.extend(page.files);
    state.snapshot.branch_files_total = page.total;
    state.snapshot.branch_files_has_more = page.has_more;
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
    state.snapshot.branch_files_error.clear();
    state.snapshot.branch_files_loading = false;
    state.snapshot.diff_loading = true;
    state.snapshot.diff_viewport = None;
    state.snapshot.diff_rows.clear();
    commands.entity(entity).remove::<GitBranchFilesQuery>();
}

fn load_next(
    trigger: On<UiInput<GitDiffNextRequest>>,
    mut pages: Query<(&mut super::state::GitState, &mut GitDiffQuery)>,
    sources: Query<&GitDiffSource>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((mut state, mut query)) = pages.get_mut(entity) else {
        return;
    };
    let Some(viewport) = state.snapshot.diff_viewport.as_ref() else {
        return;
    };
    let loaded = viewport
        .first_line
        .saturating_add(u32::try_from(viewport.lines.len()).unwrap_or(u32::MAX));
    if state.snapshot.diff_loading || !viewport.has_more {
        return;
    }
    let content = sources
        .get(entity)
        .ok()
        .filter(|source| source.dirty)
        .map(|source| source.content.clone());
    state.snapshot.diff_loading = true;
    let (branch_paths, branch_has_more) = query.next_branch().unwrap_or_default();
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(query.target.repo_root.clone()),
        DiffJob {
            path: query.target.path.clone(),
            scope: query.target.scope.clone(),
            branch_paths,
            branch_has_more,
            generation: query.generation,
            top_line: loaded,
            rows: DIFF_WINDOW_ROWS,
            content,
        },
    ));
}

fn retry_diff(
    trigger: On<UiInput<GitDiffRetryRequest>>,
    mut pages: Query<(
        &mut super::state::GitState,
        &GitDiffQuery,
        &mut super::state::GitDiffRevealRanges,
    )>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((mut state, query, mut revealed)) = pages.get_mut(entity) else {
        return;
    };
    state.snapshot.diff_loading = true;
    state.snapshot.diff_viewport = None;
    state.snapshot.diff_rows.clear();
    revealed.0.clear();
    commands.entity(entity).insert(PendingGitDiff {
        target: query.target.clone(),
        file: query.file,
        page_revision: query.page_revision,
    });
}

fn start_pending_seek(
    mut pages: Query<(
        Entity,
        &mut super::state::GitState,
        &mut GitDiffQuery,
        &PendingGitDiffSeek,
    )>,
    mut commands: Commands,
) {
    for (entity, mut state, mut query, pending) in &mut pages {
        if state.snapshot.diff_loading || !matches!(query.target.scope, DiffScope::Branch(_)) {
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
        let Some((branch_paths, branch_has_more)) = batch else {
            continue;
        };
        state.snapshot.diff_loading = true;
        commands.spawn((
            GitJob::new(entity),
            GitRepository::at(query.target.repo_root.clone()),
            DiffJob {
                path: query.target.path.clone(),
                scope: query.target.scope.clone(),
                branch_paths,
                branch_has_more,
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
        let generation = current
            .map(|current| current.generation)
            .unwrap_or_default()
            .wrapping_add(1)
            .max(1);
        if let Ok((mut page, mut revealed)) = pages.get_mut(entity) {
            page.snapshot.diff_loading = target_changed || page.snapshot.diff_viewport.is_none();
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
        let target = pending.target.clone();
        let content = source
            .filter(|source| source.dirty)
            .map(|source| source.content.clone());
        queued_diffs.despawn_for(entity, &mut commands);
        let branch = match &target.scope {
            DiffScope::Branch(_) => Some(BranchDiffProgress::new(
                branch_files
                    .map(|query| query.catalog.paths())
                    .unwrap_or_default(),
            )),
            DiffScope::File | DiffScope::Commit(_) => None,
        };
        let mut query = GitDiffQuery {
            target: target.clone(),
            generation,
            file: pending.file,
            page_revision: pending.page_revision,
            branch,
        };
        let (branch_paths, branch_has_more) = query.next_branch().unwrap_or_default();
        commands
            .entity(entity)
            .remove::<PendingGitDiff>()
            .insert(query);
        commands.spawn((
            GitJob::new(entity),
            GitRepository::at(target.repo_root),
            DiffJob {
                path: target.path,
                scope: target.scope,
                branch_paths,
                branch_has_more,
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
    use crate::event::StyledSpan;

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
                super::super::state::GitState::default(),
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
        let mut jobs = app.world_mut().query::<&DiffJob>();
        assert_eq!(jobs.iter(app.world()).count(), 1);
    }

    #[test]
    fn branch_diff_progress_loads_files_in_pages_without_repeating_a_seek() {
        let paths = (0..10)
            .map(|index| format!("file-{index}.rs").into_bytes())
            .collect();
        let mut progress = BranchDiffProgress::new(paths);

        let (first, first_has_more) = progress.next();
        assert_eq!(first.len(), BranchDiffProgress::PAGE_SIZE);
        assert!(first_has_more);
        assert!(progress.seek(b"file-9.rs").is_some());
        assert!(progress.seek(b"file-9.rs").is_none());
        let (second, second_has_more) = progress.next();

        assert_eq!(second, [b"file-2.rs".to_vec(), b"file-3.rs".to_vec()]);
        assert!(second_has_more);
    }

    #[test]
    fn failed_branch_diff_page_remains_retryable() {
        let paths = (0..10)
            .map(|index| format!("file-{index}.rs").into_bytes())
            .collect();
        let mut progress = BranchDiffProgress::new(paths);

        let (first, _) = progress.next();
        progress.complete(&first, false);
        let (retry, has_more) = progress.next();

        assert_eq!(retry, first);
        assert!(has_more);
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
        app.add_systems(Update, sync_branch_files);
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
            file: Some(FileDiffIdentity {
                document: 7,
                refresh: 0,
            }),
            page_revision: None,
            branch: None,
        };

        assert!(query.accepts_file(3, &file));

        query.file.as_mut().unwrap().refresh = 1;
        assert!(!query.accepts_file(3, &file));

        let replacement = super::super::status::FileGit::new("/repo/a.rs", 8);
        assert!(!query.accepts_file(3, &replacement));
    }
}
