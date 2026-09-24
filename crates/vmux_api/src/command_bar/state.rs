use super::{CommandBarKey, CommandBarOpenEvent, PathCompleteResponse, StartProjectBranches};
use crate::chat::{PromptHistory, ResumableSessions};
use crate::history::HistorySuggestionsResponse;
use crate::prompt_media::{ChatAttachmentPreviews, ChatAttachments, ChatMediaEntries};

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
