use bevy_ecs::prelude::Component;
use vmux_api::space::ProjectRow;

use crate::event::{
    DiffKind, GitCatalog, GitDiffRow, GitDiffViewport, GitOperationResult, GitRepositorySnapshot,
};

#[vmux_api::contract(Copy, Eq, Default)]
pub enum GitPanel {
    Files,
    #[default]
    Branches,
    Commits,
    Stash,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct RepositoryPath(pub String);

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct RepositoryName(pub String);

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckedOutBranch(pub String);

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchCheckout {
    pub branch: String,
    pub checkout: String,
    pub start_point: String,
    pub current: bool,
}

impl From<GitBranchCollection> for GitCatalog {
    fn from(collection: GitBranchCollection) -> Self {
        match collection {
            GitBranchCollection::Local => Self::LocalBranches,
            GitBranchCollection::Remote => Self::RemoteBranches,
            GitBranchCollection::Tags => Self::Tags,
        }
    }
}

impl GitBranchCollection {
    pub fn len(self, repository: &GitRepositorySnapshot) -> usize {
        match self {
            Self::Local => repository.branches.len(),
            Self::Remote => repository.remote_branches.len(),
            Self::Tags => repository.tags.len(),
        }
    }

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

    pub fn contains(self, repository: &GitRepositorySnapshot, reference: &str) -> bool {
        match self {
            Self::Local => repository
                .branches
                .iter()
                .any(|entry| entry.name == reference),
            Self::Remote => repository
                .remote_branches
                .iter()
                .any(|entry| entry.name == reference),
            Self::Tags => repository.tags.iter().any(|entry| entry.name == reference),
        }
    }

    pub fn selected_reference(self, repository: &GitRepositorySnapshot, selected: &str) -> String {
        if self.contains(repository, selected) {
            return selected.to_string();
        }
        if self == Self::Local
            && let Some(current) = repository.branches.iter().find(|entry| entry.current)
        {
            return current.name.clone();
        }
        if self == Self::Local && !repository.branch.is_empty() {
            return repository.branch.clone();
        }
        match self {
            Self::Local => repository.branches.first().map(|entry| entry.name.clone()),
            Self::Remote => repository
                .remote_branches
                .first()
                .map(|entry| entry.name.clone()),
            Self::Tags => repository.tags.first().map(|entry| entry.name.clone()),
        }
        .unwrap_or_default()
    }

    pub fn contains_any(repository: &GitRepositorySnapshot, reference: &str) -> bool {
        Self::containing(repository, reference).is_some()
    }

    pub fn containing(repository: &GitRepositorySnapshot, reference: &str) -> Option<Self> {
        if reference.is_empty() {
            return None;
        }
        [Self::Local, Self::Remote, Self::Tags]
            .into_iter()
            .find(|collection| collection.contains(repository, reference))
    }

    pub fn checkout(
        self,
        repository: &GitRepositorySnapshot,
        reference: &str,
    ) -> Option<BranchCheckout> {
        if reference.is_empty() || self == Self::Tags {
            return None;
        }
        let branch = match self {
            Self::Local => repository
                .branches
                .iter()
                .find(|branch| branch.name == reference),
            Self::Remote => repository
                .branches
                .iter()
                .find(|branch| branch.upstream == reference),
            Self::Tags => None,
        };
        if let Some(branch) = branch {
            return Some(BranchCheckout {
                branch: branch.name.clone(),
                checkout: branch.checkout.clone(),
                start_point: String::new(),
                current: branch.current,
            });
        }
        let branch = reference
            .split_once('/')
            .map(|(_, branch)| branch)
            .unwrap_or(reference)
            .to_string();
        Some(BranchCheckout {
            branch,
            checkout: String::new(),
            start_point: reference.to_string(),
            current: false,
        })
    }

    pub fn is_checked_out(self, repository: &GitRepositorySnapshot, reference: &str) -> bool {
        self.checkout(repository, reference)
            .is_some_and(|checkout| checkout.current)
    }
}

#[vmux_api::contract(Eq)]
pub enum GitBranchPrompt {
    Create { base: String },
    Delete { branch: String },
}

#[derive(Component)]
#[vmux_api::contract(Copy, Default, Eq)]
pub enum HeaderDropdown {
    #[default]
    Closed,
    Repository,
    Branch,
    View,
    Commit,
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
    pub selected_branch_collection: GitBranchCollection,
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
    pub branch_diff_paths: Vec<String>,
    pub branch_files_reference: String,
    pub branch_files_base_ref: String,
    pub branch_files_total: u32,
    pub branch_files_loading: bool,
    pub branch_files_has_more: bool,
    pub branch_files_error: String,
    pub local_branches: GitCatalogProgress,
    pub remote_branches: GitCatalogProgress,
    pub tags: GitCatalogProgress,
    pub commits: GitCatalogProgress,
    pub stashes: GitCatalogProgress,
    pub command_log: Vec<GitCommandLogEntry>,
    pub result: Option<GitOperationResult>,
    pub result_sequence: u64,
    pub loading: bool,
    pub fetching: bool,
    pub message: String,
    pub nonce: u32,
}

#[vmux_api::contract(Copy, Eq, Default)]
pub struct GitCatalogProgress {
    pub initialized: bool,
    pub loaded: bool,
    pub loading: bool,
    pub has_more: bool,
    pub failed: bool,
}

impl GitCatalogProgress {
    pub(crate) fn refresh(&mut self) {
        *self = Self {
            initialized: self.initialized,
            ..Default::default()
        };
    }
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
            branch_diff_paths: Vec::new(),
            branch_files_reference: String::new(),
            branch_files_base_ref: String::new(),
            branch_files_total: 0,
            branch_files_loading: false,
            branch_files_has_more: false,
            branch_files_error: String::new(),
            local_branches: GitCatalogProgress::default(),
            remote_branches: GitCatalogProgress::default(),
            tags: GitCatalogProgress::default(),
            commits: GitCatalogProgress::default(),
            stashes: GitCatalogProgress::default(),
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
    pub(crate) const DIFF_CONTEXT_LINES: usize = 3;

    pub(crate) fn catalog(&self, catalog: GitCatalog) -> GitCatalogProgress {
        match catalog {
            GitCatalog::LocalBranches => self.local_branches,
            GitCatalog::RemoteBranches => self.remote_branches,
            GitCatalog::Tags => self.tags,
            GitCatalog::Commits => self.commits,
            GitCatalog::Stashes => self.stashes,
        }
    }

    pub(crate) fn catalog_mut(&mut self, catalog: GitCatalog) -> &mut GitCatalogProgress {
        match catalog {
            GitCatalog::LocalBranches => &mut self.local_branches,
            GitCatalog::RemoteBranches => &mut self.remote_branches,
            GitCatalog::Tags => &mut self.tags,
            GitCatalog::Commits => &mut self.commits,
            GitCatalog::Stashes => &mut self.stashes,
        }
    }

    pub(crate) fn invalidate_catalogs(&mut self) {
        self.local_branches = GitCatalogProgress::default();
        self.remote_branches = GitCatalogProgress::default();
        self.tags = GitCatalogProgress::default();
        self.commits = GitCatalogProgress::default();
        self.stashes = GitCatalogProgress::default();
    }

    pub(crate) fn refresh_catalogs(&mut self) {
        self.local_branches.refresh();
        self.remote_branches.refresh();
        self.tags.refresh();
        self.commits.refresh();
        self.stashes.refresh();
    }

    pub(crate) fn has_failed_catalog(&self) -> bool {
        [
            self.local_branches,
            self.remote_branches,
            self.tags,
            self.commits,
            self.stashes,
        ]
        .into_iter()
        .any(|progress| progress.failed)
    }

    pub(crate) fn append_diff(&mut self, next: &GitDiffViewport) -> bool {
        if !self.can_append_diff(next) {
            return false;
        }
        let current = self.diff_viewport.as_mut().unwrap();
        current.total_lines = next.total_lines;
        current.has_more = next.has_more;
        current.lines.extend(next.lines.clone());
        current.markers.extend(next.markers.clone());
        current.markers.sort_by_key(|marker| marker.line);
        current.markers.dedup();
        current.error.clone_from(&next.error);
        true
    }

    pub(crate) fn can_append_diff(&self, next: &GitDiffViewport) -> bool {
        self.diff_viewport.as_ref().is_some_and(|current| {
            let loaded = current
                .first_line
                .saturating_add(u32::try_from(current.lines.len()).unwrap_or(u32::MAX));
            current.generation == next.generation && next.first_line == loaded
        })
    }

    pub(crate) fn append_diff_patch(
        &mut self,
        next: GitDiffViewport,
        revealed: &[(usize, usize)],
    ) -> Option<GitDiffAppend> {
        let previous_lines = self
            .diff_viewport
            .as_ref()
            .map(|viewport| u32::try_from(viewport.lines.len()).unwrap_or(u32::MAX))
            .unwrap_or_default();
        if !self.append_diff(&next) {
            return None;
        }
        self.diff_loading = false;
        self.project_diff_rows(revealed, Self::DIFF_CONTEXT_LINES);
        let requested_from = previous_lines
            .saturating_sub(u32::try_from(Self::DIFF_CONTEXT_LINES + 1).unwrap_or(u32::MAX));
        let first = self
            .diff_rows
            .partition_point(|row| row.end() <= requested_from);
        let rows = self.diff_rows[first..].to_vec();
        let rows_from = rows
            .first()
            .copied()
            .map(GitDiffRow::start)
            .unwrap_or(requested_from);
        Some(GitDiffAppend {
            viewport: next,
            rows_from,
            rows,
        })
    }

    pub(crate) fn project_diff_rows(&mut self, revealed: &[(usize, usize)], context_lines: usize) {
        let Some(viewport) = self.diff_viewport.as_ref() else {
            self.diff_rows.clear();
            return;
        };
        let lines = &viewport.lines;
        let mut visible = vec![false; lines.len()];
        for (index, line) in lines.iter().enumerate() {
            if matches!(line.kind, DiffKind::Context) {
                continue;
            }
            let start = index.saturating_sub(context_lines);
            let end = (index + context_lines + 1).min(lines.len());
            visible[start..end].fill(true);
        }
        for (start, end) in revealed {
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
            let chunk = crate::event::GitDiffRevealRequest::CHUNK as usize;
            let (reveal_start, reveal_end) = if hidden <= chunk {
                (start, index)
            } else if start == 0 {
                (index - chunk, index)
            } else {
                (start, start + chunk)
            };
            rows.push(GitDiffRow::Gap {
                start: start as u32,
                end: index as u32,
                reveal_start: reveal_start as u32,
                reveal_end: reveal_end as u32,
            });
        }
        self.diff_rows = rows;
    }
}

#[vmux_api::contract]
pub struct GitDiffAppend {
    pub viewport: GitDiffViewport,
    pub rows_from: u32,
    pub rows: Vec<GitDiffRow>,
}

#[vmux_api::ui_state_patch(Default)]
pub struct GitUiStatePatch {
    pub context: Option<GitPageContext>,
    pub repository_picked: Option<GitRepositoryPicked>,
    pub workspace: Option<GitWorkspaceChanged>,
    pub projects: Option<Box<GitProjects>>,
    pub snapshot: Option<Box<GitPageSnapshot>>,
    pub diff_append: Option<Box<GitDiffAppend>>,
    pub directory: Option<Box<GitDirectoryState>>,
    pub controller: Option<Box<GitPageControllerState>>,
    pub header_dropdown: Option<HeaderDropdown>,
    pub selection_reveal: Option<GitSelectionReveal>,
}

#[vmux_api::ui_state(Default, patch = GitUiStatePatch, version = 10)]
pub struct GitUiState {
    pub context: Option<GitPageContext>,
    pub repository_picked: Option<GitRepositoryPicked>,
    pub workspace: Option<GitWorkspaceChanged>,
    pub projects: GitProjects,
    pub snapshot: GitPageSnapshot,
    pub directory: GitDirectoryState,
    pub controller: GitPageControllerState,
    pub header_dropdown: HeaderDropdown,
    pub selection_reveal: Option<GitSelectionReveal>,
    pub diff_update: Option<GitDiffAppend>,
    pub diff_update_revision: u64,
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
        if let Some(diff_append) = patch.diff_append {
            self.diff_update = Some(*diff_append);
            self.diff_update_revision = self.diff_update_revision.wrapping_add(1).max(1);
        }
        if let Some(directory) = patch.directory {
            self.directory = *directory;
        }
        if let Some(controller) = patch.controller {
            self.controller = *controller;
        }
        if let Some(header_dropdown) = patch.header_dropdown {
            self.header_dropdown = header_dropdown;
        }
        if let Some(selection_reveal) = patch.selection_reveal {
            self.selection_reveal = Some(selection_reveal);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository_with_branches() -> GitRepositorySnapshot {
        GitRepositorySnapshot {
            path: "/tmp/repo".to_string(),
            repo_root: "/tmp/repo".to_string(),
            repo_name: "repo".to_string(),
            is_worktree: false,
            branch: "main".to_string(),
            base_ref: "main".to_string(),
            upstream: "origin/main".to_string(),
            ahead: 0,
            behind: 0,
            files: Vec::new(),
            commits: Vec::new(),
            branches: vec![
                crate::event::GitBranchEntry {
                    name: "main".to_string(),
                    current: true,
                    upstream: "origin/main".to_string(),
                    checkout: "/tmp/repo".to_string(),
                    short_sha: String::new(),
                    ahead: 0,
                    behind: 0,
                    changed_files: 0,
                    insertions: 0,
                    deletions: 0,
                },
                crate::event::GitBranchEntry {
                    name: "feature".to_string(),
                    current: false,
                    upstream: "origin/feature".to_string(),
                    checkout: String::new(),
                    short_sha: String::new(),
                    ahead: 0,
                    behind: 0,
                    changed_files: 0,
                    insertions: 0,
                    deletions: 0,
                },
            ],
            remote_branches: Vec::new(),
            tags: Vec::new(),
            stashes: Vec::new(),
        }
    }

    #[test]
    fn remote_checkout_reuses_its_tracking_branch() {
        let repository = repository_with_branches();

        let checkout = GitBranchCollection::Remote
            .checkout(&repository, "origin/feature")
            .unwrap();

        assert_eq!(checkout.branch, "feature");
        assert!(checkout.start_point.is_empty());
        assert!(!checkout.current);
        assert!(GitBranchCollection::Remote.is_checked_out(&repository, "origin/main"));
    }

    #[test]
    fn remote_checkout_creates_a_tracking_branch_when_missing() {
        let repository = repository_with_branches();

        let checkout = GitBranchCollection::Remote
            .checkout(&repository, "upstream/topic/nested")
            .unwrap();

        assert_eq!(checkout.branch, "topic/nested");
        assert_eq!(checkout.start_point, "upstream/topic/nested");
        assert!(!checkout.current);
    }

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
                has_more: true,
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
            has_more: false,
            lines: vec![diff_line(3), diff_line(4)],
            markers: Vec::new(),
            error: String::new(),
        };

        assert!(snapshot.append_diff(&next));
        assert_eq!(snapshot.diff_viewport.unwrap().lines.len(), 4);
    }

    #[test]
    fn diff_append_update_stays_bounded() {
        let first = GitPageSnapshot {
            diff_loading: true,
            diff_viewport: Some(GitDiffViewport {
                generation: 3,
                first_line: 0,
                total_lines: 4,
                has_more: true,
                lines: vec![diff_line(1), diff_line(2)],
                markers: Vec::new(),
                error: String::new(),
            }),
            ..Default::default()
        };
        let event = <GitUiState as vmux_api::UiState>::from_updates(
            None,
            vec![
                first.into(),
                GitDiffAppend {
                    viewport: GitDiffViewport {
                        generation: 3,
                        first_line: 2,
                        total_lines: 4,
                        has_more: false,
                        lines: vec![diff_line(3), diff_line(4)],
                        markers: Vec::new(),
                        error: String::new(),
                    },
                    rows_from: 0,
                    rows: vec![GitDiffRow::Line(0), GitDiffRow::Line(3)],
                }
                .into(),
            ],
        );
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<GitUiState, rkyv::rancor::Error>(&bytes).unwrap();

        assert!(decoded.snapshot.diff_loading);
        assert_eq!(decoded.snapshot.diff_viewport.unwrap().lines.len(), 2);
        assert_eq!(decoded.diff_update_revision, 1);
        assert_eq!(decoded.diff_update.unwrap().viewport.lines.len(), 2);
    }

    #[test]
    fn newer_diff_append_replaces_the_previous_update() {
        let first = GitPageSnapshot {
            diff_loading: true,
            diff_viewport: Some(GitDiffViewport {
                generation: 4,
                first_line: 0,
                total_lines: 4,
                has_more: true,
                lines: vec![diff_line(1), diff_line(2)],
                markers: Vec::new(),
                error: String::new(),
            }),
            diff_rows: vec![GitDiffRow::Line(0)],
            ..Default::default()
        };
        let event = <GitUiState as vmux_api::UiState>::from_updates(
            None,
            vec![
                first.into(),
                GitDiffAppend {
                    viewport: GitDiffViewport {
                        generation: 3,
                        first_line: 2,
                        total_lines: 4,
                        has_more: false,
                        lines: vec![diff_line(3), diff_line(4)],
                        markers: Vec::new(),
                        error: String::new(),
                    },
                    rows_from: 0,
                    rows: vec![GitDiffRow::Line(3)],
                }
                .into(),
                GitDiffAppend {
                    viewport: GitDiffViewport {
                        generation: 4,
                        first_line: 2,
                        total_lines: 5,
                        has_more: true,
                        lines: vec![diff_line(5)],
                        markers: Vec::new(),
                        error: String::new(),
                    },
                    rows_from: 1,
                    rows: vec![GitDiffRow::Line(2)],
                }
                .into(),
            ],
        );
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<GitUiState, rkyv::rancor::Error>(&bytes).unwrap();

        assert!(decoded.snapshot.diff_loading);
        assert_eq!(decoded.snapshot.diff_viewport.unwrap().lines.len(), 2);
        assert_eq!(decoded.snapshot.diff_rows, [GitDiffRow::Line(0)]);
        assert_eq!(decoded.diff_update_revision, 2);
        let update = decoded.diff_update.unwrap();
        assert_eq!(update.viewport.generation, 4);
        assert_eq!(update.rows_from, 1);
    }
}
