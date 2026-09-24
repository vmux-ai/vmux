use std::path::Path;

use bevy::prelude::*;
use bevy_cef::prelude::BinReceive;
use vmux_core::page::PageReady;

use crate::event::{
    GitBranchLogEvent, GitDiffViewportEvent, GitDirectoryEvent, GitErrorEvent, GitRepositoryEvent,
    GitResultEvent,
};
use crate::state::{GitCommandLogEntry, GitPageSnapshot, GitUiState};

pub(super) struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_core::host::UiStatePlugin::<GitUiState>::default())
            .add_observer(on_page_ready)
            .add_systems(Update, publish_git_ui_state);
    }
}

type GitUiStateUpdates = vmux_core::host::UiStateUpdates<GitUiState>;

#[derive(Component, Default)]
#[require(GitUiStateUpdates)]
pub(super) struct GitState {
    snapshot: GitPageSnapshot,
}

impl GitState {
    pub(super) fn start_repository(&mut self, path: &Path) {
        self.snapshot.workspace = path.to_string_lossy().into_owned();
        self.snapshot.loading = true;
        self.snapshot.message.clear();
    }

    pub(super) fn set_repository(&mut self, event: GitRepositoryEvent) {
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

    pub(super) fn set_directory(&mut self, event: GitDirectoryEvent) {
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

    pub(super) fn set_branch_log(&mut self, event: GitBranchLogEvent) {
        self.snapshot.branch_log = Some(event);
    }

    pub(super) fn set_diff_viewport(&mut self, event: GitDiffViewportEvent) {
        self.snapshot.diff_viewport = Some(event);
    }

    pub(super) fn start_fetch(&mut self) {
        self.snapshot.fetching = true;
    }

    pub(super) fn apply_result(&mut self, event: &GitResultEvent) {
        self.push_log(GitCommandLogEntry {
            action: event.action.clone(),
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

    pub(super) fn apply_error(&mut self, event: &GitErrorEvent) {
        self.push_log(GitCommandLogEntry {
            action: String::new(),
            message: event.message.clone(),
            ok: false,
        });
        self.snapshot.loading = false;
        self.snapshot.fetching = false;
        self.snapshot.message.clone_from(&event.message);
    }

    pub(super) fn mark_changed(&mut self) -> Option<String> {
        self.snapshot.nonce = self.snapshot.nonce.wrapping_add(1);
        (!self.snapshot.workspace.is_empty()).then(|| self.snapshot.workspace.clone())
    }

    pub(super) fn workspace(&self) -> &str {
        &self.snapshot.workspace
    }

    fn push_log(&mut self, entry: GitCommandLogEntry) {
        if self.snapshot.command_log.len() >= 24 {
            self.snapshot.command_log.remove(0);
        }
        self.snapshot.command_log.push(entry);
    }
}

fn on_page_ready(
    trigger: On<BinReceive<PageReady>>,
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

fn publish_git_ui_state(
    views: Query<(Entity, &GitState), Changed<GitState>>,
    mut commands: Commands,
) {
    for (entity, view) in &views {
        GitUiStateUpdates::write(&mut commands, entity, &view.snapshot);
    }
}
