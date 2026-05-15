use crate::message::Message;
use crate::strategy::AgentStrategy;
use crate::stream::{StreamEvent, ToolDef};

pub trait AppAgentStrategy: AgentStrategy {
    fn provider(&self) -> &'static str;
    fn model(&self) -> &'static str;
    fn models(&self) -> &'static [&'static str];
    fn default_model(&self) -> &'static str;
    fn endpoint(&self) -> &'static str;

    fn build_request(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolDef],
        api_key: &str,
    ) -> reqwest::Request;

    fn parse_sse_event(&self, payload: &str) -> Option<StreamEvent>;
}
