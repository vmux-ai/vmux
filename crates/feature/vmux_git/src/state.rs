use bevy_ecs::prelude::Component;
use vmux_api::space::ProjectRow;

use crate::event::{
    DiffKind, DiffLine, GitBranchEntry, GitBranchFileEntry, GitCatalog, GitCommitEntry, GitDiffRow,
    GitDiffViewport, GitOperationResult, GitRepositorySnapshot, GitStashEntry, GitTagEntry,
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
    pub fn route(self) -> &'static str {
        match self {
            Self::Files => "changes",
            Self::Branches => "branch-changes",
            Self::Commits => "commits",
            Self::Stash => "stashes",
        }
    }

    pub fn from_route(route: &str) -> Option<Self> {
        match route {
            "changes" => Some(Self::Files),
            "branch-changes" => Some(Self::Branches),
            "commits" => Some(Self::Commits),
            "stashes" => Some(Self::Stash),
            _ => None,
        }
    }

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

impl GitPageControllerState {
    pub fn for_repository(branch: String) -> Self {
        Self {
            selected_branch: branch,
            selected_branch_collection: GitBranchCollection::Local,
            ..Default::default()
        }
    }

    pub fn reset_for_repository(&mut self, branch: String) {
        *self = Self::for_repository(branch);
    }
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
    pub repository_request_id: u64,
    pub repository: Option<GitRepositorySnapshot>,
    pub diff_viewport: Option<GitDiffViewport>,
    pub diff_rows: Vec<GitDiffRow>,
    pub diff_loading: bool,
    pub branch_files: Vec<GitBranchFileEntry>,
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
            repository_request_id: 0,
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
    pub(crate) fn references_complete(&self) -> bool {
        for progress in [self.local_branches, self.remote_branches, self.tags] {
            if !progress.loaded || progress.has_more {
                return false;
            }
        }
        true
    }

    pub(crate) fn clear_diff(&mut self) {
        self.diff_viewport = None;
        self.diff_rows.clear();
        self.diff_loading = true;
        self.branch_files.clear();
        self.branch_diff_paths.clear();
        self.branch_files_reference.clear();
        self.branch_files_total = 0;
        self.branch_files_loading = true;
    }

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

    pub(crate) fn apply_catalog_patch(&mut self, patch: &GitCatalogPatch) {
        *self.catalog_mut(patch.catalog) = patch.progress;
        if let Some(message) = patch.message.as_ref() {
            self.message.clone_from(message);
        }
        let Some(entries) = patch.entries.as_ref() else {
            return;
        };
        let Some(repository) = self.repository.as_mut() else {
            return;
        };
        let offset = patch.offset as usize;
        match entries {
            GitCatalogEntries::LocalBranches(entries) => {
                Self::replace_page(&mut repository.branches, offset, entries);
            }
            GitCatalogEntries::RemoteBranches(entries) => {
                Self::replace_page(&mut repository.remote_branches, offset, entries);
            }
            GitCatalogEntries::Tags(entries) => {
                Self::replace_page(&mut repository.tags, offset, entries);
            }
            GitCatalogEntries::Commits(entries) => {
                Self::replace_page(&mut repository.commits, offset, entries);
            }
            GitCatalogEntries::Stashes(entries) => {
                Self::replace_page(&mut repository.stashes, offset, entries);
            }
        }
    }

    pub(crate) fn apply_branch_files_patch(&mut self, patch: &GitBranchFilesPatch) {
        if self.branch_files_reference != patch.reference {
            self.branch_files.clear();
            self.branch_diff_paths.clear();
            self.branch_files_base_ref.clear();
            self.branch_files_total = 0;
            self.branch_files_has_more = false;
            self.branch_files_reference.clone_from(&patch.reference);
        }
        if let Some(base_ref) = patch.base_ref.as_ref() {
            self.branch_files_base_ref.clone_from(base_ref);
        }
        if let Some(paths) = patch.diff_paths.as_ref() {
            self.branch_diff_paths.clone_from(paths);
        }
        if let Some(page) = patch.page.as_ref() {
            Self::replace_page(&mut self.branch_files, page.offset as usize, &page.files);
            self.branch_files_total = page.total;
            self.branch_files_has_more = page.has_more;
        }
        if let Some(diff_loading) = patch.diff_loading {
            self.diff_loading = diff_loading;
        }
        self.branch_files_loading = patch.loading;
        self.branch_files_error.clone_from(&patch.error);
    }

    fn replace_page<T: Clone>(target: &mut Vec<T>, offset: usize, page: &[T]) {
        if target.len() < offset {
            return;
        }
        target.truncate(offset);
        target.extend_from_slice(page);
    }

    pub(crate) fn loading(request: &crate::event::GitRepositoryRequest, nonce: u32) -> Self {
        Self {
            workspace: request.path.clone(),
            repository_request_id: request.request_id,
            repository: Some(GitRepositorySnapshot::loading(request)),
            diff_loading: true,
            branch_files_loading: true,
            nonce,
            loading: true,
            ..Default::default()
        }
    }

    pub(crate) fn begin_repository(&mut self, request: &crate::event::GitRepositoryRequest) {
        let nonce = self.nonce.wrapping_add(1).max(1);
        *self = Self::loading(request, nonce);
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
        self.diff_viewport
            .as_mut()
            .is_some_and(|current| current.append(next))
    }

    pub(crate) fn can_append_diff(&self, next: &GitDiffViewport) -> bool {
        self.diff_viewport
            .as_ref()
            .is_some_and(|current| current.can_append(next))
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
        let requested_from = previous_lines
            .saturating_sub(u32::try_from(Self::DIFF_CONTEXT_LINES + 1).unwrap_or(u32::MAX));
        let first = self
            .diff_rows
            .partition_point(|row| row.end() <= requested_from);
        let rows_from = self
            .diff_rows
            .get(first)
            .copied()
            .map(GitDiffRow::start)
            .unwrap_or(requested_from);
        if !self.append_diff(&next) {
            return None;
        }
        self.diff_loading = false;
        let rows = self
            .diff_viewport
            .as_ref()
            .map(|viewport| {
                Self::projected_diff_rows(
                    &viewport.lines,
                    revealed,
                    Self::DIFF_CONTEXT_LINES,
                    rows_from as usize,
                )
            })
            .unwrap_or_default();
        self.diff_rows.retain(|row| row.end() <= rows_from);
        self.diff_rows.extend(rows.iter().copied());
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
        self.diff_rows = Self::projected_diff_rows(&viewport.lines, revealed, context_lines, 0);
    }

    fn projected_diff_rows(
        lines: &[DiffLine],
        revealed: &[(usize, usize)],
        context_lines: usize,
        start: usize,
    ) -> Vec<GitDiffRow> {
        let start = start.min(lines.len());
        let mut visible = vec![false; lines.len().saturating_sub(start)];
        let scan_start = start.saturating_sub(context_lines);
        for (index, line) in lines.iter().enumerate().skip(scan_start) {
            if matches!(line.kind, DiffKind::Context) {
                continue;
            }
            let visible_start = index.saturating_sub(context_lines).max(start) - start;
            let visible_end = (index + context_lines + 1).min(lines.len()) - start;
            visible[visible_start..visible_end].fill(true);
        }
        for (revealed_start, revealed_end) in revealed {
            let revealed_start = (*revealed_start).clamp(start, lines.len()) - start;
            let revealed_end = (*revealed_end).clamp(start, lines.len()) - start;
            if revealed_start < revealed_end {
                visible[revealed_start..revealed_end].fill(true);
            }
        }
        let mut rows = Vec::new();
        let mut index = start;
        while index < lines.len() {
            if visible[index - start] {
                rows.push(GitDiffRow::Line(index as u32));
                index += 1;
                continue;
            }
            let hidden_start = index;
            while index < lines.len() && !visible[index - start] {
                index += 1;
            }
            let hidden = index - hidden_start;
            let chunk = crate::event::GitDiffRevealRequest::CHUNK as usize;
            let (reveal_start, reveal_end) = if hidden <= chunk {
                (hidden_start, index)
            } else if hidden_start == 0 {
                (index - chunk, index)
            } else {
                (hidden_start, hidden_start + chunk)
            };
            rows.push(GitDiffRow::Gap {
                start: hidden_start as u32,
                end: index as u32,
                reveal_start: reveal_start as u32,
                reveal_end: reveal_end as u32,
            });
        }
        rows
    }
}

#[vmux_api::contract]
pub struct GitDiffAppend {
    pub viewport: GitDiffViewport,
    pub rows_from: u32,
    pub rows: Vec<GitDiffRow>,
}

#[vmux_api::contract]
pub struct GitDiffRowsPatch {
    pub generation: u64,
    pub revision: u64,
    pub rows: Vec<GitDiffRow>,
}

#[vmux_api::contract]
pub struct GitRepositoryPatch {
    pub workspace: String,
    pub repository_request_id: u64,
    pub repository: Option<GitRepositorySnapshot>,
    pub local_branches: GitCatalogProgress,
    pub remote_branches: GitCatalogProgress,
    pub tags: GitCatalogProgress,
    pub commits: GitCatalogProgress,
    pub stashes: GitCatalogProgress,
    pub loading: bool,
    pub message: String,
}

impl GitRepositoryPatch {
    pub(crate) fn from_snapshot(snapshot: &GitPageSnapshot) -> Self {
        Self {
            workspace: snapshot.workspace.clone(),
            repository_request_id: snapshot.repository_request_id,
            repository: snapshot.repository.clone(),
            local_branches: snapshot.local_branches,
            remote_branches: snapshot.remote_branches,
            tags: snapshot.tags,
            commits: snapshot.commits,
            stashes: snapshot.stashes,
            loading: snapshot.loading,
            message: snapshot.message.clone(),
        }
    }
}

#[vmux_api::contract]
pub enum GitCatalogEntries {
    LocalBranches(Vec<GitBranchEntry>),
    RemoteBranches(Vec<GitBranchEntry>),
    Tags(Vec<GitTagEntry>),
    Commits(Vec<GitCommitEntry>),
    Stashes(Vec<GitStashEntry>),
}

#[vmux_api::contract]
pub struct GitCatalogPatch {
    pub catalog: GitCatalog,
    pub offset: u32,
    pub entries: Option<GitCatalogEntries>,
    pub progress: GitCatalogProgress,
    pub message: Option<String>,
}

#[vmux_api::contract]
pub struct GitBranchFilesPagePatch {
    pub offset: u32,
    pub files: Vec<GitBranchFileEntry>,
    pub total: u32,
    pub has_more: bool,
}

#[vmux_api::contract]
pub struct GitBranchFilesPatch {
    pub reference: String,
    pub base_ref: Option<String>,
    pub diff_paths: Option<Vec<String>>,
    pub page: Option<GitBranchFilesPagePatch>,
    pub diff_loading: Option<bool>,
    pub loading: bool,
    pub error: String,
}

#[vmux_api::ui_state_patch(Default)]
pub struct GitUiStatePatch {
    pub context: Option<GitPageContext>,
    pub repository_picked: Option<GitRepositoryPicked>,
    pub workspace: Option<GitWorkspaceChanged>,
    pub projects: Option<Box<GitProjects>>,
    pub snapshot: Option<Box<GitPageSnapshot>>,
    pub diff_append: Option<Box<GitDiffAppend>>,
    pub diff_rows: Option<Box<GitDiffRowsPatch>>,
    pub repository: Option<Box<GitRepositoryPatch>>,
    pub catalog: Option<Box<GitCatalogPatch>>,
    pub branch_files: Option<Box<GitBranchFilesPatch>>,
    pub directory: Option<Box<GitDirectoryState>>,
    pub controller: Option<Box<GitPageControllerState>>,
    pub selection_reveal: Option<GitSelectionReveal>,
}

#[vmux_api::ui_state(Default, patch = GitUiStatePatch, version = 16)]
pub struct GitUiState {
    pub context: Option<GitPageContext>,
    pub repository_picked: Option<GitRepositoryPicked>,
    pub workspace: Option<GitWorkspaceChanged>,
    pub projects: GitProjects,
    pub snapshot: GitPageSnapshot,
    pub directory: GitDirectoryState,
    pub controller: GitPageControllerState,
    pub selection_reveal: Option<GitSelectionReveal>,
    pub diff_updates: Vec<GitDiffAppend>,
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
            let repository_changed = self.snapshot.repository_request_id
                != snapshot.repository_request_id
                || self.snapshot.workspace != snapshot.workspace;
            self.snapshot = *snapshot;
            self.diff_updates.clear();
            if repository_changed {
                self.selection_reveal = None;
            }
        }
        if let Some(diff_append) = patch.diff_append {
            self.diff_updates.push(*diff_append);
        }
        if let Some(diff_rows) = patch.diff_rows
            && self
                .snapshot
                .diff_viewport
                .as_ref()
                .is_some_and(|viewport| {
                    viewport.generation == diff_rows.generation
                        && viewport.revision == diff_rows.revision
                })
        {
            self.snapshot.diff_rows = diff_rows.rows;
        }
        if let Some(repository) = patch.repository {
            self.snapshot.workspace = repository.workspace;
            self.snapshot.repository_request_id = repository.repository_request_id;
            self.snapshot.repository = repository.repository;
            self.snapshot.local_branches = repository.local_branches;
            self.snapshot.remote_branches = repository.remote_branches;
            self.snapshot.tags = repository.tags;
            self.snapshot.commits = repository.commits;
            self.snapshot.stashes = repository.stashes;
            self.snapshot.loading = repository.loading;
            self.snapshot.message = repository.message;
        }
        if let Some(catalog) = patch.catalog {
            self.snapshot.apply_catalog_patch(&catalog);
        }
        if let Some(branch_files) = patch.branch_files {
            self.snapshot.apply_branch_files_patch(&branch_files);
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

    fn retain(&mut self) {
        self.diff_updates.clear();
        self.selection_reveal = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_selection_replaces_stale_content_immediately() {
        let mut state = GitUiState::default();
        state.snapshot.repository = Some(repository_with_branches());
        state.snapshot.branch_files = vec![GitBranchFileEntry {
            path: "old.rs".into(),
            path_bytes: b"old.rs".to_vec(),
            previous_path: None,
            previous_path_bytes: None,
            change: crate::event::GitPathChange::Modified,
        }];
        state.snapshot.diff_viewport = Some(GitDiffViewport {
            generation: 1,
            revision: 1,
            first_line: 0,
            total_lines: 1,
            has_more: false,
            lines: vec![diff_line(1)],
            markers: Vec::new(),
            error: String::new(),
        });
        let mut request = crate::event::GitRepositoryRequest::project(
            "/tmp/next".into(),
            "next".into(),
            "feature".into(),
        );
        request.request_id = 7;

        state.snapshot.begin_repository(&request);
        state
            .controller
            .reset_for_repository(request.branch.clone());
        state.diff_updates.clear();

        let repository = state.snapshot.repository.as_ref().unwrap();
        assert_eq!(repository.repo_root, "/tmp/next");
        assert_eq!(repository.repo_name, "next");
        assert_eq!(repository.branch, "feature");
        assert_eq!(state.snapshot.repository_request_id, 7);
        assert!(state.snapshot.branch_files.is_empty());
        assert!(state.snapshot.diff_viewport.is_none());
        assert!(state.snapshot.diff_loading);
        assert_eq!(state.controller.selected_branch, "feature");
    }

    #[test]
    fn repository_projection_discards_stale_diff_updates() {
        let mut state = GitUiState::default();
        state.snapshot.workspace = "/tmp/old".into();
        state.snapshot.repository_request_id = 4;
        state.diff_updates.push(GitDiffAppend {
            viewport: GitDiffViewport {
                generation: 1,
                revision: 1,
                first_line: 0,
                total_lines: 1,
                has_more: false,
                lines: vec![diff_line(1)],
                markers: Vec::new(),
                error: String::new(),
            },
            rows_from: 0,
            rows: vec![GitDiffRow::Line(0)],
        });
        state.selection_reveal = Some(GitSelectionReveal {
            id: "old.rs".into(),
            revision: 2,
        });
        let snapshot = GitPageSnapshot {
            workspace: "/tmp/next".into(),
            repository_request_id: 5,
            ..Default::default()
        };

        vmux_api::UiStateProjection::apply(
            &mut state,
            GitUiStatePatch {
                snapshot: Some(Box::new(snapshot)),
                ..Default::default()
            },
        );

        assert!(state.diff_updates.is_empty());
        assert!(state.selection_reveal.is_none());
    }

    #[test]
    fn catalog_patch_appends_without_replacing_the_snapshot() {
        let mut state = GitUiState::default();
        state.snapshot.repository = Some(repository_with_branches());
        state.snapshot.diff_loading = true;
        let mut branch = state.snapshot.repository.as_ref().unwrap().branches[1].clone();
        branch.name = "release".to_string();
        vmux_api::UiStateProjection::apply(
            &mut state,
            GitUiStatePatch {
                catalog: Some(Box::new(GitCatalogPatch {
                    catalog: GitCatalog::LocalBranches,
                    offset: 2,
                    entries: Some(GitCatalogEntries::LocalBranches(vec![branch])),
                    progress: GitCatalogProgress {
                        initialized: true,
                        loaded: true,
                        ..Default::default()
                    },
                    message: None,
                })),
                ..Default::default()
            },
        );

        assert_eq!(
            state
                .snapshot
                .repository
                .as_ref()
                .unwrap()
                .branches
                .iter()
                .map(|branch| branch.name.as_str())
                .collect::<Vec<_>>(),
            ["main", "feature", "release"]
        );
        assert!(state.snapshot.diff_loading);
    }

    #[test]
    fn branch_file_patch_replaces_then_appends_pages() {
        let mut state = GitUiState::default();
        let file = |path: &str| GitBranchFileEntry {
            path: path.to_string(),
            path_bytes: path.as_bytes().to_vec(),
            previous_path: None,
            previous_path_bytes: None,
            change: crate::event::GitPathChange::Modified,
        };
        for patch in [
            GitBranchFilesPatch {
                reference: "feature".to_string(),
                base_ref: Some("main".to_string()),
                diff_paths: Some(vec!["a.rs".to_string(), "b.rs".to_string()]),
                page: Some(GitBranchFilesPagePatch {
                    offset: 0,
                    files: vec![file("a.rs")],
                    total: 2,
                    has_more: true,
                }),
                diff_loading: Some(true),
                loading: false,
                error: String::new(),
            },
            GitBranchFilesPatch {
                reference: "feature".to_string(),
                base_ref: None,
                diff_paths: None,
                page: Some(GitBranchFilesPagePatch {
                    offset: 1,
                    files: vec![file("b.rs")],
                    total: 2,
                    has_more: false,
                }),
                diff_loading: None,
                loading: false,
                error: String::new(),
            },
        ] {
            vmux_api::UiStateProjection::apply(
                &mut state,
                GitUiStatePatch {
                    branch_files: Some(Box::new(patch)),
                    ..Default::default()
                },
            );
        }

        assert_eq!(
            state
                .snapshot
                .branch_files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["a.rs", "b.rs"]
        );
        assert_eq!(state.snapshot.branch_files_base_ref, "main");
        assert!(!state.snapshot.branch_files_has_more);
        assert!(state.snapshot.diff_loading);
    }

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
                revision: 1,
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
            revision: 2,
            first_line: 2,
            total_lines: 4,
            has_more: false,
            lines: vec![diff_line(3), diff_line(4)],
            markers: Vec::new(),
            error: String::new(),
        };

        assert!(snapshot.append_diff(&next));
        let viewport = snapshot.diff_viewport.unwrap();
        assert_eq!(viewport.lines.len(), 4);
        assert_eq!(viewport.revision, 2);
    }

    #[test]
    fn incremental_diff_projection_matches_full_projection() {
        let mut lines = (1..=120).map(diff_line).collect::<Vec<_>>();
        lines[20].kind = DiffKind::Add;
        lines[68].kind = DiffKind::Remove;
        lines[94].kind = DiffKind::Add;
        let mut snapshot = GitPageSnapshot {
            diff_viewport: Some(GitDiffViewport {
                generation: 5,
                revision: 1,
                first_line: 0,
                total_lines: 120,
                has_more: true,
                lines: lines[..70].to_vec(),
                markers: Vec::new(),
                error: String::new(),
            }),
            ..Default::default()
        };
        snapshot.project_diff_rows(&[], GitPageSnapshot::DIFF_CONTEXT_LINES);

        let update = snapshot
            .append_diff_patch(
                GitDiffViewport {
                    generation: 5,
                    revision: 2,
                    first_line: 70,
                    total_lines: 120,
                    has_more: false,
                    lines: lines[70..].to_vec(),
                    markers: Vec::new(),
                    error: String::new(),
                },
                &[],
            )
            .unwrap();
        let mut expected = GitPageSnapshot {
            diff_viewport: snapshot.diff_viewport.clone(),
            ..Default::default()
        };
        expected.project_diff_rows(&[], GitPageSnapshot::DIFF_CONTEXT_LINES);

        assert_eq!(snapshot.diff_rows, expected.diff_rows);
        assert!(update.rows.len() < expected.diff_rows.len());
    }

    #[test]
    fn diff_append_update_stays_bounded() {
        let first = GitPageSnapshot {
            diff_loading: true,
            diff_viewport: Some(GitDiffViewport {
                generation: 3,
                revision: 1,
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
                        revision: 2,
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
        assert_eq!(decoded.diff_updates.len(), 1);
        assert_eq!(decoded.diff_updates[0].viewport.lines.len(), 2);
        let retained = vmux_api::UiState::retained(&event).unwrap();
        assert!(retained.diff_updates.is_empty());
    }

    #[test]
    fn diff_append_batch_preserves_every_update() {
        let first = GitPageSnapshot {
            diff_loading: true,
            diff_viewport: Some(GitDiffViewport {
                generation: 4,
                revision: 1,
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
                        revision: 2,
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
                        revision: 2,
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
        assert_eq!(decoded.diff_updates.len(), 2);
        let update = &decoded.diff_updates[1];
        assert_eq!(update.viewport.generation, 4);
        assert_eq!(update.rows_from, 1);
    }
}
