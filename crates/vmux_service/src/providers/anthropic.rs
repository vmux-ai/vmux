use serde::Deserialize;
use serde_json::{Value, json};

use crate::message::{AssistantBlock, Message};
use crate::stream::{StopReason, StreamEvent, ToolDef};

pub const PROVIDER: &str = "anthropic";
pub const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
pub const ENV_VAR: &str = "ANTHROPIC_API_KEY";
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

pub fn build_request(
    model: &str,
    messages: &[Message],
    tools: &[ToolDef],
    api_key: &str,
) -> reqwest::Request {
    let mut body = json!({
        "model": model,
        "max_tokens": 8192,
        "messages": messages_to_anthropic_blocks(messages),
        "stream": true,
    });
    if !tools.is_empty() {
        body["tools"] = json!(tools_to_anthropic(tools));
    }
    reqwest::Client::new()
        .post(ENDPOINT)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Accept", "text/event-stream")
        .header("Content-Type", "application/json")
        .json(&body)
        .build()
        .expect("anthropic: build_request")
}

pub fn parse_sse(payload: &str) -> Option<StreamEvent> {
    parse_messages_sse(payload)
}

fn messages_to_anthropic_blocks(messages: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();
    for msg in messages {
        match msg {
            Message::User { text } => out.push(json!({
                "role":"user",
                "content":[{"type":"text","text":text}]
            })),
            Message::Assistant { blocks } => {
                let content: Vec<Value> = blocks
                    .iter()
                    .map(|b| match b {
                        AssistantBlock::Text(t) => json!({"type":"text","text":t}),
                        AssistantBlock::ToolUse {
                            call_id,
                            name,
                            args,
                        } => json!({
                            "type":"tool_use",
                            "id":call_id,
                            "name":name,
                            "input": serde_json::from_str::<Value>(args).unwrap_or(json!({}))
                        }),
                        AssistantBlock::Diff { .. }
                        | AssistantBlock::Thinking(_)
                        | AssistantBlock::Plan { .. } => json!({"type":"text","text":""}),
                    })
                    .collect();
                out.push(json!({"role":"assistant","content": content}));
            }
            Message::ToolResult {
                call_id,
                content,
                is_error,
            } => out.push(json!({
                "role":"user",
                "content":[{
                    "type":"tool_result",
                    "tool_use_id":call_id,
                    "content":content,
                    "is_error":*is_error
                }]
            })),
        }
    }
    out
}

fn tools_to_anthropic(tools: &[ToolDef]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema,
            })
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum MessagesEvent {
    #[serde(rename = "content_block_start")]
    BlockStart {
        #[allow(dead_code)]
        #[serde(default)]
        index: usize,
        content_block: BlockStart,
    },
    #[serde(rename = "content_block_delta")]
    BlockDelta {
        #[allow(dead_code)]
        #[serde(default)]
        index: usize,
        delta: BlockDelta,
    },
    #[serde(rename = "content_block_stop")]
    BlockStop {
        #[allow(dead_code)]
        #[serde(default)]
        index: usize,
    },
    #[serde(rename = "message_delta")]
    MessageDelta { delta: MessageStopDelta },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum BlockStart {
    #[serde(rename = "text")]
    Text {
        #[serde(default)]
        text: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String },
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum BlockDelta {
    #[serde(rename = "text_delta")]
    Text { text: String },
    #[serde(rename = "input_json_delta")]
    JsonDelta { partial_json: String },
}

#[derive(Deserialize, Default)]
struct MessageStopDelta {
    #[serde(default)]
    stop_reason: Option<String>,
}

pub fn parse_messages_sse(frame: &str) -> Option<StreamEvent> {
    let data = frame.lines().find_map(|line| line.strip_prefix("data: "))?;
    let evt: MessagesEvent = serde_json::from_str(data).ok()?;
    match evt {
        MessagesEvent::BlockStart { content_block, .. } => match content_block {
            BlockStart::Text { text } if !text.is_empty() => Some(StreamEvent::TextDelta(text)),
            BlockStart::Text { .. } => None,
            BlockStart::ToolUse { id, name } => {
                Some(StreamEvent::ToolUseStart { call_id: id, name })
            }
        },
        MessagesEvent::BlockDelta { delta, .. } => match delta {
            BlockDelta::Text { text } => Some(StreamEvent::TextDelta(text)),
            BlockDelta::JsonDelta { partial_json } => Some(StreamEvent::ToolUseArgsDelta {
                call_id: String::new(),
                json_chunk: partial_json,
            }),
        },
        MessagesEvent::BlockStop { .. } => Some(StreamEvent::ToolUseEnd {
            call_id: String::new(),
        }),
        MessagesEvent::MessageDelta { delta } => {
            let reason = match delta.stop_reason.as_deref() {
                Some("tool_use") => StopReason::ToolUse,
                Some("max_tokens") => StopReason::MaxTokens,
                Some(_) => StopReason::EndTurn,
                None => return None,
            };
            Some(StreamEvent::StopTurn { reason })
        }
        MessagesEvent::MessageStop | MessagesEvent::Other => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = include_str!("../../tests/fixtures/anthropic/text.sse");
    const TOOLS: &str = include_str!("../../tests/fixtures/anthropic/tools.sse");

    fn frames(raw: &str) -> Vec<&str> {
        raw.split("\n\n").filter(|s| !s.trim().is_empty()).collect()
    }

    #[test]
    fn parses_text_block_into_deltas_then_end_turn() {
        let events: Vec<StreamEvent> = frames(TEXT)
            .into_iter()
            .filter_map(parse_messages_sse)
            .collect();
        assert!(events.contains(&StreamEvent::TextDelta("hello".into())));
        assert!(events.contains(&StreamEvent::TextDelta(" world".into())));
        assert!(events.iter().any(|e| matches!(
            e,
            StreamEvent::StopTurn {
                reason: StopReason::EndTurn
            }
        )));
    }

    #[test]
    fn parses_tool_use_block() {
        let events: Vec<StreamEvent> = frames(TOOLS)
            .into_iter()
            .filter_map(parse_messages_sse)
            .collect();
        assert!(events.iter().any(|e| matches!(
            e,
            StreamEvent::ToolUseStart { call_id, name } if call_id == "tool_1" && name == "list_spaces"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            StreamEvent::ToolUseArgsDelta { json_chunk, .. } if json_chunk == "{\"filter\":\"all\"}"
        )));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::ToolUseEnd { .. }))
        );
        assert!(events.iter().any(|e| matches!(
            e,
            StreamEvent::StopTurn {
                reason: StopReason::ToolUse
            }
        )));
    }

    #[test]
    fn build_request_sets_x_api_key_and_version_header() {
        let msgs = vec![Message::User { text: "hi".into() }];
        let req = build_request("claude-sonnet-4-6", &msgs, &[], "test-key");
        assert_eq!(req.url().as_str(), ENDPOINT);
        assert_eq!(req.headers().get("x-api-key").unwrap(), "test-key");
        assert_eq!(
            req.headers().get("anthropic-version").unwrap(),
            "2023-06-01"
        );
    }
}
