#[cfg(ui)]
pub(crate) use catalog::RegisteredStory;
pub use catalog::Story;
#[cfg(host)]
pub use host::GalleryPlugin;

mod catalog;
#[cfg(host)]
mod host;
#[cfg(ui)]
pub mod ui;

#[vmux_api::ui_state(Default)]
pub struct GalleryUiState {
    pub address: vmux_api::address::PageAddress,
    pub examples: Examples,
    pub story: Story,
    pub disabled: bool,
    pub text: String,
    pub checked: bool,
    pub choice: String,
    pub dialog_open: bool,
    pub progress: f64,
    pub interaction_count: u64,
    pub last_event: String,
    pub revision: u64,
    pub menu: String,
    pub cursor: usize,
    pub agent: String,
    pub model: String,
    pub effort: String,
    pub permission: String,
    pub project: String,
    pub branch: String,
    pub running: bool,
    pub has_attachment: bool,
    pub status: String,
    pub submitted: String,
    pub presets: Vec<StoryPreset>,
    pub button_variant: String,
    pub button_size: String,
}

#[vmux_api::contract(Default)]
pub struct Examples {
    pub files: Vec<vmux_ecs::event::FileDirEntry>,
    pub agents: Vec<vmux_api::command_bar::CommandPaletteAgent>,
    pub models: Vec<vmux_api::session::ModelOptionEntry>,
    pub modes: Vec<vmux_api::protocol::AcpModeOption>,
    pub projects: Vec<vmux_api::space::ProjectRow>,
    pub branches: Vec<vmux_api::space::ProjectBranch>,
    pub media: Vec<vmux_api::prompt_media::PromptMediaOption>,
    pub attachments: Vec<vmux_api::prompt_media::PromptComposerAttachment>,
    pub mcp: vmux_api::mcp::McpServersUiState,
}

#[vmux_api::ui_event]
pub struct GalleryDisabledRequest {
    pub disabled: bool,
}

#[vmux_api::ui_event]
pub struct GalleryTextRequest {
    pub revision: u64,
    pub text: String,
}

#[vmux_api::ui_event]
pub struct GalleryCheckedRequest {
    pub revision: u64,
    pub checked: bool,
}

#[vmux_api::ui_event]
pub struct GalleryChoiceRequest {
    pub revision: u64,
    pub choice: String,
}

#[vmux_api::ui_event]
pub struct GalleryDialogRequest {
    pub revision: u64,
    pub open: bool,
}

#[vmux_api::ui_event]
pub struct GalleryProgressRequest {
    pub revision: u64,
    pub value: f64,
}

#[vmux_api::ui_event]
pub struct GalleryClickRequest {
    pub revision: u64,
    pub label: String,
}

#[vmux_api::ui_event]
pub struct GalleryResetRequest;

#[vmux_api::ui_event]
pub struct GalleryMenuRequest {
    pub revision: u64,
    pub menu: String,
}

#[vmux_api::ui_event]
pub struct GalleryCursorRequest {
    pub revision: u64,
    pub cursor: usize,
}

#[vmux_api::ui_event]
pub struct GalleryAgentRequest {
    pub revision: u64,
    pub value: String,
}

#[vmux_api::ui_event]
pub struct GalleryModelRequest {
    pub revision: u64,
    pub value: String,
}

#[vmux_api::ui_event]
pub struct GalleryEffortRequest {
    pub revision: u64,
    pub value: String,
}

#[vmux_api::ui_event]
pub struct GalleryPermissionRequest {
    pub revision: u64,
    pub value: String,
}

#[vmux_api::ui_event]
pub struct GalleryProjectRequest {
    pub revision: u64,
    pub value: String,
}

#[vmux_api::ui_event]
pub struct GalleryBranchRequest {
    pub revision: u64,
    pub value: String,
}

#[vmux_api::ui_event]
pub struct GallerySubmitRequest {
    pub revision: u64,
}

#[vmux_api::ui_event]
pub struct GalleryStopRequest {
    pub revision: u64,
}

#[vmux_api::ui_event]
pub struct GalleryAttachRequest {
    pub revision: u64,
}

#[vmux_api::ui_event]
pub struct GalleryRemoveAttachmentRequest {
    pub revision: u64,
    pub index: usize,
}

#[vmux_api::ui_event]
pub struct GalleryPasteRequest {
    pub revision: u64,
}

#[vmux_api::ui_event]
pub struct GalleryStatusRequest {
    pub revision: u64,
    pub status: String,
}

#[vmux_api::contract(Default)]
pub struct ArgsPatch {
    pub choice: Option<String>,
    pub button_variant: Option<String>,
    pub button_size: Option<String>,
    pub disabled: Option<bool>,
    pub checked: Option<bool>,
    pub text: Option<String>,
    pub dialog_open: Option<bool>,
    pub progress: Option<f64>,
    pub menu: Option<String>,
    pub cursor: Option<usize>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub permission: Option<String>,
    pub project: Option<String>,
    pub branch: Option<String>,
    pub running: Option<bool>,
    pub has_attachment: Option<bool>,
    pub status: Option<String>,
}

#[vmux_api::ui_event]
pub struct GalleryConfigureRequest {
    pub revision: u64,
    pub args: ArgsPatch,
}

#[vmux_api::contract(Default)]
pub struct StoryPreset {
    pub name: String,
    pub args: ArgsPatch,
}

#[derive(serde::Deserialize)]
pub(crate) struct StoryDefinition {
    pub defaults: ArgsPatch,
    pub presets: Vec<StoryPreset>,
}

#[vmux_api::ui_event]
pub struct GalleryPresetRequest {
    pub revision: u64,
    pub index: usize,
}
