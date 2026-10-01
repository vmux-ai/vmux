use bevy::prelude::*;

pub use vmux_api::avatar::{AvatarSpec, hash_color, initials_of};

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
    fn user_profile_has_default_name() {
        assert_eq!(Profile::user().name, "You");
    }

    #[test]
    fn registry_avatar_derives_initials_and_stable_color() {
        let a = AvatarSpec::for_registry("Mistral Vibe", "mistral-vibe");
        assert_eq!(a.initials, "MV");
        assert_eq!(a.color, AvatarSpec::for_registry("X", "mistral-vibe").color);
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
