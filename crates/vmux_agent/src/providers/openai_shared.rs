use serde::Deserialize;
use serde_json::{Value, json};

use crate::message::{AssistantBlock, Message};
use crate::stream::{StopReason, StreamEvent, ToolDef};

#[derive(Deserialize)]
struct ChunkRoot<'a> {
    #[serde(borrow)]
    choices: Vec<Choice<'a>>,
}

#[derive(Deserialize)]
struct Choice<'a> {
    #[serde(borrow, default)]
    delta: Delta<'a>,
    #[serde(borrow, default)]
    finish_reason: Option<&'a str>,
}

#[derive(Deserialize, Default)]
struct Delta<'a> {
    #[serde(borrow, default)]
    content: Option<&'a str>,
    #[serde(borrow, default)]
    tool_calls: Option<Vec<ToolCallDelta<'a>>>,
}

#[derive(Deserialize)]
struct ToolCallDelta<'a> {
    #[serde(rename = "index", default)]
    _index: usize,
    #[serde(borrow, default)]
    id: Option<&'a str>,
    #[serde(borrow, default)]
    function: Option<FunctionDelta<'a>>,
}

#[derive(Deserialize)]
struct FunctionDelta<'a> {
    #[serde(borrow, default)]
    name: Option<&'a str>,
    #[serde(default)]
    arguments: Option<String>,
}

pub fn parse_chat_completions_sse(frame: &str) -> Option<StreamEvent> {
    let payload = frame.lines().find_map(|line| line.strip_prefix("data: "))?;
    if payload.trim() == "[DONE]" {
        return None;
    }
    let chunk: ChunkRoot = serde_json::from_str(payload).ok()?;
    let choice = chunk.choices.into_iter().next()?;
    if let Some(reason) = choice.finish_reason {
        return Some(StreamEvent::StopTurn {
            reason: match reason {
                "stop" => StopReason::EndTurn,
                "tool_calls" => StopReason::ToolUse,
                "length" => StopReason::MaxTokens,
                _ => StopReason::Other,
            },
        });
    }
    if let Some(text) = choice.delta.content
        && !text.is_empty()
    {
        return Some(StreamEvent::TextDelta(text.to_string()));
    }
    if let Some(calls) = choice.delta.tool_calls {
        let call = calls.into_iter().next()?;
        if let Some(id) = call.id {
            let name = call.function.and_then(|f| f.name).unwrap_or("").to_string();
            return Some(StreamEvent::ToolUseStart {
                call_id: id.to_string(),
                name,
            });
        }
        if let Some(args) = call.function.and_then(|f| f.arguments) {
            return Some(StreamEvent::ToolUseArgsDelta {
                call_id: String::new(),
                json_chunk: args,
            });
        }
    }
    None
}

pub fn messages_to_chat_completions(messages: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();
    for msg in messages {
        match msg {
            Message::User { text } => out.push(json!({"role":"user","content":text})),
            Message::Assistant { blocks } => {
                let mut content = String::new();
                let mut tool_calls = Vec::new();
                for b in blocks {
                    match b {
                        AssistantBlock::Text(t) => content.push_str(t),
                        AssistantBlock::ToolUse {
                            call_id,
                            name,
                            args,
                        } => tool_calls.push(json!({
                            "id": call_id,
                            "type":"function",
                            "function": {"name": name, "arguments": args}
                        })),
                    }
                }
                let mut obj = json!({"role":"assistant","content": content});
                if !tool_calls.is_empty() {
                    obj["tool_calls"] = json!(tool_calls);
                }
                out.push(obj);
            }
            Message::ToolResult {
                call_id,
                content,
                is_error,
            } => out.push(json!({
                "role":"tool",
                "tool_call_id": call_id,
                "content": if *is_error { format!("ERROR: {content}") } else { content.clone() }
            })),
        }
    }
    out
}

pub fn tools_to_function_specs(tools: &[ToolDef]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MISTRAL_TEXT: &str = include_str!("../../tests/fixtures/mistral/text.sse");
    const MISTRAL_TOOLS: &str = include_str!("../../tests/fixtures/mistral/tools.sse");

    fn frames(raw: &str) -> Vec<&str> {
        raw.split("\n\n").filter(|s| !s.trim().is_empty()).collect()
    }

    #[test]
    fn parses_text_then_stop() {
        let events: Vec<StreamEvent> = frames(MISTRAL_TEXT)
            .into_iter()
            .filter_map(parse_chat_completions_sse)
            .collect();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], StreamEvent::TextDelta("hello".into()));
        assert_eq!(events[1], StreamEvent::TextDelta(" world".into()));
        assert!(matches!(
            events[2],
            StreamEvent::StopTurn {
                reason: StopReason::EndTurn
            }
        ));
    }

    #[test]
    fn parses_tool_call_sequence() {
        let events: Vec<StreamEvent> = frames(MISTRAL_TOOLS)
            .into_iter()
            .filter_map(parse_chat_completions_sse)
            .collect();
        match &events[0] {
            StreamEvent::ToolUseStart { call_id, name } => {
                assert_eq!(call_id, "call_1");
                assert_eq!(name, "list_spaces");
            }
            other => panic!("expected ToolUseStart, got {other:?}"),
        }
        match &events[1] {
            StreamEvent::ToolUseArgsDelta { json_chunk, .. } => {
                assert_eq!(json_chunk, "{\"filter\":\"all\"}");
            }
            other => panic!("expected ToolUseArgsDelta, got {other:?}"),
        }
        assert!(matches!(
            events[2],
            StreamEvent::StopTurn {
                reason: StopReason::ToolUse
            }
        ));
    }

    #[test]
    fn messages_to_chat_completions_roundtrip() {
        let msgs = vec![
            Message::User { text: "hi".into() },
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("hello".into())],
            },
            Message::ToolResult {
                call_id: "c1".into(),
                content: "ok".into(),
                is_error: false,
            },
        ];
        let out = messages_to_chat_completions(&msgs);
        assert_eq!(out[0]["role"], "user");
        assert_eq!(out[1]["role"], "assistant");
        assert_eq!(out[2]["role"], "tool");
        assert_eq!(out[2]["tool_call_id"], "c1");
    }

    #[test]
    fn tools_to_function_specs_shape() {
        let tools = vec![ToolDef {
            name: "list_spaces",
            description: "desc",
            input_schema: json!({"type":"object"}),
            read_only: true,
        }];
        let out = tools_to_function_specs(&tools);
        assert_eq!(out[0]["type"], "function");
        assert_eq!(out[0]["function"]["name"], "list_spaces");
    }
}
