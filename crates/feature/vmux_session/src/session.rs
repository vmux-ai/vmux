use std::collections::HashSet;

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

#[derive(Component, Clone, Debug, Default, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct ApprovalPolicy {
    pub auto: HashSet<String>,
}

impl ApprovalPolicy {
    pub fn allow(&mut self, tool: &str) {
        self.auto.insert(Self::tool_key(tool));
    }

    pub fn allows(&self, tool: &str) -> bool {
        self.auto.contains(&Self::tool_key(tool))
    }

    pub fn tool_key(tool: &str) -> String {
        tool.trim()
            .to_ascii_lowercase()
            .split(|character: char| {
                character.is_ascii_whitespace() || matches!(character, '-' | '.' | ':' | '_')
            })
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_policy_normalizes_agent_tool_identifiers() {
        let mut policy = ApprovalPolicy::default();
        policy.allow("mcp__vmux__run");

        assert!(policy.allows("mcp.vmux.run"));
        assert!(!policy.allows("mcp.other.run"));
    }
}
