use crate::event::{
    ChatAttachments, ChatBranchesState, ChatComposerEffect, ChatComposerMenuState,
    ChatListSelectionState, ChatMediaState, ChatPromptFocusEffect, ChatResumeState,
    ChatSelectorState, ChatSnapshot, ChatTranscriptState, ComposerContext, ModeState, ModelState,
};
use vmux_session::CatalogSnapshot;

#[vmux_api::ui_state_patch(Default)]
pub struct ChatUiStatePatch {
    pub sessions: Option<Box<CatalogSnapshot>>,
    pub snapshot: Option<Box<ChatSnapshot>>,
    pub composer: Option<ComposerContext>,
    pub mode: Option<ModeState>,
    pub model: Option<ModelState>,
    pub list_selection: Option<ChatListSelectionState>,
    pub composer_menu: Option<ChatComposerMenuState>,
    pub selector: Option<ChatSelectorState>,
    pub transcript: Option<Box<ChatTranscriptState>>,
    pub attachments: Option<Box<ChatAttachments>>,
    pub media: Option<Box<ChatMediaState>>,
    pub branches: Option<Box<ChatBranchesState>>,
    pub resume: Option<Box<ChatResumeState>>,
    pub composer_effect: Option<ChatComposerEffect>,
    pub prompt_focus: Option<ChatPromptFocusEffect>,
}

#[vmux_api::ui_state(Default)]
pub struct ChatUiState {
    pub sequence: u64,
    pub patches: Vec<ChatUiStatePatch>,
}
