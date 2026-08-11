use bevy::prelude::*;

use crate::agent::AgentKind;
pub use vmux_wire::avatar::{AvatarSpec, hash_color, initials_of};

#[derive(Component, Clone, Debug)]
pub struct Profile {
    pub name: String,
    pub avatar: AvatarSpec,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct User;

#[derive(Component, Clone, Copy, Debug)]
pub struct Tester;

#[derive(Component, Clone, Debug)]
pub struct Agent {
    pub sid: String,
    pub kind: Option<AgentKind>,
}

impl Profile {
    pub fn user() -> Self {
        Self {
            name: "You".into(),
            avatar: AvatarSpec::for_user(),
        }
    }

    pub fn user_named(name: String) -> Self {
        let avatar = AvatarSpec::for_user_named(&name);
        Self { name, avatar }
    }

    pub fn agent(kind: AgentKind) -> Self {
        Self {
            name: kind.display_name().to_string(),
            avatar: kind.avatar(),
        }
    }

    pub fn registry(name: &str, seed: &str) -> Self {
        Self {
            name: name.to_string(),
            avatar: AvatarSpec::for_registry(name, seed),
        }
    }
}

impl AgentKind {
    /// How this agent is drawn without a picture. Hangs off the kind rather than AvatarSpec
    /// because that type lives in vmux_wire now — the phone needs the same colours and cannot
    /// take a dependency on this crate.
    pub fn avatar(self) -> AvatarSpec {
        AvatarSpec {
            initials: match self {
                AgentKind::Claude => "CL",
                AgentKind::Codex => "CX",
                AgentKind::Vibe => "VB",
            }
            .into(),
            color: vmux_wire::avatar::agent_color(self.as_url_segment()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_avatar_is_kind_specific() {
        assert_eq!(AgentKind::Claude.avatar().initials, "CL");
        assert_ne!(
            AgentKind::Codex.avatar().color,
            AgentKind::Vibe.avatar().color
        );
    }

    #[test]
    fn agent_profile_name_is_display_name() {
        assert_eq!(Profile::agent(AgentKind::Claude).name, "Claude");
        assert_eq!(Profile::user().name, "You");
    }

    #[test]
    fn registry_avatar_derives_initials_and_stable_color() {
        let a = AvatarSpec::for_registry("Mistral Vibe", "mistral-vibe");
        assert_eq!(a.initials, "MV");
        // Deterministic: same seed -> same color.
        assert_eq!(a.color, AvatarSpec::for_registry("X", "mistral-vibe").color);
        // Valid 7-char hex.
        assert!(a.color.starts_with('#') && a.color.len() == 7);
    }

    #[test]
    fn registry_color_differs_by_seed() {
        assert_ne!(
            AvatarSpec::for_registry("A", "claude-acp").color,
            AvatarSpec::for_registry("A", "mistral-vibe").color
        );
    }

    #[test]
    fn registry_profile_uses_name() {
        assert_eq!(
            Profile::registry("Claude Agent", "claude-acp").name,
            "Claude Agent"
        );
    }
}
