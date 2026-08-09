//! The rendered chat transcript model, shared by every host that draws a conversation.
//!
//! Grouping happens once on the daemon side; hosts receive `Vec<ChatItem>` and render it.

use crate::prompt_media::ChatSubmitAttachment;

/// The page's block type inside a [`ChatTurn`]. Mirrors `vmux_service::message::AssistantBlock`
/// plus folded tool results and reconnect progress.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// Page representation of `vmux_service::message::SubagentBlock`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChatSubagent {
    pub call_id: String,
    pub provider: String,
    pub title: String,
    pub status: String,
    pub action: String,
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

/// Mirror of `vmux_service::message::PlanStep`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChatPlanStep {
    pub content: String,
    pub status: String,
}

/// A rendered conversation entry: a user bubble or a grouped assistant turn. Built backend by
/// `group_turns`, carried as JSON in `ChatSnapshot::messages_json`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChatItem {
    User {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<ChatSubmitAttachment>,
    },
    Turn(ChatTurn),
}

impl ChatItem {
    pub fn user(text: impl Into<String>) -> Self {
        Self::User {
            text: text.into(),
            context: None,
            attachments: Vec::new(),
        }
    }
}

/// One assistant turn: its ordered prose/activity timeline and run-state.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChatTurn {
    /// Prose, thinking, tools, reconnects, plans, and diffs in transcript order.
    pub blocks: Vec<ChatBlock>,
    /// True only for the live (tail) turn while the run is active.
    pub running: bool,
    /// Final wall-clock seconds for a turn that finished this process; `None` otherwise.
    pub duration_secs: Option<u32>,
    /// Number of non-prose activity blocks.
    pub step_count: u32,
}

/// The name of a tool an agent called.
#[derive(Clone, Copy, Debug)]
pub struct ToolName<'a>(pub &'a str);

impl ToolName<'_> {
    /// Whether this is a review or approval tool, which the transcript renders differently
    /// because its output is a verdict on other work rather than work of its own.
    pub fn is_guardian(&self) -> bool {
        let lower = self.0.to_ascii_lowercase();
        lower.contains("guardian")
            || lower.contains("approval")
            || lower == "review"
            || lower.ends_with("_review")
            || lower.ends_with(".review")
            || lower.ends_with(":review")
    }
}

impl ChatTurn {
    pub fn latest_top_level_tool_index(&self) -> Option<usize> {
        self.blocks
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, block)| match block {
                ChatBlock::ToolUse { .. } if self.parent_tool_index(index).is_none() => Some(index),
                _ => None,
            })
    }

    pub fn parent_tool_index(&self, index: usize) -> Option<usize> {
        let mut parent = self.direct_parent_index(index)?;
        for _ in 0..self.blocks.len() {
            let Some(next) = self.direct_parent_index(parent) else {
                break;
            };
            if next == parent {
                break;
            }
            parent = next;
        }
        Some(parent)
    }

    fn direct_parent_index(&self, index: usize) -> Option<usize> {
        match self.blocks.get(index)? {
            ChatBlock::ToolUse {
                parent_call_id: Some(parent_call_id),
                ..
            } => self.call_index(parent_call_id),
            ChatBlock::Subagent(subagent) => subagent
                .parent_call_id
                .as_deref()
                .and_then(|parent_call_id| self.call_index(parent_call_id)),
            ChatBlock::ToolUse { name, .. } if ToolName(name).is_guardian() => {
                self.guardian_parent_index(index)
            }
            ChatBlock::ToolResult { call_id, .. } if !call_id.is_empty() => {
                self.call_index(call_id)
            }
            _ => None,
        }
    }

    fn call_index(&self, call_id: &str) -> Option<usize> {
        self.blocks.iter().position(|block| match block {
            ChatBlock::ToolUse {
                call_id: block_call_id,
                ..
            } => block_call_id == call_id,
            ChatBlock::Subagent(subagent) => subagent.call_id == call_id,
            _ => false,
        })
    }

    fn guardian_parent_index(&self, index: usize) -> Option<usize> {
        for (candidate, block) in self.blocks[..index].iter().enumerate().rev() {
            match block {
                ChatBlock::ToolUse { name, .. } if ToolName(name).is_guardian() => {}
                ChatBlock::ToolUse { .. } | ChatBlock::Subagent(_) => return Some(candidate),
                _ => return None,
            }
        }
        None
    }
}

pub fn latest_tool_location(items: &[ChatItem]) -> Option<(usize, usize)> {
    items
        .iter()
        .enumerate()
        .rev()
        .find_map(|(item_index, item)| match item {
            ChatItem::Turn(turn) => turn
                .latest_top_level_tool_index()
                .map(|block_index| (item_index, block_index)),
            ChatItem::User { .. } => None,
        })
}

/// The curated verbs the running-turn header cycles through (owned by the shared contract, not
/// the view). The page picks one at random every few seconds while streaming.
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
