use std::path::Path;

use vmux_core::event::space::ProjectRequest;
use vmux_ui::hooks::send;

use crate::event::{
    GitBranchEntry, GitConfigEditRequest, GitDirectoryRequest, GitOperation, GitOperationRequest,
    GitRepositoryRequest, GitUpdateCheckRequest,
};

pub(super) struct GitWorkspace;

impl GitWorkspace {
    pub(super) fn request(path: &str) {
        if path.is_empty() {
            return;
        }
        let _ = send(&GitRepositoryRequest {
            path: path.to_string(),
        });
    }

    pub(super) fn absolute_path(root: &str, relative: &str) -> String {
        if root.is_empty() || relative.is_empty() {
            return String::new();
        }
        Path::new(root).join(relative).to_string_lossy().to_string()
    }

    pub(super) fn browse(path: &str, preview: bool) {
        let _ = send(&GitDirectoryRequest {
            path: path.to_string(),
            preview,
        });
    }

    pub(super) fn activate(path: &str) {
        let _ = send(&ProjectRequest::Activate {
            path: path.to_string(),
            branch: String::new(),
            checkout: String::new(),
            pane_id: None,
        });
    }

    pub(super) fn select_branch(repo_root: &str, branch: &GitBranchEntry) {
        let _ = send(&ProjectRequest::Activate {
            path: repo_root.to_string(),
            branch: branch.name.clone(),
            checkout: branch.checkout.clone(),
            pane_id: None,
        });
    }

    pub(super) fn select_branch_name(repo_root: &str, branch: &str) {
        let _ = send(&ProjectRequest::Activate {
            path: repo_root.to_string(),
            branch: branch.to_string(),
            checkout: String::new(),
            pane_id: None,
        });
    }

    pub(super) fn operate(repo_root: &str, operation: GitOperation) {
        let _ = send(&GitOperationRequest {
            repo_root: repo_root.to_string(),
            operation,
        });
    }

    pub(super) fn edit_config(repo_root: &str) {
        let _ = send(&GitConfigEditRequest {
            repo_root: repo_root.to_string(),
        });
    }

    pub(super) fn check_for_updates() {
        let _ = send(&GitUpdateCheckRequest);
    }
}
