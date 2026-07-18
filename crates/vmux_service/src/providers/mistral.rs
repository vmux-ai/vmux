use serde_json::json;

use crate::message::Message;
use crate::providers::openai_shared::{
    messages_to_chat_completions, parse_chat_completions_sse, tools_to_function_specs,
};
use crate::stream::{StreamEvent, ToolDef};

pub const PROVIDER: &str = "mistral";
pub const ENDPOINT: &str = "https://api.mistral.ai/v1/chat/completions";
pub const ENV_VAR: &str = "MISTRAL_API_KEY";
pub const DEFAULT_MODEL: &str = "devstral-2";

pub fn build_request(
    model: &str,
    messages: &[Message],
    tools: &[ToolDef],
    api_key: &str,
) -> reqwest::Request {
    let mut body = json!({
        "model": model,
        "messages": messages_to_chat_completions(messages),
        "stream": true,
    });
    if !tools.is_empty() {
        body["tools"] = json!(tools_to_function_specs(tools));
        body["tool_choice"] = json!("auto");
    }
    reqwest::Client::new()
        .post(ENDPOINT)
        .bearer_auth(api_key)
        .header("Accept", "text/event-stream")
        .header("Content-Type", "application/json")
        .json(&body)
        .build()
        .expect("mistral: build_request")
}

pub fn parse_sse(payload: &str) -> Option<StreamEvent> {
    parse_chat_completions_sse(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_sets_headers_and_url() {
        let msgs = vec![Message::user("hi")];
        let req = build_request("devstral-2", &msgs, &[], "test-key");
        assert_eq!(req.url().as_str(), ENDPOINT);
        let auth = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert_eq!(auth, "Bearer test-key");
        let body = req.body().unwrap().as_bytes().unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(body).unwrap();
        assert_eq!(parsed["model"], "devstral-2");
        assert_eq!(parsed["stream"], true);
        assert_eq!(parsed["messages"][0]["role"], "user");
    }

    #[test]
    fn parse_sse_event_delegates_to_shared_parser() {
        let frame = r#"data: {"id":"c1","choices":[{"index":0,"delta":{"content":"hi"},"finish_reason":null}]}"#;
        assert_eq!(parse_sse(frame), Some(StreamEvent::TextDelta("hi".into())));
    }
}
