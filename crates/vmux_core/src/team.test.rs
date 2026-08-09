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
