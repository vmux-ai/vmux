use super::CommandBarPick;

#[vmux_api::ui_event(Eq, targets = ["command-bar", "start", "layout"])]
pub struct PromptRequest {
    pub text: String,
    pub target_url: Option<String>,
    pub attachments: Vec<crate::prompt_media::ChatSubmitAttachment>,
}

impl PromptRequest {
    pub fn new(
        text: &str,
        target_url: &str,
        attachments: &[crate::prompt_media::ChatAttachment],
    ) -> Self {
        let mut submitted = Vec::with_capacity(attachments.len());
        for attachment in attachments {
            submitted.push(crate::prompt_media::ChatSubmitAttachment::from(attachment));
        }
        Self {
            text: text.to_string(),
            target_url: (!target_url.is_empty()).then(|| target_url.to_string()),
            attachments: submitted,
        }
    }
}

#[vmux_api::ui_event(Eq, targets = ["command-bar", "start", "layout"])]
pub struct OpenRequest {
    pub value: String,
    pub open: Option<crate::open_target::OpenTarget>,
}

impl OpenRequest {
    pub fn new(value: &str, open: Option<crate::open_target::OpenTarget>) -> Self {
        Self {
            value: value.to_string(),
            open,
        }
    }
}

#[vmux_api::ui_event(Eq, targets = ["command-bar", "start", "layout"])]
pub struct TerminalRequest {
    pub value: String,
}

#[vmux_api::ui_event(Eq, targets = ["command-bar", "start", "layout"])]
pub struct InvokeRequest {
    pub id: String,
    pub open: Option<crate::open_target::OpenTarget>,
}

#[vmux_api::ui_event(Eq, targets = ["command-bar", "start", "layout"])]
pub struct SwitchSpaceRequest {
    pub id: String,
}

#[vmux_api::ui_event(Eq, targets = ["command-bar", "start", "layout"])]
pub struct SwitchTabRequest {
    pub pane: u64,
    pub index: usize,
}

#[vmux_api::ui_event(Eq, targets = ["command-bar", "start", "layout"])]
pub struct ExRequest {
    pub line: String,
}

#[vmux_api::ui_event(Eq, targets = ["command-bar", "start", "layout"])]
pub struct PickRequest {
    pub pick: CommandBarPick,
}

#[vmux_api::ui_event(Copy, Default, Eq, targets = ["command-bar", "start", "layout"])]
pub struct DismissRequest;

#[vmux_api::ui_event(Default, Eq, targets = ["command-bar", "start", "layout"])]
pub struct CommandPaletteDraftRequest {
    pub open_id: super::OpenId,
    pub query: String,
    pub start: bool,
    pub target_url: String,
    pub selected: u32,
    pub navigating: bool,
}

#[vmux_api::ui_event(Default, Eq, targets = ["command-bar", "start", "layout"])]
pub struct CommandPaletteSelectionRequest {
    pub open_id: super::OpenId,
    pub selected: u32,
    pub navigating: bool,
}

#[vmux_api::ui_event(Default, Eq, targets = ["command-bar", "start", "layout"])]
pub struct CommandPalettePromptHistoryRequest {
    pub open_id: super::OpenId,
    pub agent: String,
    pub cwd: String,
}

#[vmux_api::ui_event(Default, Eq, targets = ["command-bar", "start", "layout"])]
pub struct CommandPaletteBranchesRequest {
    pub open_id: super::OpenId,
    pub project: String,
}

#[vmux_api::ui_event(Default, Eq, targets = ["command-bar", "start", "layout"])]
pub struct CommandPaletteRemoveAttachmentRequest {
    pub open_id: super::OpenId,
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExCommandName {
    pub name: &'static str,
    pub hint: &'static str,
}

impl ExCommandName {
    pub const ALL: [Self; 8] = [
        Self {
            name: "w",
            hint: "ex-write",
        },
        Self {
            name: "wq",
            hint: "ex-write-quit",
        },
        Self {
            name: "q",
            hint: "ex-quit",
        },
        Self {
            name: "q!",
            hint: "ex-quit-force",
        },
        Self {
            name: "noh",
            hint: "ex-nohighlight",
        },
        Self {
            name: "d",
            hint: "ex-delete",
        },
        Self {
            name: "y",
            hint: "ex-yank",
        },
        Self {
            name: "s/",
            hint: "ex-substitute",
        },
    ];

    pub fn matching(typed: &str) -> Vec<Self> {
        let mut found = Vec::new();
        for entry in Self::ALL {
            if entry.name.starts_with(typed) {
                found.push(entry);
            }
        }
        found
    }
}

#[vmux_api::ui_event(Default, Eq, target = "start")]
pub struct StartSelectWorkspace {
    pub current_dir: String,
}

#[vmux_api::ui_event(Default, target = "start")]
pub struct StartBranchesRequest {
    pub project: String,
}

#[vmux_api::contract(Default)]
pub struct StartProjectBranches {
    pub project: String,
    pub branches: Vec<crate::space::ProjectBranch>,
}

#[vmux_api::ui_event(Default, target = "start")]
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

#[vmux_api::ui_event(Default, target = "start")]
pub struct StartSelectModel {
    pub agent_key: String,
    pub model_id: String,
}

#[vmux_api::ui_event(Default, target = "start")]
pub struct StartSelectMode {
    pub agent_key: String,
    pub mode_id: String,
}
