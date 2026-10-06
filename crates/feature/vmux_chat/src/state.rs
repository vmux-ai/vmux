use crate::event::{
    ChatAttachments, ChatBranchesState, ChatComposerDraft, ChatComposerEffect, ChatComposerMedia,
    ChatComposerMenuState, ChatListSelectionState, ChatMediaState, ChatPromptFocusEffect,
    ChatResumeState, ChatSelectorState, ChatSnapshot, ChatTranscriptState, ComposerContext,
    ModeState, ModelState,
};
use vmux_api::mcp::McpServersUiState;
use vmux_session::CatalogSnapshot;

#[vmux_api::ui_state_patch(Default)]
pub struct ChatUiStatePatch {
    pub sessions: Option<Box<CatalogSnapshot>>,
    pub snapshot: Option<Box<ChatSnapshot>>,
    pub composer: Option<ComposerContext>,
    pub composer_draft: Option<ChatComposerDraft>,
    pub mode: Option<ModeState>,
    pub model: Option<ModelState>,
    pub list_selection: Option<ChatListSelectionState>,
    pub composer_menu: Option<ChatComposerMenuState>,
    pub selector: Option<ChatSelectorState>,
    pub transcript: Option<Box<ChatTranscriptState>>,
    pub attachments: Option<Box<ChatAttachments>>,
    pub media: Option<Box<ChatMediaState>>,
    pub composer_media: Option<Box<ChatComposerMedia>>,
    pub branches: Option<Box<ChatBranchesState>>,
    pub resume: Option<Box<ChatResumeState>>,
    pub composer_effect: Option<ChatComposerEffect>,
    pub prompt_focus: Option<ChatPromptFocusEffect>,
    pub mcp: Option<Box<McpServersUiState>>,
}

#[vmux_api::ui_state(Default, patch = ChatUiStatePatch, version = 2)]
pub struct ChatUiState {
    pub sessions: CatalogSnapshot,
    pub snapshot: ChatSnapshot,
    pub snapshot_ready: bool,
    pub composer: ComposerContext,
    pub composer_draft: String,
    pub composer_ready: bool,
    pub mode: ModeState,
    pub model: ModelState,
    pub model_ready: bool,
    pub list_selection: Option<ChatListSelectionState>,
    pub composer_menu: ChatComposerMenuState,
    pub selector: ChatSelectorState,
    pub transcript: ChatTranscriptState,
    pub transcript_revision: u64,
    pub attachments: ChatAttachments,
    pub media: ChatMediaState,
    pub composer_media: ChatComposerMedia,
    pub branches: ChatBranchesState,
    pub resume: ChatResumeState,
    pub composer_effect: ChatComposerEffect,
    pub prompt_focus: Option<ChatPromptFocusEffect>,
    pub mcp: McpServersUiState,
}

impl vmux_api::UiStateProjection<ChatUiStatePatch> for ChatUiState {
    fn apply(&mut self, patch: ChatUiStatePatch) {
        if let Some(sessions) = patch.sessions {
            self.sessions = *sessions;
        }
        if let Some(snapshot) = patch.snapshot {
            self.snapshot = *snapshot;
            self.snapshot_ready = true;
        }
        if let Some(composer) = patch.composer {
            self.composer = composer;
            self.composer_ready = true;
        }
        if let Some(composer_draft) = patch.composer_draft {
            self.composer_draft = composer_draft.text;
        }
        if let Some(mode) = patch.mode {
            self.mode = mode;
        }
        if let Some(model) = patch.model {
            self.model = model;
            self.model_ready = true;
        }
        if let Some(list_selection) = patch.list_selection {
            self.list_selection = Some(list_selection);
        }
        if let Some(composer_menu) = patch.composer_menu {
            self.composer_menu = composer_menu;
        }
        if let Some(selector) = patch.selector {
            self.selector = selector;
        }
        if let Some(transcript) = patch.transcript {
            self.transcript = *transcript;
            self.transcript_revision = self.transcript_revision.wrapping_add(1).max(1);
        }
        if let Some(attachments) = patch.attachments {
            self.attachments = *attachments;
        }
        if let Some(media) = patch.media {
            self.media = *media;
        }
        if let Some(composer_media) = patch.composer_media {
            self.composer_media = *composer_media;
        }
        if let Some(branches) = patch.branches {
            self.branches = *branches;
        }
        if let Some(resume) = patch.resume {
            self.resume = *resume;
        }
        if let Some(composer_effect) = patch.composer_effect {
            self.composer_effect = composer_effect;
        }
        if let Some(prompt_focus) = patch.prompt_focus {
            self.prompt_focus = Some(prompt_focus);
        }
        if let Some(mcp) = patch.mcp {
            self.mcp = *mcp;
        }
    }
}
