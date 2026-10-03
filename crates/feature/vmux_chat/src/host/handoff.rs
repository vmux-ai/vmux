use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};
use vmux_api::conversation::Message;

#[derive(Component, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImportedConversation {
    pub source_agent: String,
    pub source_sid: String,
    pub messages: Vec<Message>,
    pub truncated: bool,
    pub first_prompt: Option<String>,
}
