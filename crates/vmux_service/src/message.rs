use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Message {
    User {
        text: String,
    },
    Assistant {
        blocks: Vec<AssistantBlock>,
    },
    ToolResult {
        call_id: String,
        content: String,
        is_error: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AssistantBlock {
    Text(String),
    /// The agent's streamed internal reasoning (ACP `AgentThoughtChunk`), shown as a
    /// collapsible "Thinking" section.
    Thinking(String),
    ToolUse {
        call_id: String,
        name: String,
        args: String,
    },
    /// A proposed file edit (ACP `ToolCallContent::Diff`), rendered as an inline diff in the chat.
    Diff {
        call_id: String,
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    /// The agent's execution plan / task tree (ACP `SessionUpdate::Plan`). Re-sent in full on each
    /// update, so the projector replaces the single plan block in place.
    Plan { steps: Vec<PlanStep> },
}

/// One entry in an agent [`AssistantBlock::Plan`]. `status` is `pending` | `in_progress` |
/// `completed`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanStep {
    pub content: String,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_roundtrip() {
        let m = Message::User { text: "hi".into() };
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn assistant_blocks_roundtrip() {
        let m = Message::Assistant {
            blocks: vec![
                AssistantBlock::Text("hello".into()),
                AssistantBlock::ToolUse {
                    call_id: "abc".into(),
                    name: "list_spaces".into(),
                    args: "{}".to_string(),
                },
            ],
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn tool_result_roundtrip() {
        let m = Message::ToolResult {
            call_id: "abc".into(),
            content: "ok".into(),
            is_error: false,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }
}
