use serde::Deserialize;
use serde_json::{Value, json};

use crate::message::{AssistantBlock, Message};
use crate::providers::openai_shared::tools_to_function_specs;
use crate::stream::{StopReason, StreamEvent, ToolDef};

pub const PROVIDER: &str = "openai";
pub const ENDPOINT: &str = "https://api.openai.com/v1/responses";
pub const ENV_VAR: &str = "OPENAI_API_KEY";
pub const DEFAULT_MODEL: &str = "gpt-5";

pub fn build_request(
    model: &str,
    messages: &[Message],
    tools: &[ToolDef],
    api_key: &str,
) -> reqwest::Request {
    let mut body = json!({
        "model": model,
        "input": messages_to_responses_input(messages),
        "stream": true,
    });
    if !tools.is_empty() {
        body["tools"] = json!(tools_to_responses_tools(tools));
    }
    reqwest::Client::new()
        .post(ENDPOINT)
        .bearer_auth(api_key)
        .header("Accept", "text/event-stream")
        .header("Content-Type", "application/json")
        .json(&body)
        .build()
        .expect("openai: build_request")
}

pub fn parse_sse(payload: &str) -> Option<StreamEvent> {
    parse_responses_sse(payload)
}

fn messages_to_responses_input(messages: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();
    for msg in messages {
        match msg {
            Message::User { text } => out.push(json!({
                "type":"message","role":"user","content":[{"type":"input_text","text":text}]
            })),
            Message::Assistant { blocks } => {
                let mut content_parts = Vec::new();
                for b in blocks {
                    match b {
                        AssistantBlock::Text(t) => {
                            content_parts.push(json!({"type":"output_text","text":t}))
                        }
                        AssistantBlock::ToolUse {
                            call_id,
                            name,
                            args,
                        } => out.push(json!({
                            "type":"function_call","call_id":call_id,"name":name,"arguments":args
                        })),
                        AssistantBlock::Diff { .. }
                        | AssistantBlock::Thinking(_)
                        | AssistantBlock::Plan { .. } => {}
                    }
                }
                if !content_parts.is_empty() {
                    out.push(json!({"type":"message","role":"assistant","content":content_parts}));
                }
            }
            Message::ToolResult {
                call_id,
                content,
                is_error,
            } => out.push(json!({
                "type":"function_call_output","call_id":call_id,
                "output": if *is_error { format!("ERROR: {content}") } else { content.clone() }
            })),
        }
    }
    out
}

fn tools_to_responses_tools(tools: &[ToolDef]) -> Vec<Value> {
    tools_to_function_specs(tools)
        .into_iter()
        .map(|spec| {
            json!({
                "type":"function",
                "name": spec["function"]["name"],
                "description": spec["function"]["description"],
                "parameters": spec["function"]["parameters"],
            })
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ResponsesEvent {
    #[serde(rename = "response.output_text.delta")]
    TextDelta { delta: String },
    #[serde(rename = "response.output_item.added")]
    ItemAdded { item: ItemAdded },
    #[serde(rename = "response.function_call_arguments.delta")]
    ArgsDelta { item_id: String, delta: String },
    #[serde(rename = "response.output_item.done")]
    ItemDone {
        #[serde(default)]
        item: ItemDone,
    },
    #[serde(rename = "response.completed")]
    Completed {
        #[serde(default)]
        response: CompletedResponse,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct ItemAdded {
    #[allow(dead_code)]
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize, Default)]
struct ItemDone {
    #[allow(dead_code)]
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    id: String,
}

#[derive(Deserialize, Default)]
struct CompletedResponse {
    #[serde(default)]
    stop_reason: String,
}

pub fn parse_responses_sse(frame: &str) -> Option<StreamEvent> {
    let data = frame.lines().find_map(|line| line.strip_prefix("data: "))?;
    let evt: ResponsesEvent = serde_json::from_str(data).ok()?;
    match evt {
        ResponsesEvent::TextDelta { delta } => Some(StreamEvent::TextDelta(delta)),
        ResponsesEvent::ItemAdded { item } if item.kind == "function_call" => {
            Some(StreamEvent::ToolUseStart {
                call_id: item.id,
                name: item.name,
            })
        }
        ResponsesEvent::ArgsDelta { item_id, delta } => Some(StreamEvent::ToolUseArgsDelta {
            call_id: item_id,
            json_chunk: delta,
        }),
        ResponsesEvent::ItemDone { item } if item.kind == "function_call" => {
            Some(StreamEvent::ToolUseEnd { call_id: item.id })
        }
        ResponsesEvent::Completed { response } => {
            let reason = match response.stop_reason.as_str() {
                "tool_use" => StopReason::ToolUse,
                "length" => StopReason::MaxTokens,
                _ => StopReason::EndTurn,
            };
            Some(StreamEvent::StopTurn { reason })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = include_str!("../../tests/fixtures/openai/text.sse");
    const TOOLS: &str = include_str!("../../tests/fixtures/openai/tools.sse");

    fn frames(raw: &str) -> Vec<&str> {
        raw.split("\n\n").filter(|s| !s.trim().is_empty()).collect()
    }

    #[test]
    fn parses_text_then_completed_end_turn() {
        let events: Vec<StreamEvent> = frames(TEXT)
            .into_iter()
            .filter_map(parse_responses_sse)
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
    fn parses_tool_call_start_args_end_completed_tool_use() {
        let events: Vec<StreamEvent> = frames(TOOLS)
            .into_iter()
            .filter_map(parse_responses_sse)
            .collect();
        let has_start = events.iter().any(|e| {
            matches!(e, StreamEvent::ToolUseStart{call_id, name} if call_id == "call_1" && name == "list_spaces")
        });
        let has_args = events.iter().any(|e| {
            matches!(e, StreamEvent::ToolUseArgsDelta{json_chunk, ..} if json_chunk == "{\"filter\":\"all\"}")
        });
        let has_end = events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUseEnd { call_id } if call_id == "call_1"));
        let has_stop = events.iter().any(|e| {
            matches!(
                e,
                StreamEvent::StopTurn {
                    reason: StopReason::ToolUse
                }
            )
        });
        assert!(has_start && has_args && has_end && has_stop, "{events:?}");
    }

    #[test]
    fn build_request_uses_responses_endpoint_and_bearer_auth() {
        let msgs = vec![Message::User { text: "hi".into() }];
        let req = build_request("gpt-5", &msgs, &[], "test-key");
        assert_eq!(req.url().as_str(), ENDPOINT);
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Bearer test-key"
        );
        let body: serde_json::Value =
            serde_json::from_slice(req.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(body["model"], "gpt-5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["input"][0]["type"], "message");
    }
}
