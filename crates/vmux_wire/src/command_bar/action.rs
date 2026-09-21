use super::CommandBarPick;

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum CommandBarActionEvent {
    Prompt {
        text: String,
        target_url: Option<String>,
        attachments: Vec<crate::prompt_media::ChatSubmitAttachment>,
    },
    Open {
        value: String,
        open: Option<crate::open_target::OpenTarget>,
    },
    Terminal {
        value: String,
    },
    Command {
        id: String,
        open: Option<crate::open_target::OpenTarget>,
    },
    Space {
        id: String,
    },
    SwitchTab {
        pane: u64,
        index: usize,
    },
    Ex {
        line: String,
    },
    Pick {
        pick: CommandBarPick,
    },
    Dismiss,
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

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct StartSelectWorkspace {
    pub current_dir: String,
}

pub const START_PROJECT_BRANCHES_EVENT: &str = "start_project_branches";

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct StartBranchesRequest {
    pub project: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct StartProjectBranches {
    pub project: String,
    pub branches: Vec<crate::space::ProjectBranch>,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct StartGoToBranch {
    pub project: String,
    pub branch: String,
    #[serde(default)]
    pub checkout: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AgentModels {
    pub agent_key: String,
    pub url: String,
    pub selected: String,
    pub models: Vec<crate::room::ModelOptionEntry>,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AgentModes {
    pub agent_key: String,
    pub url: String,
    pub selected: String,
    pub modes: Vec<crate::protocol::AcpModeOption>,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct StartSelectModel {
    pub agent_key: String,
    pub model_id: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct StartSelectMode {
    pub agent_key: String,
    pub mode_id: String,
}

impl CommandBarActionEvent {
    pub fn open(value: &str, open: Option<crate::open_target::OpenTarget>) -> Self {
        Self::Open {
            value: value.to_string(),
            open,
        }
    }

    pub fn prompt(
        text: &str,
        target_url: &str,
        attachments: &[crate::prompt_media::ChatAttachment],
    ) -> Self {
        let mut submitted = Vec::with_capacity(attachments.len());
        for attachment in attachments {
            submitted.push(crate::prompt_media::ChatSubmitAttachment {
                path: attachment.path.clone(),
                name: attachment.name.clone(),
                mime_type: attachment.mime_type.clone(),
                size: attachment.size,
            });
        }
        Self::Prompt {
            text: text.to_string(),
            target_url: (!target_url.is_empty()).then(|| target_url.to_string()),
            attachments: submitted,
        }
    }
}
