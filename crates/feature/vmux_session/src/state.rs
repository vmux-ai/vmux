use crate::event::{
    ChatAttachments, ChatBranchesState, ChatComposerDraft, ChatComposerEffect, ChatComposerMedia,
    ChatComposerMenuState, ChatListSelectionState, ChatMediaState, ChatPromptFocusEffect,
    ChatResumeState, ChatSelectorState, ChatSnapshot, ChatTranscriptState, ChatViewportState,
    ComposerContext, ModeState, ModelState, SessionDirectorySelection, SessionRepositories,
};
use vmux_api::address::{AddressCrumb, AddressIntent, AddressOption, PageAddress};
use vmux_api::mcp::McpServersUiState;
use vmux_session::{CatalogSnapshot, SessionSummary, StageSummary};

use crate::event::SessionRepository;

#[vmux_api::ui_state_patch(Default)]
pub struct SessionCatalogUiStatePatch {
    pub catalog: Option<Box<CatalogSnapshot>>,
    pub repositories: Option<Box<SessionRepositories>>,
    pub loading: Option<bool>,
}

#[vmux_api::ui_state(patch = SessionCatalogUiStatePatch, version = 3)]
pub struct SessionCatalogUiState {
    pub catalog: CatalogSnapshot,
    pub repositories: SessionRepositories,
    pub loading: bool,
}

impl Default for SessionCatalogUiState {
    fn default() -> Self {
        Self {
            catalog: CatalogSnapshot::default(),
            repositories: SessionRepositories::default(),
            loading: true,
        }
    }
}

impl vmux_api::UiStateProjection<SessionCatalogUiStatePatch> for SessionCatalogUiState {
    fn apply(&mut self, patch: SessionCatalogUiStatePatch) {
        if let Some(catalog) = patch.catalog {
            self.catalog = *catalog;
        }
        if let Some(repositories) = patch.repositories {
            self.repositories = *repositories;
        }
        if let Some(loading) = patch.loading {
            self.loading = loading;
        }
    }
}

impl SessionCatalogUiState {
    pub fn session(&self, id: &str) -> Option<&SessionSummary> {
        self.catalog
            .sessions
            .iter()
            .find(|session| session.id == id)
    }
}

#[vmux_api::ui_state(Default, Eq, version = 2)]
pub struct SessionDetailUiState {
    pub session: Option<SessionSummary>,
    pub repository: Option<SessionRepository>,
    pub stages: Vec<StageSummary>,
    pub navigation: Navigation,
}

impl SessionDetailUiState {
    pub fn address(&self, url: String) -> PageAddress {
        let mut address = PageAddress {
            url,
            current: AddressOption {
                label_key: "sessions-title".into(),
                icon: Some(vmux_api::BuiltinIcon::MessagesRound),
                intent: Some(AddressIntent::Navigate {
                    url: "vmux://sessions/".into(),
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let Some(session) = &self.session else {
            return address;
        };
        let entry = match &self.repository {
            Some(repository) => AddressOption {
                label: repository.project.clone(),
                identity: repository.project_identity.clone(),
                intent: Some(AddressIntent::Navigate {
                    url: repository.project_url.clone(),
                }),
                ..Default::default()
            },
            None => AddressOption {
                label_key: "layout-projects".into(),
                icon: Some(vmux_api::BuiltinIcon::Project),
                intent: Some(AddressIntent::Navigate {
                    url: "vmux://projects/".into(),
                }),
                ..Default::default()
            },
        };
        let mut options = Vec::new();
        for project in &self.navigation.projects {
            options.push(AddressOption {
                label: project.name.clone(),
                identity: project.identity.clone(),
                intent: Some(AddressIntent::Navigate {
                    url: project.url.clone(),
                }),
                ..Default::default()
            });
        }
        address.fixed.push(AddressCrumb {
            entry,
            options,
            ..Default::default()
        });
        if let Some(repository) = &self.repository
            && !repository.url.is_empty()
        {
            address.contextual.push(AddressOption {
                label_key: "git-title".into(),
                icon: Some(vmux_api::BuiltinIcon::GitBranch),
                intent: Some(AddressIntent::Navigate {
                    url: repository.url.clone(),
                }),
                ..Default::default()
            });
        }
        if !session.cwd.is_empty()
            && let Ok(url) = url::Url::from_directory_path(&session.cwd)
        {
            address.contextual.push(AddressOption {
                label_key: "editor-editor".into(),
                icon: Some(vmux_api::BuiltinIcon::Code),
                intent: Some(AddressIntent::Navigate { url: url.into() }),
                ..Default::default()
            });
        }
        let title = vmux_session::SessionSummaryProjection::new(
            session,
            &self.stages,
            vmux_ecs::UnixMillis::now().0,
        )
        .title;
        let project = self
            .repository
            .as_ref()
            .map(|repository| repository.project_url.as_str());
        let options = self
            .navigation
            .sessions
            .iter()
            .filter(|session| project.is_none_or(|project| session.project_url == project))
            .map(|session| AddressOption {
                label: session.title.clone(),
                identity: session.id.clone(),
                intent: Some(AddressIntent::Session {
                    id: session.id.clone(),
                }),
                ..Default::default()
            })
            .collect();
        address.fixed.push(AddressCrumb {
            entry: AddressOption {
                label: title,
                identity: session.id.clone(),
                intent: Some(AddressIntent::Session {
                    id: session.id.clone(),
                }),
                ..Default::default()
            },
            options,
            active: true,
            ..Default::default()
        });
        address
    }
}

#[vmux_api::contract(Default, Eq)]
pub struct Navigation {
    pub projects: Vec<NavigationProject>,
    pub sessions: Vec<NavigationSession>,
}

#[vmux_api::contract(Default, Eq)]
pub struct NavigationProject {
    #[serde(default)]
    pub identity: String,
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
pub struct SessionScopeUiState {
    pub sessions: Vec<SessionSummary>,
    pub stages: Vec<StageSummary>,
}

#[vmux_api::ui_state_patch(Default)]
pub struct ChatUiStatePatch {
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
    pub viewport: Option<ChatViewportState>,
    pub attachments: Option<Box<ChatAttachments>>,
    pub media: Option<Box<ChatMediaState>>,
    pub composer_media: Option<Box<ChatComposerMedia>>,
    pub branches: Option<Box<ChatBranchesState>>,
    pub resume: Option<Box<ChatResumeState>>,
    pub composer_effect: Option<ChatComposerEffect>,
    pub prompt_focus: Option<ChatPromptFocusEffect>,
    pub mcp: Option<Box<McpServersUiState>>,
}

#[vmux_api::ui_state(Default, patch = ChatUiStatePatch, version = 7)]
pub struct ChatUiState {
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
    pub viewport: ChatViewportState,
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
        if let Some(viewport) = patch.viewport {
            self.viewport = viewport;
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
