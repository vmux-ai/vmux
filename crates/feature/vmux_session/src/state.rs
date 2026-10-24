use crate::event::{
    ChatAttachments, ChatBranchesState, ChatComposerDraft, ChatComposerEffect, ChatComposerMedia,
    ChatComposerMenuState, ChatListSelectionState, ChatMediaState, ChatPromptFocusEffect,
    ChatResumeState, ChatSelectorState, ChatSnapshot, ChatTranscriptState, ComposerContext,
    ModeState, ModelState, SessionDirectorySelection, SessionRepositories,
};
use vmux_api::mcp::UiMcpServers;
use vmux_session::{CatalogSnapshot, SessionSummary, StageSummary};

use crate::event::SessionRepository;

#[vmux_api::ui_state_patch(Default)]
pub struct UiSessionCatalogPatch {
    pub catalog: Option<Box<CatalogSnapshot>>,
    pub repositories: Option<Box<SessionRepositories>>,
}

#[vmux_api::ui_state(Default, patch = UiSessionCatalogPatch, version = 1)]
pub struct UiSessionCatalog {
    pub catalog: CatalogSnapshot,
    pub repositories: SessionRepositories,
}

impl vmux_api::UiStateProjection<UiSessionCatalogPatch> for UiSessionCatalog {
    fn apply(&mut self, patch: UiSessionCatalogPatch) {
        if let Some(catalog) = patch.catalog {
            self.catalog = *catalog;
        }
        if let Some(repositories) = patch.repositories {
            self.repositories = *repositories;
        }
    }
}

impl UiSessionCatalog {
    pub fn session(&self, id: &str) -> Option<&SessionSummary> {
        self.catalog
            .sessions
            .iter()
            .find(|session| session.id == id)
    }
}

#[vmux_api::ui_state(Default, Eq)]
pub struct UiSessionDetail {
    pub session: Option<SessionSummary>,
    pub repository: Option<SessionRepository>,
    pub stages: Vec<StageSummary>,
    pub navigation: Navigation,
}

#[vmux_api::contract(Default, Eq)]
pub struct Navigation {
    pub projects: Vec<NavigationProject>,
    pub sessions: Vec<NavigationSession>,
}

#[vmux_api::contract(Default, Eq)]
pub struct NavigationProject {
    pub name: String,
    pub url: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct NavigationSession {
    pub id: String,
    pub title: String,
    pub project_url: String,
}

#[vmux_api::ui_state(Default, Eq)]
pub struct UiSessionScope {
    pub sessions: Vec<SessionSummary>,
    pub stages: Vec<StageSummary>,
}

#[vmux_api::ui_state_patch(Default)]
pub struct UiChatPatch {
    pub session_directory: Option<SessionDirectorySelection>,
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
    pub mcp: Option<Box<UiMcpServers>>,
}

#[vmux_api::ui_state(Default, patch = UiChatPatch, version = 4)]
pub struct UiChat {
    pub session_directory: Option<SessionDirectorySelection>,
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
    pub mcp: UiMcpServers,
}

impl vmux_api::UiStateProjection<UiChatPatch> for UiChat {
    fn apply(&mut self, patch: UiChatPatch) {
        if let Some(session_directory) = patch.session_directory {
            self.session_directory = Some(session_directory);
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
