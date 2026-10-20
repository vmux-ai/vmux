use bevy_ecs::prelude::Component;
use vmux_api::space::ProjectRow;

use crate::event::{GitDiffRow, GitDiffViewport, GitOperationResult, GitRepositorySnapshot};

#[vmux_api::contract(Copy, Eq, Default)]
pub enum GitPanel {
    Files,
    #[default]
    Branches,
    Commits,
    Stash,
}

impl GitPanel {
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "0" | "1" => Some(Self::Files),
            "2" => Some(Self::Branches),
            "3" => Some(Self::Commits),
            "4" => Some(Self::Stash),
            _ => None,
        }
    }

    pub fn next(self, reverse: bool) -> Self {
        match (self, reverse) {
            (Self::Files, false) => Self::Branches,
            (Self::Branches, false) => Self::Commits,
            (Self::Commits, false) => Self::Stash,
            (Self::Stash, false) => Self::Files,
            (Self::Files, true) => Self::Stash,
            (Self::Branches, true) => Self::Files,
            (Self::Commits, true) => Self::Branches,
            (Self::Stash, true) => Self::Commits,
        }
    }
}

#[vmux_api::contract(Copy, Eq, Default)]
pub enum GitBranchCollection {
    #[default]
    Local,
    Remote,
    Tags,
}

impl GitBranchCollection {
    pub fn references(self, repository: &GitRepositorySnapshot) -> Vec<String> {
        match self {
            Self::Local => repository
                .branches
                .iter()
                .map(|entry| entry.name.clone())
                .collect(),
            Self::Remote => repository
                .remote_branches
                .iter()
                .map(|entry| entry.name.clone())
                .collect(),
            Self::Tags => repository
                .tags
                .iter()
                .map(|entry| entry.name.clone())
                .collect(),
        }
    }

    pub fn selected_reference(self, repository: &GitRepositorySnapshot, selected: &str) -> String {
        let references = self.references(repository);
        if references.iter().any(|reference| reference == selected) {
            return selected.to_string();
        }
        if self == Self::Local
            && let Some(current) = repository.branches.iter().find(|entry| entry.current)
        {
            return current.name.clone();
        }
        references.into_iter().next().unwrap_or_default()
    }
}

#[vmux_api::contract(Eq)]
pub enum GitBranchPrompt {
    Create { base: String },
    Delete { branch: String },
}

#[vmux_api::contract(Default, Eq)]
pub struct GitOperationEligibility {
    pub stage_all: bool,
    pub stash: bool,
    pub amend: bool,
    pub toggle_stage: bool,
    pub discard: bool,
    pub commit: bool,
    pub push: bool,
    pub checkout_branch: bool,
    pub create_branch: bool,
    pub delete_branch: bool,
    pub rebase: bool,
    pub merge: bool,
    pub fast_forward: bool,
    pub checkout_commit: bool,
    pub cherry_pick: bool,
    pub revert_commit: bool,
    pub stash_pop: bool,
    pub stash_drop: bool,
}

#[derive(Component)]
#[vmux_api::contract(Default, Eq)]
pub struct GitPageControllerState {
    pub selected_path: String,
    pub selected_path_bytes: Vec<u8>,
    pub selected_abs_path: String,
    pub selected_commit: String,
    pub selected_branch: String,
    pub selected_branch_path: String,
    pub selected_branch_path_bytes: Vec<u8>,
    pub branch_collection: GitBranchCollection,
    pub selected_stash: String,
    pub confirm_discard: Vec<u8>,
    pub focused_panel: GitPanel,
    pub shortcut_help_visible: bool,
    pub branch_prompt: Option<GitBranchPrompt>,
    pub branch_draft: String,
    pub commit_message: String,
    pub commit_pending: String,
    pub operations: GitOperationEligibility,
}

#[vmux_api::contract(Eq)]
pub struct GitSelectionReveal {
    pub id: String,
    pub revision: u64,
}

#[vmux_api::contract]
pub struct GitPageContext {
    pub working_directory: String,
    pub page_url: String,
}

#[vmux_api::contract]
pub struct GitRepositoryPicked {
    pub path: String,
}

#[vmux_api::contract]
pub struct GitWorkspaceChanged {
    pub path: String,
    pub branch: String,
    pub error: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct GitProjects {
    pub rows: Vec<ProjectRow>,
}

#[vmux_api::contract]
pub struct GitCommandLogEntry {
    pub operation: String,
    pub message: String,
    pub ok: bool,
}

#[vmux_api::contract(Eq)]
pub struct GitDirectoryState {
    pub path: String,
    pub parent_entries: Vec<vmux_ecs::event::FileDirEntry>,
    pub entries: Vec<vmux_ecs::event::FileDirEntry>,
    pub children: Option<Vec<vmux_ecs::event::FileDirEntry>>,
    pub selected: u32,
    pub show_hidden: bool,
}

impl Default for GitDirectoryState {
    fn default() -> Self {
        Self {
            path: String::new(),
            parent_entries: Vec::new(),
            entries: Vec::new(),
            children: None,
            selected: 0,
            show_hidden: true,
        }
    }
}

#[vmux_api::contract]
pub struct GitPageSnapshot {
    pub workspace: String,
    pub repository: Option<GitRepositorySnapshot>,
    pub diff_viewport: Option<GitDiffViewport>,
    pub diff_rows: Vec<GitDiffRow>,
    pub diff_loading: bool,
    pub branch_files: Vec<crate::event::GitBranchFileEntry>,
    pub branch_files_reference: String,
    pub branch_files_total: u32,
    pub branch_files_loading: bool,
    pub branch_files_has_more: bool,
    pub branch_files_error: String,
    pub command_log: Vec<GitCommandLogEntry>,
    pub result: Option<GitOperationResult>,
    pub result_sequence: u64,
    pub loading: bool,
    pub fetching: bool,
    pub message: String,
    pub nonce: u32,
}

impl Default for GitPageSnapshot {
    fn default() -> Self {
        Self {
            workspace: String::new(),
            repository: None,
            diff_viewport: None,
            diff_rows: Vec::new(),
            diff_loading: false,
            branch_files: Vec::new(),
            branch_files_reference: String::new(),
            branch_files_total: 0,
            branch_files_loading: false,
            branch_files_has_more: false,
            branch_files_error: String::new(),
            command_log: Vec::new(),
            result: None,
            result_sequence: 0,
            loading: true,
            fetching: false,
            message: String::new(),
            nonce: 0,
        }
    }
}

impl GitPageSnapshot {
    pub(crate) fn append_diff(&mut self, next: &GitDiffViewport) -> bool {
        let Some(current) = self.diff_viewport.as_mut() else {
            return false;
        };
        let loaded = current
            .first_line
            .saturating_add(u32::try_from(current.lines.len()).unwrap_or(u32::MAX));
        if next.first_line != loaded {
            return false;
        }
        current.total_lines = next.total_lines;
        current.lines.extend(next.lines.clone());
        current.markers.extend(next.markers.clone());
        current.markers.sort_by_key(|marker| marker.line);
        current.markers.dedup();
        current.error.clone_from(&next.error);
        true
    }
}

#[vmux_api::ui_state_patch(Default)]
pub struct GitUiStatePatch {
    pub context: Option<GitPageContext>,
    pub repository_picked: Option<GitRepositoryPicked>,
    pub workspace: Option<GitWorkspaceChanged>,
    pub projects: Option<Box<GitProjects>>,
    pub snapshot: Option<Box<GitPageSnapshot>>,
    pub directory: Option<Box<GitDirectoryState>>,
    pub controller: Option<Box<GitPageControllerState>>,
    pub selection_reveal: Option<GitSelectionReveal>,
}

#[vmux_api::ui_state(Default, patch = GitUiStatePatch, version = 5)]
pub struct GitUiState {
    pub context: Option<GitPageContext>,
    pub repository_picked: Option<GitRepositoryPicked>,
    pub workspace: Option<GitWorkspaceChanged>,
    pub projects: GitProjects,
    pub snapshot: GitPageSnapshot,
    pub directory: GitDirectoryState,
    pub controller: GitPageControllerState,
    pub selection_reveal: Option<GitSelectionReveal>,
}

impl vmux_api::UiStateProjection<GitUiStatePatch> for GitUiState {
    fn apply(&mut self, patch: GitUiStatePatch) {
        if let Some(context) = patch.context {
            self.context = Some(context);
        }
        if let Some(repository_picked) = patch.repository_picked {
            self.repository_picked = Some(repository_picked);
        }
        if let Some(workspace) = patch.workspace {
            self.workspace = Some(workspace);
        }
        if let Some(projects) = patch.projects {
            self.projects = *projects;
        }
        if let Some(snapshot) = patch.snapshot {
            self.snapshot = *snapshot;
        }
        if let Some(directory) = patch.directory {
            self.directory = *directory;
        }
        if let Some(controller) = patch.controller {
            self.controller = *controller;
        }
        if let Some(selection_reveal) = patch.selection_reveal {
            self.selection_reveal = Some(selection_reveal);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff_line(number: u32) -> crate::event::DiffLine {
        crate::event::DiffLine {
            kind: crate::event::DiffKind::Context,
            old_no: Some(number),
            new_no: Some(number),
            hunk: None,
            spans: Vec::new(),
        }
    }

    #[test]
    fn snapshot_round_trips_through_ui_state() {
        let event = <GitUiState as vmux_api::UiState>::from_updates(
            None,
            vec![
                GitPageContext {
                    working_directory: "/tmp".to_string(),
                    page_url: "git://tmp/repo".to_string(),
                }
                .into(),
                GitPageSnapshot {
                    workspace: "/tmp/repo".to_string(),
                    ..Default::default()
                }
                .into(),
                GitProjects {
                    rows: vec![ProjectRow {
                        path: "/tmp/repo".to_string(),
                        label: "repo".to_string(),
                        ..Default::default()
                    }],
                }
                .into(),
            ],
        );
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<GitUiState, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(
            decoded
                .context
                .as_ref()
                .map(|context| context.working_directory.as_str()),
            Some("/tmp")
        );
        assert_eq!(decoded.snapshot.workspace, "/tmp/repo");
        assert_eq!(decoded.projects.rows[0].path, "/tmp/repo");
    }

    #[test]
    fn diff_pages_append_without_discarding_loaded_lines() {
        let mut snapshot = GitPageSnapshot {
            diff_viewport: Some(GitDiffViewport {
                generation: 3,
                first_line: 0,
                total_lines: 4,
                lines: vec![diff_line(1), diff_line(2)],
                markers: Vec::new(),
                error: String::new(),
            }),
            ..Default::default()
        };
        let next = GitDiffViewport {
            generation: 3,
            first_line: 2,
            total_lines: 4,
            lines: vec![diff_line(3), diff_line(4)],
            markers: Vec::new(),
            error: String::new(),
        };

        assert!(snapshot.append_diff(&next));
        assert_eq!(snapshot.diff_viewport.unwrap().lines.len(), 4);
    }
}
