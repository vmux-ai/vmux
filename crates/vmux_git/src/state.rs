use crate::event::{
    GitBranchLogEvent, GitDiffViewportEvent, GitDirectoryEvent, GitRepositoryEvent, GitResultEvent,
};

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
    pub action: String,
    pub message: String,
    pub ok: bool,
}

#[vmux_api::contract]
pub struct GitPageSnapshot {
    pub workspace: String,
    pub repository: Option<GitRepositoryEvent>,
    pub directory: Option<GitDirectoryEvent>,
    pub directory_preview: Option<GitDirectoryEvent>,
    pub branch_log: Option<GitBranchLogEvent>,
    pub diff_viewport: Option<GitDiffViewportEvent>,
    pub command_log: Vec<GitCommandLogEntry>,
    pub result: Option<GitResultEvent>,
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
            directory: None,
            directory_preview: None,
            branch_log: None,
            diff_viewport: None,
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

#[vmux_api::ui_state_patch]
pub enum GitUiStatePatch {
    Context(GitPageContext),
    RepositoryPicked(GitRepositoryPicked),
    Workspace(GitWorkspaceChanged),
    Snapshot(Box<GitPageSnapshot>),
}

impl From<GitPageSnapshot> for GitUiStatePatch {
    fn from(snapshot: GitPageSnapshot) -> Self {
        Self::Snapshot(Box::new(snapshot))
    }
}

#[vmux_api::ui_state(Default, target = "git")]
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
        assert!(matches!(
            decoded.patches.as_slice(),
            [
                GitUiStatePatch::Context(GitPageContext { working_directory, .. }),
                GitUiStatePatch::Snapshot(snapshot),
            ] if working_directory == "/tmp" && snapshot.workspace == "/tmp/repo"
        ));
    }
}
