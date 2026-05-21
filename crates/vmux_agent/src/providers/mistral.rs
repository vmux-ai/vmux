use serde_json::json;

use crate::client::page::strategy::AgentPageStrategy;
use crate::message::Message;
use crate::providers::openai_shared::{
    messages_to_chat_completions, parse_chat_completions_sse, tools_to_function_specs,
};
use crate::strategy::AgentStrategy;
use crate::stream::{StreamEvent, ToolDef};
use crate::{AgentKind, AgentVariant};

pub struct MistralStrategy {
    provider: String,
    model: String,
}

impl MistralStrategy {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }
}

impl AgentStrategy for MistralStrategy {
    fn kind(&self) -> AgentKind {
        AgentKind::Vibe
    }
    fn variant(&self) -> AgentVariant {
        AgentVariant::Page
    }
}

impl AgentPageStrategy for MistralStrategy {
    fn provider(&self) -> &str {
        &self.provider
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn endpoint(&self) -> &str {
        "https://api.mistral.ai/v1/chat/completions"
    }
    fn env_var(&self) -> &'static str {
        "MISTRAL_API_KEY"
    }

    fn build_request(
        &self,
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
            .post(self.endpoint())
            .bearer_auth(api_key)
            .header("Accept", "text/event-stream")
            .header("Content-Type", "application/json")
            .json(&body)
            .build()
            .expect("MistralStrategy: build_request")
    }

    fn parse_sse_event(&self, payload: &str) -> Option<StreamEvent> {
        parse_chat_completions_sse(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_sets_headers_and_url() {
        let s = MistralStrategy::new("mistral", "devstral-2");
        let msgs = vec![Message::User { text: "hi".into() }];
        let req = s.build_request("devstral-2", &msgs, &[], "test-key");
        assert_eq!(req.url().as_str(), s.endpoint());
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
        let s = MistralStrategy::new("mistral", "devstral-2");
        let frame = r#"data: {"id":"c1","choices":[{"index":0,"delta":{"content":"hi"},"finish_reason":null}]}"#;
        assert_eq!(
            s.parse_sse_event(frame),
            Some(StreamEvent::TextDelta("hi".into()))
        );
    }
}
