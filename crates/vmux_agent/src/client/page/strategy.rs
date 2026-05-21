use crate::client::page::strategy_components::ParseSse;
use crate::message::Message;
use crate::strategy::AgentStrategy;
use crate::stream::{StreamEvent, ToolDef};

pub trait AgentPageStrategy: AgentStrategy {
    fn provider(&self) -> &str;
    fn model(&self) -> &str;
    fn endpoint(&self) -> &str;
    fn env_var(&self) -> &'static str;

    fn build_request(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolDef],
        api_key: &str,
    ) -> reqwest::Request;

    fn parse_sse_event(&self, payload: &str) -> Option<StreamEvent>;

    fn parse_sse_fn(&self) -> ParseSse;
}
