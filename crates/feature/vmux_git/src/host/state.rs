use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::page::PageReady;

use crate::event::{
    DiffKind, GitBranchLog, GitBranchLogRequest, GitDiffViewport, GitOperationError,
    GitOperationResult, GitRepositoryRequest, GitRepositorySnapshot,
};
use crate::state::{GitCommandLogEntry, GitDiffRow, GitPageSnapshot, GitUiState};

use super::controller::GitController;
use super::directory::GitDirectoryNavigation;
use super::job::{BranchLogJob, RepositoryJob};
use super::job_runner::{GitJob, GitJobFailure};
use super::repository::GitRepository;
use super::watch::GitWatch;

type GitUiStateUpdates = vmux_ecs::host::UiState<GitUiState>;

const DIFF_CONTEXT_LINES: usize = 3;
const DIFF_REVEAL_LINES: usize = 20;

pub(super) struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_ecs::host::UiStatePlugin::<GitUiState>::default())
            .add_plugins(UiEventPlugin::<(GitRepositoryRequest, GitBranchLogRequest)>::default())
            .add_observer(page_ready)
            .add_observer(repository_request)
            .add_observer(branch_log_request)
            .add_systems(Update, publish.after(super::GitUpdateSet::Jobs));
    }
}

#[derive(Component, Default)]
#[require(GitUiStateUpdates, GitController, GitDirectoryNavigation)]
pub(super) struct GitState {
    snapshot: GitPageSnapshot,
    revealed_diff: Vec<(usize, usize)>,
}

impl GitState {
    pub(super) fn reset(&mut self, workspace: String) {
        self.snapshot = GitPageSnapshot {
            workspace,
            loading: true,
            ..Default::default()
        };
        self.revealed_diff.clear();
    }

    fn start_repository(&mut self, path: &Path) {
        self.snapshot.workspace = path.to_string_lossy().into_owned();
        self.snapshot.loading = true;
        self.snapshot.message.clear();
    }

    pub(super) fn set_repository(&mut self, event: GitRepositorySnapshot) {
        self.snapshot.workspace.clone_from(&event.repo_root);
        self.snapshot.repository = Some(event);
        self.snapshot.loading = false;
        self.snapshot.message.clear();
    }

    pub(super) fn start_directory(&mut self, path: &Path) {
        self.snapshot.workspace = path.to_string_lossy().into_owned();
        self.snapshot.loading = true;
        self.snapshot.message.clear();
    }

    pub(super) fn finish_directory(&mut self) {
        self.snapshot.repository = None;
        self.snapshot.loading = false;
        self.snapshot.message.clear();
    }

    pub(super) fn set_branch_log(&mut self, event: GitBranchLog) {
        self.snapshot.branch_log = Some(event);
    }

    pub(super) fn start_diff(&mut self, target_changed: bool) {
        self.snapshot.diff_loading = target_changed || self.snapshot.diff_viewport.is_none();
        if target_changed {
            self.snapshot.diff_viewport = None;
            self.snapshot.diff_rows.clear();
            self.revealed_diff.clear();
        }
    }

    pub(super) fn set_diff_viewport(&mut self, event: GitDiffViewport) {
        self.snapshot.diff_loading = false;
        self.snapshot.diff_viewport = Some(event);
        self.revealed_diff.clear();
        self.project_diff();
    }

    pub(super) fn reveal_diff(&mut self, start: u32, end: u32) -> bool {
        let range = (start as usize, end as usize);
        if range.0 >= range.1 || self.revealed_diff.contains(&range) {
            return false;
        }
        self.revealed_diff.push(range);
        self.project_diff();
        true
    }

    pub(super) fn start_fetch(&mut self) {
        self.snapshot.fetching = true;
    }

    pub(super) fn apply_result(&mut self, event: &GitOperationResult) {
        self.push_log(GitCommandLogEntry {
            operation: event.operation.clone(),
            message: event.message.clone(),
            ok: event.ok,
        });
        if event.operation == "fetch" {
            self.snapshot.fetching = false;
        }
        if event.ok {
            self.snapshot.message.clear();
        } else {
            self.snapshot.message.clone_from(&event.message);
        }
        self.snapshot.nonce = self.snapshot.nonce.wrapping_add(1);
        self.snapshot.result = Some(event.clone());
        self.snapshot.result_sequence = self.snapshot.result_sequence.wrapping_add(1).max(1);
    }

    pub(super) fn apply_error(&mut self, event: &GitOperationError) {
        self.push_log(GitCommandLogEntry {
            operation: String::new(),
            message: event.message.clone(),
            ok: false,
        });
        self.snapshot.loading = false;
        self.snapshot.fetching = false;
        self.snapshot.message.clone_from(&event.message);
    }

    pub(super) fn apply_workspace_error(&mut self, message: String) {
        self.push_log(GitCommandLogEntry {
            operation: String::new(),
            message: message.clone(),
            ok: false,
        });
        self.snapshot.loading = false;
        self.snapshot.fetching = false;
        self.snapshot.message = message;
    }

    pub(super) fn mark_changed(&mut self) -> Option<String> {
        self.snapshot.nonce = self.snapshot.nonce.wrapping_add(1);
        (!self.snapshot.workspace.is_empty()).then(|| self.snapshot.workspace.clone())
    }

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

    fn push_log(&mut self, entry: GitCommandLogEntry) {
        if self.snapshot.command_log.len() >= 24 {
            self.snapshot.command_log.remove(0);
        }
        self.snapshot.command_log.push(entry);
    }

    fn project_diff(&mut self) {
        let Some(viewport) = self.snapshot.diff_viewport.as_ref() else {
            self.snapshot.diff_rows.clear();
            return;
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
        for (start, end) in &self.revealed_diff {
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
        self.snapshot.diff_rows = rows;
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
    mut views: Query<&mut GitState>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let path: PathBuf = trigger.event().payload.path.clone().into();
    if let Ok(mut view) = views.get_mut(webview) {
        view.start_repository(&path);
    }
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
        Ref<'static, GitController>,
        Ref<'static, GitDirectoryNavigation>,
    ),
>;

fn publish(views: GitStateQuery, mut commands: Commands) {
    for (entity, view, controller, directory) in &views {
        if view.is_changed() {
            commands.trigger(vmux_ecs::host::UiStateWrite::<GitUiState>::from_event(
                entity,
                &view.snapshot,
            ));
        }
        if controller.is_changed() {
            commands.trigger(vmux_ecs::host::UiStateWrite::<GitUiState>::from_event(
                entity,
                controller.state(),
            ));
        }
        if directory.is_changed() {
            commands.trigger(vmux_ecs::host::UiStateWrite::<GitUiState>::from_event(
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

    fn state_with_change(line_count: u32, changed: usize) -> GitState {
        let mut lines = (1..=line_count)
            .map(|number| line(DiffKind::Context, number))
            .collect::<Vec<_>>();
        lines[changed].kind = DiffKind::Add;
        let mut state = GitState::default();
        state.set_diff_viewport(GitDiffViewport {
            generation: 1,
            first_line: 0,
            total_lines: line_count,
            lines,
            markers: Vec::new(),
            error: String::new(),
        });
        state
    }

    #[test]
    fn diff_projection_collapses_context_outside_changed_hunks() {
        let state = state_with_change(20, 9);

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
        let mut state = state_with_change(60, 49);

        assert!(state.reveal_diff(0, DIFF_REVEAL_LINES as u32));

        assert!(state.snapshot.diff_rows.contains(&GitDiffRow::Line(0)));
        assert!(state.snapshot.diff_rows.contains(&GitDiffRow::Gap {
            start: DIFF_REVEAL_LINES as u32,
            end: 46,
            reveal_start: DIFF_REVEAL_LINES as u32,
            reveal_end: (DIFF_REVEAL_LINES * 2) as u32,
        }));
    }
}
