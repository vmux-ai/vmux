use super::CommandBarPick;

#[vmux_api::ui_event(Eq)]
pub struct PromptRequest {
    pub text: String,
    pub target_url: Option<String>,
    pub attachments: Vec<crate::prompt_media::ChatSubmitAttachment>,
}

#[vmux_api::ui_event(Eq)]
pub struct OpenRequest {
    pub value: String,
    pub open: Option<crate::open_target::OpenTarget>,
}

#[vmux_api::ui_event(Eq)]
pub struct TerminalRequest {
    pub value: String,
}

#[vmux_api::ui_event(Eq)]
pub struct InvokeRequest {
    pub id: String,
    pub open: Option<crate::open_target::OpenTarget>,
}

#[vmux_api::ui_event(Eq)]
pub struct SwitchSpaceRequest {
    pub id: String,
}

#[vmux_api::ui_event(Eq)]
pub struct SwitchTabRequest {
    pub pane: u64,
    pub index: usize,
}

#[vmux_api::ui_event(Eq)]
pub struct ExRequest {
    pub line: String,
}

#[vmux_api::ui_event(Eq)]
pub struct PickRequest {
    pub pick: CommandBarPick,
}

#[vmux_api::ui_event]
pub struct DismissRequest;

#[vmux_api::ui_event(Default, Eq)]
pub struct CommandPaletteDraftRequest {
    pub open_id: super::OpenId,
    pub query: String,
    pub start: bool,
    pub target_url: String,
    pub selected: u32,
    pub navigating: bool,
}

#[vmux_api::ui_event(Copy, Default, Eq)]
pub struct CommandPaletteSubmitRequest {
    pub open_id: super::OpenId,
}

#[vmux_api::ui_event(Copy, Default, Eq)]
pub struct CommandPaletteHistoryMoveRequest {
    pub open_id: super::OpenId,
    pub older: bool,
}

#[vmux_api::ui_event(Copy, Default, Eq)]
pub struct CommandPaletteActivateRequest {
    pub open_id: super::OpenId,
    pub index: u32,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct CommandPaletteRemoveAttachmentRequest {
    pub open_id: super::OpenId,
    pub path: String,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct StartSelectWorkspace {
    pub current_dir: String,
}

#[vmux_api::ui_event(Default)]
pub struct StartBranchesRequest {
    pub project: String,
}

#[vmux_api::contract(Default)]
pub struct StartProjectBranches {
    pub project: String,
    pub branches: Vec<crate::space::ProjectBranch>,
}

#[vmux_api::ui_event(Default)]
pub struct StartGoToBranch {
    pub project: String,
    pub branch: String,
    #[serde(default)]
    pub checkout: String,
}

#[vmux_api::contract(Default)]
pub struct AgentModels {
    pub agent_key: String,
    pub url: String,
    pub selected: String,
    pub models: Vec<crate::room::ModelOptionEntry>,
}

#[vmux_api::contract(Default)]
pub struct AgentModes {
    pub agent_key: String,
    pub url: String,
    pub selected: String,
    pub modes: Vec<crate::protocol::AcpModeOption>,
}

#[vmux_api::ui_event(Default)]
pub struct StartSelectModel {
    pub agent_key: String,
    pub model_id: String,
}

#[vmux_api::ui_event(Default)]
pub struct StartSelectMode {
    pub agent_key: String,
    pub mode_id: String,
}
