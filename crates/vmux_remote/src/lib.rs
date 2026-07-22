use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;
pub use vmux_wire::protocol::AgentAttachment;
use vmux_wire::protocol::AgentRunStatus;

pub const CONVERSATION_TITLE_MAX_GRAPHEMES: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Message {
    User {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<AgentAttachment>,
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

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self::User {
            text: text.into(),
            attachments: Vec::new(),
        }
    }

    pub fn user_with_attachments(
        text: impl Into<String>,
        attachments: Vec<AgentAttachment>,
    ) -> Self {
        Self::User {
            text: text.into(),
            attachments,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AssistantBlock {
    Text(String),
    /// The agent's streamed internal reasoning.
    Thinking(String),
    ToolUse {
        call_id: String,
        name: String,
        args: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_call_id: Option<String>,
    },
    Subagent(Box<SubagentBlock>),
    /// A proposed file edit rendered as an inline diff.
    Diff {
        call_id: String,
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    /// The agent's execution plan.
    Plan {
        steps: Vec<PlanStep>,
    },
}

/// A delegated agent operation surfaced by an ACP adapter.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SubagentBlock {
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

/// One entry in an agent [`AssistantBlock::Plan`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanStep {
    pub content: String,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteStatus {
    Idle,
    Streaming,
    Interrupted,
    Errored(String),
}

impl From<&AgentRunStatus> for RemoteStatus {
    fn from(status: &AgentRunStatus) -> Self {
        match status {
            AgentRunStatus::Idle => Self::Idle,
            AgentRunStatus::Streaming => Self::Streaming,
            AgentRunStatus::Interrupted => Self::Interrupted,
            AgentRunStatus::Errored(message) => Self::Errored(message.clone()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RemoteApproval {
    pub call_id: String,
    pub name: String,
    pub args_json: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RemoteMediaEntry {
    pub path: String,
    pub name: String,
    pub parent: String,
    pub mime_type: String,
    pub size: u64,
    pub is_dir: bool,
    pub preview_data_url: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RemoteSession {
    pub sid: String,
    #[serde(default)]
    pub title: String,
    pub name: String,
    pub runtime: String,
    pub model: Option<String>,
    pub cwd: String,
    pub status: RemoteStatus,
    pub approval: Option<RemoteApproval>,
    pub created_at_ms: u64,
}

pub fn conversation_title(messages: &[Message], fallback: &str) -> String {
    conversation_title_from_prompts(
        messages.iter().filter_map(|message| match message {
            Message::User { text, .. } => Some(text.as_str()),
            Message::Assistant { .. } | Message::ToolResult { .. } => None,
        }),
        fallback,
    )
}

pub fn conversation_title_from_prompts<'a>(
    prompts: impl IntoIterator<Item = &'a str>,
    fallback: &str,
) -> String {
    prompts
        .into_iter()
        .map(normalize_conversation_title)
        .find(|title| !title.is_empty())
        .unwrap_or_else(|| normalize_conversation_title(fallback))
}

fn normalize_conversation_title(value: &str) -> String {
    let mut title = String::new();
    let mut graphemes_written = 0;
    let mut pending_space = false;
    let mut truncated = false;

    for grapheme in value.graphemes(true) {
        if grapheme.chars().all(char::is_whitespace) {
            pending_space = !title.is_empty();
            continue;
        }
        let grapheme = grapheme
            .chars()
            .filter(|character| !is_disallowed_title_char(*character))
            .collect::<String>();
        if grapheme.is_empty() {
            continue;
        }
        if pending_space {
            if graphemes_written >= CONVERSATION_TITLE_MAX_GRAPHEMES {
                truncated = true;
                break;
            }
            title.push(' ');
            graphemes_written += 1;
            pending_space = false;
        }
        if graphemes_written >= CONVERSATION_TITLE_MAX_GRAPHEMES {
            truncated = true;
            break;
        }
        title.push_str(&grapheme);
        graphemes_written += 1;
    }

    if truncated {
        if let Some((start, _)) = title.grapheme_indices(true).next_back() {
            title.truncate(start);
        }
        title.push('…');
    }
    title
}

fn is_disallowed_title_char(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{00AD}'
                | '\u{034F}'
                | '\u{061C}'
                | '\u{180E}'
                | '\u{200B}'
                | '\u{200E}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{206F}'
                | '\u{FEFF}'
                | '\u{FFF9}'..='\u{FFFB}'
                | '\u{1BCA0}'..='\u{1BCA3}'
        )
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RemoteEvent {
    Session { session: RemoteSession },
    Snapshot { messages: Vec<Message> },
    Delta { text: String },
    Status { status: RemoteStatus },
    Approval { approval: Option<RemoteApproval> },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PromptRequest {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AgentAttachment>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NewChatRequest {
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApprovalRequest {
    pub call_id: String,
    pub allow: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineMediaQuery<'a> {
    pub start: usize,
    pub query: &'a str,
}

pub fn inline_media_query(draft: &str) -> Option<InlineMediaQuery<'_>> {
    draft.rmatch_indices('@').find_map(|(start, _)| {
        let boundary = start == 0
            || draft[..start]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        let query = &draft[start + 1..];
        (boundary && !query.chars().any(char::is_whitespace))
            .then_some(InlineMediaQuery { start, query })
    })
}

pub fn replace_inline_media_query(
    draft: &str,
    query: InlineMediaQuery<'_>,
    replacement: &str,
) -> String {
    let mut value = String::with_capacity(draft.len() + replacement.len());
    value.push_str(&draft[..query.start]);
    value.push_str(replacement);
    value
}

pub fn media_reference(entry: &RemoteMediaEntry) -> String {
    let encode = |value: &str| value.replace('%', "%25").replace(' ', "%20");
    if entry.parent == "~" {
        format!("~/{name}", name = encode(&entry.name))
    } else {
        format!(
            "{parent}/{name}",
            parent = encode(&entry.parent),
            name = encode(&entry.name)
        )
    }
}

pub fn media_display_path(entry: &RemoteMediaEntry) -> String {
    if entry.parent == "~" {
        format!("~/{}", entry.name)
    } else {
        format!("{}/{}", entry.parent.trim_end_matches('/'), entry.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_roundtrip() {
        let message = Message::user("hi");
        let json = serde_json::to_string(&message).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(message, back);
        assert!(!json.contains("attachments"));
    }

    #[test]
    fn user_deserializes_legacy_message_without_attachments() {
        let message: Message = serde_json::from_str(r#"{"User":{"text":"hi"}}"#).unwrap();
        assert_eq!(message, Message::user("hi"));
    }

    #[test]
    fn assistant_blocks_roundtrip() {
        let message = Message::Assistant {
            blocks: vec![
                AssistantBlock::Text("hello".into()),
                AssistantBlock::ToolUse {
                    call_id: "abc".into(),
                    name: "list_spaces".into(),
                    args: "{}".to_string(),
                    parent_call_id: None,
                },
            ],
        };
        let json = serde_json::to_string(&message).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(message, back);
    }

    #[test]
    fn tool_use_deserializes_without_parent_call_id() {
        let block: AssistantBlock =
            serde_json::from_str(r#"{"ToolUse":{"call_id":"abc","name":"run","args":"{}"}}"#)
                .unwrap();
        assert!(matches!(
            block,
            AssistantBlock::ToolUse {
                parent_call_id: None,
                ..
            }
        ));
    }

    #[test]
    fn new_chat_request_roundtrips() {
        let request = NewChatRequest {
            text: "start here".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let back: NewChatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.text, request.text);
    }

    #[test]
    fn prompt_request_deserializes_without_attachments() {
        let request: PromptRequest = serde_json::from_str(r#"{"text":"hello"}"#).unwrap();
        assert_eq!(request.text, "hello");
        assert!(request.attachments.is_empty());
    }

    #[test]
    fn inline_media_query_requires_an_open_token() {
        assert_eq!(
            inline_media_query("inspect @Pictures/scr"),
            Some(InlineMediaQuery {
                start: 8,
                query: "Pictures/scr",
            })
        );
        assert_eq!(inline_media_query("mail@example.com"), None);
        assert_eq!(inline_media_query("inspect @image.png next"), None);
    }

    #[test]
    fn conversation_title_uses_first_user_prompt() {
        let messages = vec![
            Message::user("  Show me something fun.\n in terminal  "),
            Message::Assistant { blocks: Vec::new() },
            Message::user("later"),
        ];
        assert_eq!(
            conversation_title(&messages, "Codex"),
            "Show me something fun. in terminal"
        );
    }

    #[test]
    fn conversation_title_falls_back_and_sanitizes() {
        assert_eq!(conversation_title(&[], "Codex"), "Codex");
        assert_eq!(
            conversation_title(&[Message::user("Fix \u{202e}\x1b title")], "Codex"),
            "Fix title"
        );
    }
}
