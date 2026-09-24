use vmux_ui::i18n::translate;

use crate::event::{FileStatus, GitFileEntry};

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
}
