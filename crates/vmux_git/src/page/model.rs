use dioxus::prelude::*;
use vmux_ui::i18n::translate;
use vmux_ui::list_nav::{MenuDirection, move_selection};
use vmux_ui::scroll::ScrollIntoView;

use crate::event::{FileStatus, GitFileEntry, GitOperation, GitRepositoryEvent, GitResultEvent};

use super::workspace::GitWorkspace;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BranchPrompt {
    Create { base: String },
    Delete { branch: String },
}

impl BranchPrompt {
    pub(super) fn submit(
        &self,
        repo_root: &str,
        draft: Signal<String>,
        mut pending_checkout: Signal<String>,
    ) -> bool {
        match self {
            Self::Create { base } => {
                let branch = draft().trim().to_string();
                if branch.is_empty() {
                    return false;
                }
                pending_checkout.set(branch.clone());
                GitWorkspace::operate(
                    repo_root,
                    GitOperation::CreateBranch {
                        branch,
                        start_point: base.clone(),
                    },
                );
            }
            Self::Delete { branch } => {
                GitWorkspace::operate(
                    repo_root,
                    GitOperation::DeleteBranch {
                        branch: branch.clone(),
                    },
                );
            }
        }
        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum BranchCollection {
    #[default]
    Local,
    Remote,
    Tags,
}

impl BranchCollection {
    pub(super) fn references(self, repository: &GitRepositoryEvent) -> Vec<String> {
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

    pub(super) fn selected_reference(
        self,
        repository: &GitRepositoryEvent,
        selected: &str,
    ) -> String {
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum GitPanel {
    #[default]
    Status,
    Files,
    Branches,
    Commits,
    Stash,
}

#[derive(Clone, Copy)]
pub(super) struct GitPanelSelection {
    pub(super) path: Signal<String>,
    pub(super) path_bytes: Signal<Vec<u8>>,
    pub(super) absolute_path: Signal<String>,
    pub(super) branch: Signal<String>,
    pub(super) branch_collection: Signal<BranchCollection>,
    pub(super) commit: Signal<String>,
    pub(super) stash: Signal<String>,
}

impl GitPanel {
    pub(super) fn menu_direction(event: &KeyboardData) -> Option<MenuDirection> {
        let modifiers = event.modifiers();
        if !modifiers.ctrl() && !modifiers.alt() && !modifiers.meta() && !modifiers.shift() {
            match event.key().to_string().as_str() {
                "j" => return Some(MenuDirection::Next),
                "k" => return Some(MenuDirection::Previous),
                _ => {}
            }
        }
        MenuDirection::from_key(event)
    }

    pub(super) fn from_key(key: &str) -> Option<Self> {
        match key {
            "0" => Some(Self::Status),
            "1" => Some(Self::Status),
            "2" => Some(Self::Files),
            "3" => Some(Self::Branches),
            "4" => Some(Self::Commits),
            "5" => Some(Self::Stash),
            _ => None,
        }
    }

    pub(super) fn next(self, reverse: bool) -> Self {
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

    pub(super) fn move_selection(
        self,
        repository: &GitRepositoryEvent,
        mut selection: GitPanelSelection,
        direction: MenuDirection,
    ) -> bool {
        match self {
            Self::Status => false,
            Self::Files => {
                let len = repository.files.len();
                if len == 0 {
                    return false;
                }
                let current = repository
                    .files
                    .iter()
                    .position(|entry| entry.path_bytes == (selection.path_bytes)())
                    .unwrap_or(match direction {
                        MenuDirection::Next => len - 1,
                        MenuDirection::Previous => 0,
                    });
                let index = move_selection(current, len, direction);
                let entry = &repository.files[index];
                selection.path.set(entry.path.clone());
                selection.path_bytes.set(entry.path_bytes.clone());
                selection.absolute_path.set(GitWorkspace::absolute_path(
                    &repository.repo_root,
                    &entry.path,
                ));
                let section = if entry.staged { "staged" } else { "unstaged" };
                ScrollIntoView::nearest(&format!("git-file-{section}-row-{index}"));
                true
            }
            Self::Branches => {
                let references = (selection.branch_collection)().references(repository);
                let len = references.len();
                if len == 0 {
                    return false;
                }
                let current = references
                    .iter()
                    .position(|reference| reference == &(selection.branch)())
                    .unwrap_or(match direction {
                        MenuDirection::Next => len - 1,
                        MenuDirection::Previous => 0,
                    });
                let index = move_selection(current, len, direction);
                selection.branch.set(references[index].clone());
                ScrollIntoView::nearest(&format!("git-branch-row-{index}"));
                true
            }
            Self::Commits => {
                let len = repository.commits.len();
                if len == 0 {
                    return false;
                }
                let current = repository
                    .commits
                    .iter()
                    .position(|entry| entry.sha == (selection.commit)())
                    .unwrap_or(match direction {
                        MenuDirection::Next => len - 1,
                        MenuDirection::Previous => 0,
                    });
                let index = move_selection(current, len, direction);
                selection.commit.set(repository.commits[index].sha.clone());
                ScrollIntoView::nearest(&format!("git-commit-row-{index}"));
                true
            }
            Self::Stash => {
                let len = repository.stashes.len();
                if len == 0 {
                    return false;
                }
                let current = repository
                    .stashes
                    .iter()
                    .position(|entry| entry.reference == (selection.stash)())
                    .unwrap_or(match direction {
                        MenuDirection::Next => len - 1,
                        MenuDirection::Previous => 0,
                    });
                let index = move_selection(current, len, direction);
                selection
                    .stash
                    .set(repository.stashes[index].reference.clone());
                ScrollIntoView::nearest(&format!("git-stash-row-{index}"));
                true
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct GitCommandLogEntry {
    pub(super) action: String,
    pub(super) message: String,
    pub(super) ok: bool,
}

impl GitCommandLogEntry {
    pub(super) fn from_result(result: &GitResultEvent) -> Self {
        Self {
            action: result.action.clone(),
            message: result.message.clone(),
            ok: result.ok,
        }
    }

    pub(super) fn error(message: &str) -> Self {
        Self {
            action: String::new(),
            message: message.to_string(),
            ok: false,
        }
    }

    pub(super) fn append(self, entries: &mut Vec<Self>) {
        if entries.len() >= 24 {
            entries.remove(0);
        }
        entries.push(self);
    }
}

pub(super) trait FileStatusView {
    fn label(self) -> String;
    fn code(self) -> &'static str;
    fn class(self) -> &'static str;
}

impl FileStatusView for FileStatus {
    fn label(self) -> String {
        match self {
            Self::Clean => translate("git-status-clean"),
            Self::Modified => translate("git-status-modified"),
            Self::Staged => translate("git-status-staged"),
            Self::StagedModified => translate("git-status-staged-modified"),
            Self::Untracked => translate("git-status-untracked"),
            Self::Deleted => translate("git-status-deleted"),
            Self::Conflicted => translate("git-status-conflict"),
        }
    }

    fn code(self) -> &'static str {
        match self {
            Self::Clean => "·",
            Self::Modified => "M",
            Self::Staged => "A",
            Self::StagedModified => "M",
            Self::Untracked => "U",
            Self::Deleted => "D",
            Self::Conflicted => "!",
        }
    }

    fn class(self) -> &'static str {
        match self {
            Self::Staged | Self::StagedModified => "text-ansi-2",
            Self::Conflicted | Self::Deleted => "text-ansi-1",
            Self::Modified | Self::Untracked => "text-ansi-3",
            Self::Clean => "text-muted-foreground",
        }
    }
}

impl GitFileEntry {
    pub(super) fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    pub(super) fn parent(&self) -> &str {
        self.path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("")
    }

    pub(super) fn can_discard(&self) -> bool {
        self.unstaged && self.status != FileStatus::Untracked
    }
}
