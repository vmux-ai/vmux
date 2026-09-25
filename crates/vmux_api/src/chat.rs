use crate::prompt_media::ChatAttachment;

#[vmux_api::contract(Eq)]
pub enum ChatBlock {
    Text(String),
    Thinking(String),
    ToolUse {
        call_id: String,
        name: String,
        args: String,
        parent_call_id: Option<String>,
    },
    Subagent(Box<ChatSubagent>),
    Diff {
        call_id: String,
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    Plan {
        steps: Vec<ChatPlanStep>,
    },
    ToolResult {
        call_id: String,
        content: String,
        is_error: bool,
    },
    Reconnect {
        attempt: u32,
        total: u32,
    },
}

#[vmux_api::contract(Eq)]
pub struct ChatSubagent {
    pub call_id: String,
    pub provider: String,
    pub title: String,
    pub status: String,
    pub activity: String,
    pub agent_name: Option<String>,
    pub thread_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub child_thread_ids: Vec<String>,
    pub parent_call_id: Option<String>,
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub raw_input: String,
}

#[vmux_api::contract(Eq)]
pub struct ChatPlanStep {
    pub content: String,
    pub status: String,
}

#[vmux_api::contract(Eq)]
pub enum ChatTurnRow {
    Text {
        index: u32,
        text: String,
    },
    Thinking {
        index: u32,
        text: String,
        latest: bool,
    },
    Tool(ChatToolCall),
    FinishedTools {
        index: u32,
        calls: Vec<ChatToolCall>,
    },
    Subagent(ChatSubagentState),
    Plan {
        index: u32,
        steps: Vec<ChatPlanItem>,
    },
    Diff(ChatDiff),
    ToolResult {
        index: u32,
        content: String,
        is_error: bool,
    },
    Reconnect {
        index: u32,
        attempt: u32,
        total: u32,
    },
}

impl ChatTurnRow {
    pub fn index(&self) -> u32 {
        match self {
            Self::Text { index, .. }
            | Self::Thinking { index, .. }
            | Self::FinishedTools { index, .. }
            | Self::Plan { index, .. }
            | Self::ToolResult { index, .. }
            | Self::Reconnect { index, .. } => *index,
            Self::Tool(call) => call.index,
            Self::Subagent(subagent) => subagent.index,
            Self::Diff(diff) => diff.index,
        }
    }
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatToolCall {
    pub index: u32,
    pub name: String,
    pub kind: ChatToolKind,
    pub activity: ChatActivityKind,
    pub fallback_label: String,
    pub file_path: Option<String>,
    pub arguments: ChatToolArguments,
    pub children: Vec<ChatToolChild>,
    pub live: bool,
}

#[vmux_api::contract(Eq)]
pub enum ChatToolChild {
    Tool(ChatToolChildCall),
    Subagent(ChatSubagentSummary),
    Result {
        index: u32,
        content: String,
        is_error: bool,
    },
}

impl ChatToolChild {
    pub fn index(&self) -> u32 {
        match self {
            Self::Tool(call) => call.index,
            Self::Subagent(subagent) => subagent.index,
            Self::Result { index, .. } => *index,
        }
    }
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatToolChildCall {
    pub index: u32,
    pub name: String,
    pub kind: ChatToolKind,
    pub activity: ChatActivityKind,
    pub fallback_label: String,
    pub file_path: Option<String>,
    pub arguments: ChatToolArguments,
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatSubagentState {
    pub index: u32,
    pub call_id: String,
    pub provider: String,
    pub title: String,
    pub status: ChatSubagentStatus,
    pub activity: String,
    pub agent_name: Option<String>,
    pub thread_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub child_threads: String,
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub raw_input: String,
    pub children: Vec<ChatToolChild>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatSubagentSummary {
    pub index: u32,
    pub title: String,
    pub status: ChatSubagentStatus,
    pub provider: String,
    pub agent_name: Option<String>,
    pub prompt: Option<String>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatDiff {
    pub index: u32,
    pub path: String,
    pub name: String,
    pub lines: Vec<ChatDiffLine>,
}

#[vmux_api::contract(Eq)]
pub struct ChatDiffLine {
    pub kind: ChatDiffLineKind,
    pub text: String,
}

#[vmux_api::contract(Copy, Eq)]
pub enum ChatDiffLineKind {
    Removed,
    Added,
}

#[vmux_api::contract(Eq)]
pub struct ChatPlanItem {
    pub content: String,
    pub status: ChatPlanStatus,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub enum ChatPlanStatus {
    Complete,
    Active,
    #[default]
    Pending,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub enum ChatSubagentStatus {
    Running,
    Complete,
    Failed,
    #[default]
    Pending,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub enum ChatToolKind {
    Guardian,
    ReadFile,
    ReadSkill,
    WriteFile,
    Layout,
    Worktree,
    Image,
    Screenshot,
    OpenPage,
    Browser,
    Search,
    Command,
    #[default]
    Other,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub enum ChatActivityKind {
    #[default]
    None,
    Thinking,
    Writing,
    Installing,
    Awaiting,
    Python,
    ReadFile,
    WriteFile,
    Layout,
    Worktree,
    Search,
    Image,
    Screenshot,
    OpenPage,
    Command,
    Browser,
    Guardian,
    Subagent,
    Tool,
    Output,
    Error,
    Plan,
    Diff,
    Reconnect,
}

#[vmux_api::contract(Default, Eq)]
pub enum ChatToolArguments {
    #[default]
    None,
    Fields(Vec<ChatToolArgument>),
    Value(ChatToolArgumentValue),
    Raw(String),
}

#[vmux_api::contract(recursive, Eq)]
pub struct ChatToolArgument {
    pub name: String,
    pub label: String,
    pub value: ChatToolArgumentValue,
}

#[vmux_api::contract(recursive, Eq)]
pub enum ChatToolArgumentValue {
    Path(String),
    Code(String),
    Text(String),
    Bool(bool),
    Number(String),
    List(Vec<ChatToolArgument>),
    Object(Vec<ChatToolArgument>),
    Null,
}

#[vmux_api::contract(Eq)]
pub enum ChatItem {
    User {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<ChatAttachment>,
        #[serde(default, skip_serializing_if = "is_zero")]
        created_at_ms: u64,
    },
    Turn(ChatTurn),
}

impl ChatItem {
    pub fn user(text: impl Into<String>) -> Self {
        Self::User {
            text: text.into(),
            context: None,
            attachments: Vec::new(),
            created_at_ms: 0,
        }
    }
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatTurn {
    pub blocks: Vec<ChatBlock>,
    #[serde(default)]
    pub rows: Vec<ChatTurnRow>,
    #[serde(default)]
    pub copy_text: String,
    #[serde(default)]
    pub activity: ChatActivityKind,
    #[serde(default)]
    pub active_subagents: u32,
    #[serde(default)]
    pub active_tasks: u32,
    pub running: bool,
    pub duration_secs: Option<u32>,
    pub step_count: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub created_at_ms: u64,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

pub const WORKING_VERB_IDS: &[&str] = &[
    "agent-working-working",
    "agent-working-thinking",
    "agent-working-pondering",
    "agent-working-noodling",
    "agent-working-percolating",
    "agent-working-conjuring",
    "agent-working-cooking",
    "agent-working-brewing",
    "agent-working-musing",
    "agent-working-ruminating",
    "agent-working-scheming",
    "agent-working-synthesizing",
    "agent-working-tinkering",
    "agent-working-churning",
    "agent-working-vibing",
    "agent-working-simmering",
    "agent-working-crafting",
    "agent-working-divining",
    "agent-working-mulling",
    "agent-working-spelunking",
];

#[vmux_api::contract(Copy, Eq)]
pub enum ChatKey {
    ListNext,
    ListPrevious,
    ListChoose,
    HistoryOlder,
    HistoryNewer,
    Submit,
    DismissSelector,
    Interrupt,
    Cancel,
}

#[vmux_api::ui_event(Default, Eq, targets = ["sessions", "agent", "start"])]
pub struct PromptHistoryRequest {
    pub agent: String,
    pub cwd: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct PromptHistory {
    pub prompts: Vec<String>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ResumableSessionEntry {
    pub kind: String,
    pub sid: String,
    pub cwd: String,
    pub url: String,
    pub title: String,
    pub latest: String,
    pub subtitle: String,
    pub age_seconds: u64,
    #[serde(default)]
    pub updated_at: String,
    pub agent_name: String,
    pub project: String,
    pub branch: String,
    pub cross_runtime: bool,
}

impl ResumableSessionEntry {
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || self.sid.to_lowercase().contains(&query)
            || self.title.to_lowercase().contains(&query)
            || self.cwd.to_lowercase().contains(&query)
    }
}
#[vmux_api::contract(Default)]
pub struct ResumableSessions {
    pub request_id: u64,
    pub query: String,
    pub sessions: Vec<ResumableSessionEntry>,
    pub offset: u32,
    pub total: u32,
}

impl ResumableSessions {
    pub const PAGE: u32 = 50;

    pub fn reaches(&self) -> u32 {
        self.offset + self.sessions.len() as u32
    }

    pub fn has_more(&self) -> bool {
        self.reaches() < self.total
    }
}
#[vmux_api::contract(Default, Eq)]
pub struct SlashCommandEntry {
    pub name: String,
    pub description: String,
}
#[vmux_api::contract(Default)]
pub struct SlashCommands {
    pub commands: Vec<SlashCommandEntry>,
}
#[vmux_api::ui_event(Default, targets = ["sessions", "agent", "start"])]
pub struct ResumeListRequest {
    pub request_id: u64,
    pub query: String,
    pub offset: u32,
}
#[vmux_api::ui_event(Default, targets = ["sessions", "agent", "start"])]
pub struct ResumeSession {
    pub kind: String,
    pub sid: String,
    pub cwd: String,
}

impl SlashCommands {
    pub fn for_start() -> Self {
        let commands = vec![
            SlashCommandEntry {
                name: "upload".into(),
                description: "Attach files".into(),
            },
            SlashCommandEntry {
                name: "resume".into(),
                description: "Resume a past session".into(),
            },
        ];
        #[cfg(host)]
        let commands = {
            let mut commands = commands;
            commands.push(SlashCommandEntry {
                name: "mcp".into(),
                description: String::new(),
            });
            commands
        };
        Self { commands }
    }

    pub fn for_agent(cross_runtime: bool, has_models: bool) -> Self {
        let mut commands = vec![
            SlashCommandEntry {
                name: "upload".into(),
                description: "Attach files".into(),
            },
            SlashCommandEntry {
                name: "resume".into(),
                description: "Resume a past session".into(),
            },
        ];
        #[cfg(host)]
        commands.push(SlashCommandEntry {
            name: "mcp".into(),
            description: String::new(),
        });
        if has_models {
            commands.push(SlashCommandEntry {
                name: "model".into(),
                description: "Select model".into(),
            });
        }
        if cross_runtime {
            commands.push(SlashCommandEntry {
                name: "cli".into(),
                description: "Continue this session in the CLI".into(),
            });
        }
        Self { commands }
    }
}
