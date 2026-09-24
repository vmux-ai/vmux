use std::path::Path;

use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_core::page::PageReady;

use crate::event::{
    GitBranchLog, GitDiffViewport, GitDirectorySnapshot, GitOperationError, GitOperationResult,
    GitRepositorySnapshot,
};
use crate::state::{GitCommandLogEntry, GitPageSnapshot, GitUiState};

use super::controller::GitController;

type GitUiStateUpdates = vmux_core::host::UiState<GitUiState>;

pub(super) struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_core::host::UiStatePlugin::<GitUiState>::default())
            .add_observer(on_page_ready)
            .add_systems(Update, publish_git_state.after(super::GitUpdateSet::Jobs));
    }
}

#[derive(Component, Default)]
#[require(GitUiStateUpdates, GitController)]
pub(super) struct GitState {
    snapshot: GitPageSnapshot,
}

impl GitState {
    pub(super) fn reset(&mut self, workspace: String) {
        self.snapshot = GitPageSnapshot {
            workspace,
            loading: true,
            ..Default::default()
        };
    }

    pub(super) fn start_repository(&mut self, path: &Path) {
        self.snapshot.workspace = path.to_string_lossy().into_owned();
        self.snapshot.loading = true;
        self.snapshot.message.clear();
    }

    pub(super) fn set_repository(&mut self, event: GitRepositorySnapshot) {
        self.snapshot.workspace.clone_from(&event.repo_root);
        self.snapshot.repository = Some(event);
        self.snapshot.directory = None;
        self.snapshot.directory_preview = None;
        self.snapshot.loading = false;
        self.snapshot.message.clear();
    }

    pub(super) fn start_directory(&mut self, path: &Path, preview: bool) {
        if preview {
            return;
        }
        self.snapshot.workspace = path.to_string_lossy().into_owned();
        self.snapshot.loading = true;
        self.snapshot.message.clear();
    }

    pub(super) fn set_directory(&mut self, event: GitDirectorySnapshot) {
        if event.preview {
            self.snapshot.directory_preview = Some(event);
            return;
        }
        self.snapshot.workspace.clone_from(&event.path);
        self.snapshot.repository = None;
        self.snapshot.directory = Some(event);
        self.snapshot.directory_preview = None;
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
        }
    }

    pub(super) fn set_diff_viewport(&mut self, event: GitDiffViewport) {
        self.snapshot.diff_loading = false;
        self.snapshot.diff_viewport = Some(event);
    }

    pub(super) fn start_fetch(&mut self) {
        self.snapshot.fetching = true;
    }

    pub(super) fn apply_result(&mut self, event: &GitOperationResult) {
        self.push_log(GitCommandLogEntry {
            operation: event.action.clone(),
            message: event.message.clone(),
            ok: event.ok,
        });
        if event.action == "fetch" {
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
}

fn on_page_ready(
    trigger: On<UiInput<PageReady>>,
    pages: Query<&vmux_core::PageMetadata>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(page) = pages.get(entity) else {
        return;
    };
    if !page.url.starts_with(crate::GIT_PAGE_URL) && page.url != crate::GIT_DOCUMENT_URL {
        return;
    }
    commands.entity(entity).insert(GitState::default());
}

fn publish_git_state(
    views: Query<(Entity, Ref<GitState>, Ref<GitController>)>,
    mut commands: Commands,
) {
    for (entity, view, controller) in &views {
        if !view.is_changed() && !controller.is_changed() {
            continue;
        }
        commands.trigger(vmux_core::host::UiStateWrite::<GitUiState>::from_event(
            entity,
            &view.snapshot,
        ));
        commands.trigger(vmux_core::host::UiStateWrite::<GitUiState>::from_event(
            entity,
            controller.state(),
        ));
    }
}
