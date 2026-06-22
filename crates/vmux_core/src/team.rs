use bevy::prelude::*;

use crate::agent::AgentKind;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AvatarSpec {
    pub initials: String,
    pub color: String,
}

#[derive(Component, Clone, Debug)]
pub struct Profile {
    pub name: String,
    pub avatar: AvatarSpec,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct User;

#[derive(Component, Clone, Debug)]
pub struct Agent {
    pub sid: String,
    pub kind: AgentKind,
}

impl AvatarSpec {
    pub fn for_user() -> Self {
        Self {
            initials: "You".into(),
            color: "#3b82f6".into(),
        }
    }

    pub fn for_user_named(name: &str) -> Self {
        Self {
            initials: initials_of(name),
            color: "#3b82f6".into(),
        }
    }

    pub fn for_agent(kind: AgentKind) -> Self {
        let (initials, color) = match kind {
            AgentKind::Claude => ("CL", "#d97757"),
            AgentKind::Codex => ("CX", "#10a37f"),
            AgentKind::Vibe => ("VB", "#7c3aed"),
        };
        Self {
            initials: initials.into(),
            color: color.into(),
        }
    }
}

/// Up to two uppercase initials from a display name ("Junichi Sugiura" -> "JS").
pub fn initials_of(name: &str) -> String {
    let initials: String = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(2)
        .filter_map(|word| word.chars().next())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if initials.is_empty() {
        "?".to_string()
    } else {
        initials
    }
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
            avatar: AvatarSpec::for_agent(kind),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_avatar_is_kind_specific() {
        assert_eq!(AvatarSpec::for_agent(AgentKind::Claude).initials, "CL");
        assert_ne!(
            AvatarSpec::for_agent(AgentKind::Codex).color,
            AvatarSpec::for_agent(AgentKind::Vibe).color
        );
    }

    #[test]
    fn agent_profile_name_is_display_name() {
        assert_eq!(Profile::agent(AgentKind::Claude).name, "Claude");
        assert_eq!(Profile::user().name, "You");
    }
}
