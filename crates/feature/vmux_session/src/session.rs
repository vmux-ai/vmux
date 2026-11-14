use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct AgentConversationTitle(pub String);

impl AgentConversationTitle {
    pub fn from_prompt(text: &str) -> Option<Self> {
        let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.is_empty() {
            return None;
        }
        if normalized.chars().count() <= 120 {
            return Some(Self(normalized));
        }
        let mut title = normalized.chars().take(119).collect::<String>();
        let trimmed_len = title.trim_end().len();
        title.truncate(trimmed_len);
        title.push('…');
        Some(Self(title))
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TranscriptLoading;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct TitlePending;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApprovalCount(pub u32);

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ApprovalProjectionSet;
