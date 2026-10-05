use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::page::PageReady;

use crate::event::GitDiffRow;
#[cfg(test)]
use crate::event::GitDiffViewport;
use crate::event::{
    DiffKind, GitBranchLog, GitBranchLogRequest, GitOperationError, GitOperationResult,
    GitRepositoryRequest, GitRepositorySnapshot,
};
use crate::state::{GitCommandLogEntry, GitPageControllerState, GitPageSnapshot, GitUiState};

use super::controller::{GitCommitResultSequence, PendingBranchCheckout, SelectionRevealRevision};
use super::directory::GitDirectoryNavigation;
use super::job::{BranchLogJob, RepositoryJob};
use super::job_runner::{GitJob, GitJobFailure};
use super::repository::GitRepository;
use super::watch::GitWatch;

type GitUiStateUpdates = vmux_ecs::UiState<GitUiState>;

const DIFF_CONTEXT_LINES: usize = 3;
const DIFF_REVEAL_LINES: usize = 20;

pub(super) struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_ecs::UiStatePlugin::<GitUiState>::default())
            .add_plugins(UiEventPlugin::<(GitRepositoryRequest, GitBranchLogRequest)>::default())
            .add_observer(page_ready)
            .add_observer(repository_request)
            .add_observer(branch_log_request)
            .add_systems(
                Update,
                (project_diff, publish)
                    .chain()
                    .after(super::controller::ControllerSet::BranchLog),
            );
    }
}

#[derive(Component, Default)]
#[require(
    GitUiStateUpdates,
    GitPageControllerState,
    GitDirectoryNavigation,
    PendingBranchCheckout,
    SelectionRevealRevision,
    GitCommitResultSequence,
    GitDiffRevealRanges
)]
pub(super) struct GitState {
    pub(super) snapshot: GitPageSnapshot,
}

#[derive(Component, Default)]
pub(super) struct GitDiffRevealRanges(pub(super) Vec<(usize, usize)>);

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

    pub(super) fn branch_log(&self) -> Option<&GitBranchLog> {
        self.snapshot.branch_log.as_ref()
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

    pub(super) fn workspace(&self, entity: Entity) -> Option<String> {
        self.states
            .get(entity)
            .ok()
            .map(|state| state.snapshot.workspace.clone())
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

    pub(super) fn start_repository(&mut self, entity: Entity, path: &Path) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.snapshot.workspace = path.to_string_lossy().into_owned();
        state.snapshot.loading = true;
        state.snapshot.message.clear();
    }

    pub(super) fn set_repository(&mut self, entity: Entity, event: GitRepositorySnapshot) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.snapshot.workspace.clone_from(&event.repo_root);
        state.snapshot.repository = Some(event);
        state.snapshot.loading = false;
        state.snapshot.message.clear();
    }

    pub(super) fn start_directory(&mut self, entity: Entity, path: &Path) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.snapshot.workspace = path.to_string_lossy().into_owned();
        state.snapshot.loading = true;
        state.snapshot.message.clear();
    }

    pub(super) fn finish_directory(&mut self, entity: Entity) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.snapshot.repository = None;
        state.snapshot.loading = false;
        state.snapshot.message.clear();
    }

    pub(super) fn set_branch_log(&mut self, entity: Entity, event: GitBranchLog) {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return;
        };
        state.snapshot.branch_log = Some(event);
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
    ) -> Option<String> {
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
        } else {
            state.snapshot.message.clone_from(&event.message);
        }
        state.snapshot.nonce = state.snapshot.nonce.wrapping_add(1);
        state.snapshot.result = Some(event.clone());
        state.snapshot.result_sequence = state.snapshot.result_sequence.wrapping_add(1).max(1);
        Some(state.snapshot.workspace.clone())
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

    pub(super) fn mark_changed(&mut self, entity: Entity) -> Option<String> {
        let Ok(mut state) = self.states.get_mut(entity) else {
            return None;
        };
        state.snapshot.nonce = state.snapshot.nonce.wrapping_add(1);
        (!state.snapshot.workspace.is_empty()).then(|| state.snapshot.workspace.clone())
    }

    fn push_log(state: &mut GitState, entry: GitCommandLogEntry) {
        if state.snapshot.command_log.len() >= 24 {
            state.snapshot.command_log.remove(0);
        }
        state.snapshot.command_log.push(entry);
    }
}

fn project_diff(mut pages: Query<(&mut GitState, Ref<GitDiffRevealRanges>)>) {
    for (mut state, revealed) in &mut pages {
        if !state.is_changed() && !revealed.is_changed() {
            continue;
        }
        let Some(viewport) = state.snapshot.diff_viewport.as_ref() else {
            if !state.snapshot.diff_rows.is_empty() {
                state.snapshot.diff_rows.clear();
            }
            continue;
        };
        let lines = &viewport.lines;
        let mut visible = vec![false; lines.len()];
        for (index, line) in lines.iter().enumerate() {
            if matches!(line.kind, DiffKind::Context) {
                continue;
            }
            let start = index.saturating_sub(DIFF_CONTEXT_LINES);
            let end = (index + DIFF_CONTEXT_LINES + 1).min(lines.len());
            visible[start..end].fill(true);
        }
        for (start, end) in &revealed.0 {
            let start = (*start).min(lines.len());
            let end = (*end).min(lines.len());
            if start < end {
                visible[start..end].fill(true);
            }
        }

        let mut rows = Vec::new();
        let mut index = 0;
        while index < lines.len() {
            if visible[index] {
                rows.push(GitDiffRow::Line(index as u32));
                index += 1;
                continue;
            }
            let start = index;
            while index < lines.len() && !visible[index] {
                index += 1;
            }
            let hidden = index - start;
            let (reveal_start, reveal_end) = if hidden <= DIFF_REVEAL_LINES {
                (start, index)
            } else if start == 0 {
                (index - DIFF_REVEAL_LINES, index)
            } else {
                (start, start + DIFF_REVEAL_LINES)
            };
            rows.push(GitDiffRow::Gap {
                start: start as u32,
                end: index as u32,
                reveal_start: reveal_start as u32,
                reveal_end: reveal_end as u32,
            });
        }
        if state.snapshot.diff_rows != rows {
            state.snapshot.diff_rows = rows;
        }
    }
}

fn page_ready(
    trigger: On<UiInput<PageReady>>,
    pages: Query<&vmux_ecs::PageMetadata>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(page) = pages.get(entity) else {
        return;
    };
    if !page.url.starts_with(crate::GIT_PAGE_URL) && page.url != super::GitPlugin::MANIFEST.url {
        return;
    }
    commands.entity(entity).insert(GitState::default());
}

fn repository_request(
    trigger: On<UiInput<GitRepositoryRequest>>,
    watch: Option<NonSendMut<GitWatch>>,
    mut pages: Query<&mut vmux_ecs::PageMetadata>,
    mut states: GitStates,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let path: PathBuf = trigger.event().payload.path.clone().into();
    states.start_repository(webview, &path);
    let repo_root = if let Some(mut watch) = watch {
        match watch.subscribe(webview, &path) {
            Ok(repo_root) => repo_root,
            Err(error) => {
                commands.trigger(GitJobFailure {
                    webview,
                    message: error.0,
                });
                return;
            }
        }
    } else {
        match GitRepository::discover(&path) {
            Ok(repository) => repository.path().to_path_buf(),
            Err(error) => {
                commands.trigger(GitJobFailure {
                    webview,
                    message: error.0,
                });
                return;
            }
        }
    };
    if let Ok(mut page) = pages.get_mut(webview)
        && let Some(url) = crate::GitUrl::from_path(&repo_root)
        && page.url != url
    {
        page.url = url;
    }
    commands.spawn((GitJob::new(webview), RepositoryJob { path }));
}

fn branch_log_request(trigger: On<UiInput<GitBranchLogRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(request.repo_root.clone()),
        BranchLogJob {
            branch: request.branch.clone(),
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
    use crate::event::DiffLine;

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
            .push((0, DIFF_REVEAL_LINES));
        app.update();
        let state = app.world().get::<GitState>(entity).unwrap();

        assert!(state.snapshot.diff_rows.contains(&GitDiffRow::Line(0)));
        assert!(state.snapshot.diff_rows.contains(&GitDiffRow::Gap {
            start: DIFF_REVEAL_LINES as u32,
            end: 46,
            reveal_start: DIFF_REVEAL_LINES as u32,
            reveal_end: (DIFF_REVEAL_LINES * 2) as u32,
        }));
    }
}
