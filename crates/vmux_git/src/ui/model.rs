use dioxus::prelude::*;
use vmux_ui::i18n::translate;
use vmux_ui::list_nav::MenuDirection;

use crate::event::{FileStatus, GitFileEntry, GitRepositorySnapshot};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BranchPrompt {
    Create { base: String },
    Delete { branch: String },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum BranchCollection {
    #[default]
    Local,
    Remote,
    Tags,
}

impl BranchCollection {
    pub(super) fn references(self, repository: &GitRepositorySnapshot) -> Vec<String> {
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
        repository: &GitRepositorySnapshot,
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
