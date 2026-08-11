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

/// How much the agent still has in flight: sub-agents running, and plan steps not yet done.
///
/// Derived from the transcript rather than reported, so any surface holding the transcript can
/// show it without the host sending a count.
pub fn activity_counts(items: &[ChatItem]) -> (usize, usize) {
    let mut subagents = 0usize;
    let mut tasks = 0usize;
    for item in items {
        let ChatItem::Turn(turn) = item else {
            continue;
        };
        for block in &turn.blocks {
            match block {
                ChatBlock::Subagent(subagent) if subagent.status == "in_progress" => {
                    subagents += 1;
                }
                ChatBlock::Plan { steps } => {
                    tasks += steps
                        .iter()
                        .filter(|step| step.status != "completed")
                        .count();
                }
                _ => {}
            }
        }
    }
    (subagents, tasks)
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

/// The event id [`ChatKey`] is pushed under.
pub const CHAT_KEY_EVENT: &str = "chat-key";

/// What the keymap made of a key the chat page handed over, sent back to the page that sent it.
///
/// The page keeps the doing; only the deciding moved. Which list a [`ChatKey::ListChoose`] lands
/// in, and which row of it, is derived from the draft text — which lives in the browser's own
/// `<textarea>` and changes on every keystroke. Shipping that to the host and back would make the
/// answer one character stale, so the verb travels instead, in the direction that already holds
/// the state.
///
/// This is why the page no longer names a key anywhere: `Enter` is four lines in the keymap, each
/// carrying the context it applies in, rather than four branches in a `keydown`.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum ChatKey {
    /// Highlight the next row of whichever list is open.
    ListNext,
    ListPrevious,
    /// Commit the highlighted row: answer the approval, answer the question, or take the match.
    ListChoose,
    /// Recall an earlier prompt into the draft.
    HistoryOlder,
    HistoryNewer,
    Submit,
    /// Close the open picker, dropping the `@`-mention or the draft that opened it.
    DismissSelector,
    /// Send everything queued, and clear a draft nothing is waiting on.
    Interrupt,
    /// Stop the running turn.
    Cancel,
}

#[cfg(test)]
mod activity_counts_tests {
    use super::*;

    fn subagent(status: &str) -> ChatBlock {
        ChatBlock::Subagent(Box::new(ChatSubagent {
            call_id: String::new(),
            provider: String::new(),
            title: String::new(),
            status: status.to_string(),
            action: String::new(),
            agent_name: None,
            thread_id: None,
            parent_thread_id: None,
            child_thread_ids: Vec::new(),
            parent_call_id: None,
            prompt: None,
            model: None,
            reasoning_effort: None,
            raw_input: String::new(),
        }))
    }

    fn turn(blocks: Vec<ChatBlock>) -> ChatItem {
        ChatItem::Turn(ChatTurn {
            blocks,
            running: false,
            duration_secs: None,
            step_count: 0,
        })
    }

    /// The composer shows these as "still working" counts, so anything already finished has to
    /// drop out — a completed sub-agent or plan step reads as outstanding work otherwise.
    #[test]
    fn only_unfinished_work_counts() {
        let items = vec![
            turn(vec![
                subagent("in_progress"),
                subagent("completed"),
                ChatBlock::Plan {
                    steps: vec![
                        ChatPlanStep {
                            content: "a".into(),
                            status: "completed".into(),
                        },
                        ChatPlanStep {
                            content: "b".into(),
                            status: "in_progress".into(),
                        },
                        ChatPlanStep {
                            content: "c".into(),
                            status: "pending".into(),
                        },
                    ],
                },
            ]),
            turn(vec![subagent("in_progress")]),
            ChatItem::User {
                text: "ignored".into(),
                context: None,
                attachments: Vec::new(),
            },
        ];

        assert_eq!(activity_counts(&items), (2, 2));
    }

    #[test]
    fn an_empty_transcript_has_nothing_outstanding() {
        assert_eq!(activity_counts(&[]), (0, 0));
    }
}
