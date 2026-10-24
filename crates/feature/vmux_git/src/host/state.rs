use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::PagePresented;
use vmux_ecs::page::PageReady;
use vmux_path::PathIdentity;

#[cfg(test)]
use crate::event::GitDiffViewport;
use crate::event::{
    GitOperationError, GitOperationResult, GitRepositoryRequest, GitRepositorySnapshot,
};
#[cfg(test)]
use crate::state::GitCatalogProgress;
use crate::state::{
    CheckedOutBranch, GitCommandLogEntry, GitPageControllerState, GitPageSnapshot, GitUiState,
    HeaderDropdown, RepositoryName, RepositoryPath,
};

use super::controller::{GitCommitResultSequence, PendingBranchCheckout, SelectionRevealRevision};
use super::directory::GitDirectoryNavigation;
use super::job::RepositoryJob;
use super::job_runner::GitJob;
use super::watch::GitWatch;

type GitUiStateUpdates = vmux_ecs::UiState<GitUiState>;

pub(super) struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_ecs::UiStatePlugin::<GitUiState>::default())
            .add_plugins(UiEventPlugin::<(GitRepositoryRequest,)>::default())
            .add_observer(page_ready)
            .add_observer(repository_request)
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
    GitUiStateUpdates,
    GitPageControllerState,
    HeaderDropdown,
    GitDirectoryNavigation,
    PendingBranchCheckout,
    SelectionRevealRevision,
    GitCommitResultSequence,
    GitDiffRevealRanges,
    GitDiffProjectionState
)]
pub(super) struct GitState {
    pub(super) snapshot: GitPageSnapshot,
}

#[derive(Component, Default)]
pub(super) struct GitDiffRevealRanges(pub(super) Vec<(usize, usize)>);

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

    pub(super) fn diff_revision(&self) -> u32 {
        self.snapshot.nonce
    }

    pub(super) fn repository(&self) -> Option<&GitRepositorySnapshot> {
        self.snapshot.repository.as_ref()
    }

    fn replace_repository(
        &mut self,
        mut repository: GitRepositorySnapshot,
        refresh_catalogs: bool,
    ) {
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
        self.snapshot.repository = Some(repository);
        if same_repository {
            if refresh_catalogs {
                self.snapshot.refresh_catalogs();
            }
        } else {
            self.snapshot.invalidate_catalogs();
        }
        self.snapshot.nonce = self.snapshot.nonce.wrapping_add(1).max(1);
        self.snapshot.loading = false;
        self.snapshot.message.clear();
    }
}

#[derive(SystemParam)]
pub(super) struct GitStates<'w, 's> {
    states: Query<'w, 's, &'static mut GitState>,
}

impl GitStates<'_, '_> {
    pub(super) fn contains(&self, entity: Entity) -> bool {
        self.states.contains(entity)
    }

    pub(super) fn repository_loaded_at(&self, entity: Entity, path: &str) -> bool {
        self.states.get(entity).is_ok_and(|state| {
            state.snapshot.workspace == path && state.snapshot.repository.is_some()
        })
    }

    pub(super) fn repository_loading_at(&self, entity: Entity, path: &str) -> bool {
        self.states
            .get(entity)
            .is_ok_and(|state| state.snapshot.workspace == path && state.snapshot.loading)
    }

    pub(super) fn repository_requested_at(
        &self,
        entity: Entity,
        path: &Path,
        revision: u32,
    ) -> bool {
        self.states.get(entity).is_ok_and(|state| {
            state.snapshot.nonce == revision
                && PathIdentity::lexical(Path::new(&state.snapshot.workspace))
                    == PathIdentity::lexical(path)
        })
    }

    pub(super) fn reset(&mut self, entity: Entity, workspace: String) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.snapshot = GitPageSnapshot {
            workspace,
            loading: true,
            ..Default::default()
        };
    }

    pub(super) fn start_repository(&mut self, entity: Entity, path: &Path) -> Option<u32> {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return None;
        };
        state.snapshot.workspace = path.to_string_lossy().into_owned();
        state.snapshot.loading = true;
        state.snapshot.message.clear();
        state.snapshot.nonce = state.snapshot.nonce.wrapping_add(1).max(1);
        Some(state.snapshot.nonce)
    }

    pub(super) fn set_repository(
        &mut self,
        entity: Entity,
        repository: GitRepositorySnapshot,
        refresh_catalogs: bool,
    ) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.replace_repository(repository, refresh_catalogs);
    }

    pub(super) fn start_directory(&mut self, entity: Entity, path: &Path) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.snapshot.workspace = path.to_string_lossy().into_owned();
        state.snapshot.loading = true;
        state.snapshot.message.clear();
        state.snapshot.nonce = state.snapshot.nonce.wrapping_add(1).max(1);
    }

    pub(super) fn finish_directory(&mut self, entity: Entity) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.snapshot.repository = None;
        state.snapshot.loading = false;
        state.snapshot.message.clear();
    }

    pub(super) fn start_fetch(&mut self, entity: Entity) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.snapshot.fetching = true;
    }

    pub(super) fn apply_result(
        &mut self,
        entity: Entity,
        event: &GitOperationResult,
    ) -> Option<(String, u32)> {
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
        Some((state.snapshot.workspace.clone(), state.snapshot.nonce))
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

    pub(super) fn mark_changed(&mut self, entity: Entity) -> Option<(String, u32)> {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return None;
        };
        state.snapshot.nonce = state.snapshot.nonce.wrapping_add(1).max(1);
        (!state.snapshot.workspace.is_empty())
            .then(|| (state.snapshot.workspace.clone(), state.snapshot.nonce))
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
        &mut GitState,
        Ref<GitDiffRevealRanges>,
        &mut GitDiffProjectionState,
    )>,
) {
    for (mut state, revealed, mut projection) in &mut pages {
        if !state.is_changed() && !revealed.is_changed() {
            continue;
        }
        if projection.current(&state.snapshot, &revealed.0) {
            continue;
        }
        state
            .snapshot
            .project_diff_rows(&revealed.0, GitPageSnapshot::DIFF_CONTEXT_LINES);
        projection.capture(&state.snapshot, &revealed.0);
    }
}

fn page_ready(
    trigger: On<UiInput<PageReady>>,
    pages: Query<&vmux_ecs::PageMetadata>,
    states: Query<(), With<GitState>>,
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
    if states.contains(entity) {
        return;
    }
    commands.entity(entity).insert(GitState::default());
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
        Option<&'static RepositoryPath>,
        Option<&'static RepositoryName>,
        Option<&'static CheckedOutBranch>,
    ),
    Changed<GitState>,
>;

fn sync_selection(pages: ChangedGitSelections, mut commands: Commands) {
    for (entity, state, current_path, current_name, current_branch) in &pages {
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

fn repository_request(
    trigger: On<UiInput<GitRepositoryRequest>>,
    mut watch: Option<NonSendMut<GitWatch>>,
    presented: Query<(), With<PagePresented>>,
    mut states: GitStates,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let path: PathBuf = trigger.event().payload.path.clone().into();
    let Some(revision) = states.start_repository(webview, &path) else {
        return;
    };
    if !presented.contains(webview) {
        if let Some(watch) = watch.as_deref_mut() {
            watch.release_subscription(webview);
        }
        return;
    }
    if let Some(mut watch) = watch {
        watch.release_subscription(webview);
    }
    commands.spawn((
        GitJob::new(webview),
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
        Ref<'static, HeaderDropdown>,
        Ref<'static, GitDirectoryNavigation>,
    ),
>;

fn publish(views: GitStateQuery, mut commands: Commands) {
    for (entity, view, controller, header_dropdown, directory) in &views {
        if view.is_changed() {
            commands.trigger(vmux_ecs::UiStateWrite::<GitUiState>::from_event(
                entity,
                &view.snapshot,
            ));
        }
        if controller.is_changed() {
            commands.trigger(vmux_ecs::UiStateWrite::<GitUiState>::from_event(
                entity,
                &*controller,
            ));
        }
        if header_dropdown.is_changed() {
            commands.trigger(vmux_ecs::UiStateWrite::<GitUiState>::from_event(
                entity,
                &*header_dropdown,
            ));
        }
        if directory.is_changed() {
            commands.trigger(vmux_ecs::UiStateWrite::<GitUiState>::from_event(
                entity,
                &directory.state(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
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

    fn app_with_change(line_count: u32, changed: usize) -> (App, Entity) {
        let mut lines = (1..=line_count)
            .map(|number| line(DiffKind::Context, number))
            .collect::<Vec<_>>();
        lines[changed].kind = DiffKind::Add;
        let mut app = App::new();
        app.add_systems(Update, project_diff);
        let entity = app
            .world_mut()
            .spawn((
                GitState {
                    snapshot: GitPageSnapshot {
                        diff_viewport: Some(GitDiffViewport {
                            generation: 1,
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
    fn inactive_git_page_defers_repository_work_until_activation() {
        let repo = test_repo::init();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_observer(repository_request);
        app.insert_non_send(GitWatch::test());
        let page = app
            .world_mut()
            .spawn((vmux_ecs::PageMetadata::default(), GitState::default()))
            .id();

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GitRepositoryRequest {
                path: repo.path().to_string_lossy().into_owned(),
            },
        });
        app.update();

        let state = app.world().get::<GitState>(page).unwrap();
        assert_eq!(state.workspace(), repo.path().to_string_lossy());
        assert!(state.snapshot.loading);
        assert!(!app.world().non_send::<GitWatch>().has_subscription(page));
        let mut jobs = app.world_mut().query::<&RepositoryJob>();
        assert_eq!(jobs.iter(app.world()).count(), 0);
    }

    #[test]
    fn repository_status_refresh_preserves_loaded_catalogs() {
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
                    loaded: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        };
        let next = GitRepositorySnapshot::load_summary(repo.path()).unwrap();

        state.replace_repository(next.snapshot, false);

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
