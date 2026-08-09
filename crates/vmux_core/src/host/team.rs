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

#[derive(Component, Clone, Copy, Debug)]
pub struct Tester;

#[derive(Component, Clone, Debug)]
pub struct Agent {
    pub sid: String,
    pub kind: Option<AgentKind>,
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

    /// Avatar for a registry-driven ACP agent: initials from the display name, a stable
    /// brand color hashed from the registry id (so each agent reads distinctly).
    pub fn for_registry(name: &str, seed: &str) -> Self {
        Self {
            initials: initials_of(name),
            color: hash_color(seed),
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

/// A stable brand color for a seed string (e.g. an ACP registry id), picked from a fixed
/// palette by an FNV-1a hash. Deterministic and wasm-safe.
pub fn hash_color(seed: &str) -> String {
    const PALETTE: [&str; 8] = [
        "#ef4444", "#f97316", "#eab308", "#22c55e", "#14b8a6", "#3b82f6", "#8b5cf6", "#ec4899",
    ];
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in seed.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    PALETTE[(hash % PALETTE.len() as u64) as usize].to_string()
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

    pub fn registry(name: &str, seed: &str) -> Self {
        Self {
            name: name.to_string(),
            avatar: AvatarSpec::for_registry(name, seed),
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
