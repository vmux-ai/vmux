use super::{CommandBarKey, CommandBarOpenEvent, PathCompleteResponse, StartProjectBranches};
use crate::chat::{PromptHistory, ResumableSessionEntry, ResumableSessions};
use crate::history::{HistoryEntry, HistorySuggestionsResponse};
use crate::prompt_media::{
    ChatAttachment, ChatAttachmentPreviews, ChatAttachments, ChatMediaEntries, ChatMediaEntry,
};
use crate::space::ProjectBranch;

#[vmux_api::contract(Copy, Default, Eq)]
pub struct CommandBarFocusInput;

#[vmux_api::ui_state_patch]
pub enum CommandBarUiStatePatch {
    Snapshot(Box<CommandBarOpenEvent>),
    Key(CommandBarKey),
    PathCompletion(PathCompleteResponse),
    HistorySuggestions(HistorySuggestionsResponse),
    PromptHistory(Box<PromptHistory>),
    ProjectBranches(Box<StartProjectBranches>),
    ResumableSessions(Box<ResumableSessions>),
    Attachments(Box<ChatAttachments>),
    AttachmentPreviews(Box<ChatAttachmentPreviews>),
    MediaEntries(Box<ChatMediaEntries>),
    FocusInput(CommandBarFocusInput),
}

#[vmux_api::ui_state(Default, targets = ["command-bar", "start", "layout"])]
pub struct CommandBarUiState {
    pub sequence: u64,
    pub patches: Vec<CommandBarUiStatePatch>,
}

#[vmux_api::ui_state(Default, targets = ["command-bar", "start", "layout"])]
pub struct CommandPaletteState {
    pub open_id: super::OpenId,
    pub completions: Vec<super::PathEntry>,
    pub completions_partial: bool,
    pub completions_total: u32,
    pub history: Vec<HistoryEntry>,
    pub prompt_history: Vec<String>,
    pub branch_project: String,
    pub branches: Vec<ProjectBranch>,
    pub sessions: Vec<ResumableSessionEntry>,
    pub sessions_total: u32,
    pub sessions_loading: bool,
    pub media_query: Option<String>,
    pub media_entries: Vec<ChatMediaEntry>,
    pub media_loading: bool,
    pub attachments: Vec<ChatAttachment>,
    pub attachment_sequence: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_preserve_patch_order() {
        let state = CommandBarUiState {
            sequence: 4,
            patches: vec![
                CommandBarOpenEvent::default().into(),
                CommandBarKey::Next.into(),
                PathCompleteResponse::default().into(),
            ],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&state).unwrap();
        let decoded = rkyv::from_bytes::<CommandBarUiState, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(decoded.sequence, 4);
        assert!(matches!(
            decoded.patches.as_slice(),
            [
                CommandBarUiStatePatch::Snapshot(_),
                CommandBarUiStatePatch::Key(CommandBarKey::Next),
                CommandBarUiStatePatch::PathCompletion(_),
            ]
        ));
    }
}
