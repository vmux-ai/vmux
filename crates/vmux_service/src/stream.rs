use serde::{Deserialize, Serialize};

use crate::message::Message;

pub type BuildRequest =
    fn(model: &str, messages: &[Message], tools: &[ToolDef], api_key: &str) -> reqwest::Request;

pub type ParseSse = fn(payload: &str) -> Option<StreamEvent>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum StreamEvent {
    TextDelta(String),
    ToolUseStart { call_id: String, name: String },
    ToolUseArgsDelta { call_id: String, json_chunk: String },
    ToolUseEnd { call_id: String },
    StopTurn { reason: StopReason },
    Error(String),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub read_only: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PartialToolUse {
    pub call_id: String,
    pub name: String,
    pub args_buf: String,
}

#[cfg(test)]
#[path = "stream.test.rs"]
mod tests;
