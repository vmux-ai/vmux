use super::{CommandBarOpenEvent, PathCompleteResponse, StartProjectBranches};
use crate::chat::{PromptHistory, ResumableSessions};
use crate::history::{HistoryEntry, HistorySuggestionsResponse};
use crate::prompt_media::{
    ChatAttachment, ChatAttachments, ChatMediaEntries, ChatMediaEntry, PromptComposerAttachment,
    PromptMediaOption,
};
use crate::space::ProjectBranch;

#[vmux_api::contract(Copy, Default, Eq)]
pub struct CommandBarFocusEffect {
    pub revision: u64,
}

#[vmux_api::ui_state_patch(Default)]
pub struct CommandBarUiStatePatch {
    pub snapshot: Option<Box<CommandBarOpenEvent>>,
    pub path_completion: Option<PathCompleteResponse>,
    pub history_suggestions: Option<HistorySuggestionsResponse>,
    pub prompt_history: Option<Box<PromptHistory>>,
    pub project_branches: Option<Box<StartProjectBranches>>,
    pub resumable_sessions: Option<Box<ResumableSessions>>,
    pub attachments: Option<Box<ChatAttachments>>,
    pub media_entries: Option<Box<ChatMediaEntries>>,
    pub focus: Option<CommandBarFocusEffect>,
    pub palette: Option<Box<CommandPaletteUiState>>,
}

#[vmux_api::ui_state(Default, patch = CommandBarUiStatePatch)]
pub struct CommandBarUiState {
    pub snapshot: CommandBarOpenEvent,
    pub path_completion: Option<PathCompleteResponse>,
    pub history_suggestions: Option<HistorySuggestionsResponse>,
    pub prompt_history: Option<PromptHistory>,
    pub project_branches: Option<StartProjectBranches>,
    pub resumable_sessions: Option<ResumableSessions>,
    pub attachments: ChatAttachments,
    pub media_entries: ChatMediaEntries,
    pub focus: CommandBarFocusEffect,
    pub palette: CommandPaletteUiState,
}

#[vmux_api::ui_state(Default)]
pub struct CommandPaletteUiState {
    pub open_id: super::OpenId,
    pub projection: super::CommandPaletteProjection,
    pub completions: Vec<super::PathEntry>,
    pub completions_partial: bool,
    pub completions_total: u32,
    pub history: Vec<HistoryEntry>,
    pub prompt_history: Vec<String>,
    pub branch_project: String,
    pub branches: Vec<ProjectBranch>,
    pub media_query: Option<String>,
    pub media_entries: Vec<ChatMediaEntry>,
    pub media_options: Vec<PromptMediaOption>,
    pub media_loading: bool,
    pub media_selected: u32,
    pub attachments: Vec<ChatAttachment>,
    pub composer_attachments: Vec<PromptComposerAttachment>,
    pub attachment_sequence: u64,
}

impl crate::UiStateProjection<CommandBarUiStatePatch> for CommandBarUiState {
    fn apply(&mut self, patch: CommandBarUiStatePatch) {
        if let Some(snapshot) = patch.snapshot {
            self.snapshot = *snapshot;
        }
        if let Some(path_completion) = patch.path_completion {
            self.path_completion = Some(path_completion);
        }
        if let Some(history_suggestions) = patch.history_suggestions {
            self.history_suggestions = Some(history_suggestions);
        }
        if let Some(prompt_history) = patch.prompt_history {
            self.prompt_history = Some(*prompt_history);
        }
        if let Some(project_branches) = patch.project_branches {
            self.project_branches = Some(*project_branches);
        }
        if let Some(resumable_sessions) = patch.resumable_sessions {
            self.resumable_sessions = Some(*resumable_sessions);
        }
        if let Some(attachments) = patch.attachments {
            self.attachments = *attachments;
        }
        if let Some(media_entries) = patch.media_entries {
            self.media_entries = *media_entries;
        }
        if let Some(focus) = patch.focus {
            self.focus = focus;
        }
        if let Some(palette) = patch.palette {
            self.palette = *palette;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_bar::{CommandBarResultItem, CommandPaletteProjection, OpenId, PaletteMode};

    #[test]
    fn patches_build_a_retained_tree() {
        let state = <CommandBarUiState as crate::UiState>::from_updates(
            None,
            vec![
                CommandBarOpenEvent::default().into(),
                PathCompleteResponse::default().into(),
            ],
        );
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&state).unwrap();
        let decoded = rkyv::from_bytes::<CommandBarUiState, rkyv::rancor::Error>(&bytes).unwrap();

        assert!(decoded.path_completion.is_some());
    }

    #[test]
    fn palette_projection_round_trips() {
        let state = CommandPaletteUiState {
            open_id: OpenId(8),
            projection: CommandPaletteProjection {
                rows: vec![CommandBarResultItem {
                    key: "settings".to_string(),
                    title: "Settings".to_string(),
                    url: "vmux://settings".to_string(),
                    ..Default::default()
                }],
                ghost: "/settings".to_string(),
                mode: PaletteMode::Url,
                ..Default::default()
            },
            ..Default::default()
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&state).unwrap();
        let decoded =
            rkyv::from_bytes::<CommandPaletteUiState, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(decoded.open_id, OpenId(8));
        assert_eq!(decoded.projection.mode, PaletteMode::Url);
        assert_eq!(decoded.projection.rows[0].key, "settings");
        assert_eq!(decoded.projection.rows[0].url, "vmux://settings");
    }
}
