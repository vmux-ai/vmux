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
    #[allow(dead_code)]
    #[serde(default)]
    index: usize,
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
            // Empty call_id is intentional: chat-completions continuation chunks
            // only repeat `index`, not `id`. drain_stream correlates via the
            // in-progress PartialToolUse in AgentRunState::Streaming.
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
            Message::User { text, .. } => out.push(json!({"role":"user","content":text})),
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
                            ..
                        } => tool_calls.push(json!({
                            "id": call_id,
                            "type":"function",
                            "function": {"name": name, "arguments": args}
                        })),
                        AssistantBlock::Subagent(subagent) => tool_calls.push(json!({
                            "id": subagent.call_id,
                            "type":"function",
                            "function": {"name": "subagent", "arguments": subagent.raw_input}
                        })),
                        AssistantBlock::Diff { .. }
                        | AssistantBlock::Thinking(_)
                        | AssistantBlock::Plan { .. } => {}
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
#[path = "openai_shared.test.rs"]
mod tests;
