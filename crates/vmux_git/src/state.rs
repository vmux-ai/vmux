use crate::event::{GitBranchLog, GitDiffViewport, GitOperationResult, GitRepositorySnapshot};

#[vmux_api::contract(Copy, Eq, Default)]
pub enum GitPanel {
    #[default]
    Status,
    Files,
    Branches,
    Commits,
    Stash,
}

impl GitPanel {
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "0" | "1" => Some(Self::Status),
            "2" => Some(Self::Files),
            "3" => Some(Self::Branches),
            "4" => Some(Self::Commits),
            "5" => Some(Self::Stash),
            _ => None,
        }
    }

    pub fn next(self, reverse: bool) -> Self {
        match (self, reverse) {
            (Self::Status, false) => Self::Files,
            (Self::Files, false) => Self::Branches,
            (Self::Branches, false) => Self::Commits,
            (Self::Commits, false) => Self::Stash,
            (Self::Stash, false) => Self::Status,
            (Self::Status, true) => Self::Stash,
            (Self::Files, true) => Self::Status,
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

#[vmux_api::contract(Default, Eq)]
pub struct GitPageControllerState {
    pub selected_path: String,
    pub selected_path_bytes: Vec<u8>,
    pub selected_abs_path: String,
    pub selected_commit: String,
    pub selected_branch: String,
    pub branch_collection: GitBranchCollection,
    pub selected_stash: String,
    pub confirm_discard: Vec<u8>,
    pub focused_panel: GitPanel,
    pub operations: GitOperationEligibility,
}

#[vmux_api::contract(Eq)]
pub struct GitBranchPromptRequested {
    pub prompt: GitBranchPrompt,
}

#[vmux_api::contract(Eq)]
pub struct GitSelectionReveal {
    pub id: String,
}

#[vmux_api::contract(Copy, Eq, Default)]
pub struct GitShortcutHelpToggle;

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

#[vmux_api::contract]
pub struct GitCommandLogEntry {
    pub operation: String,
    pub message: String,
    pub ok: bool,
}

#[vmux_api::contract(Eq)]
pub struct GitDirectoryState {
    pub path: String,
    pub parent_entries: Vec<vmux_core::event::FileDirEntry>,
    pub entries: Vec<vmux_core::event::FileDirEntry>,
    pub children: Option<Vec<vmux_core::event::FileDirEntry>>,
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
    pub branch_log: Option<GitBranchLog>,
    pub diff_viewport: Option<GitDiffViewport>,
    pub diff_loading: bool,
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
            branch_log: None,
            diff_viewport: None,
            diff_loading: false,
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

#[vmux_api::ui_state_patch(Default)]
pub struct GitUiStatePatch {
    pub context: Option<GitPageContext>,
    pub repository_picked: Option<GitRepositoryPicked>,
    pub workspace: Option<GitWorkspaceChanged>,
    pub snapshot: Option<Box<GitPageSnapshot>>,
    pub directory: Option<Box<GitDirectoryState>>,
    pub controller: Option<Box<GitPageControllerState>>,
    pub branch_prompt: Option<GitBranchPromptRequested>,
    pub selection_reveal: Option<GitSelectionReveal>,
    pub shortcut_help_toggle: Option<GitShortcutHelpToggle>,
}

#[vmux_api::ui_state(Default, url = "git://")]
pub struct GitUiState {
    pub sequence: u64,
    pub patches: Vec<GitUiStatePatch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trips_through_ui_state() {
        let event = GitUiState {
            sequence: 2,
            patches: vec![
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
            ],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<GitUiState, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(decoded.sequence, 2);
        assert_eq!(
            decoded.patches[0]
                .context
                .as_ref()
                .map(|context| context.working_directory.as_str()),
            Some("/tmp")
        );
        assert_eq!(
            decoded.patches[1]
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.workspace.as_str()),
            Some("/tmp/repo")
        );
    }
}
