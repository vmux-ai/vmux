pub fn display_name(segment: &str) -> Option<&'static str> {
    match segment {
        "vibe" => Some("Vibe"),
        "claude" => Some("Claude"),
        "codex" => Some("Codex"),
        _ => None,
    }
}

pub fn install_command(segment: &str) -> Option<&'static str> {
    match segment {
        "vibe" => Some("curl -LsSf https://mistral.ai/vibe/install.sh | bash"),
        "claude" => Some("brew install --cask claude-code"),
        "codex" => Some("brew install --cask codex"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_segments_resolve() {
        for segment in ["vibe", "claude", "codex"] {
            assert!(display_name(segment).is_some(), "display_name {segment}");
            assert!(
                install_command(segment).is_some(),
                "install_command {segment}"
            );
        }
        assert_eq!(
            install_command("vibe"),
            Some("curl -LsSf https://mistral.ai/vibe/install.sh | bash")
        );
        assert_eq!(
            install_command("claude"),
            Some("brew install --cask claude-code")
        );
        assert_eq!(install_command("codex"), Some("brew install --cask codex"));
    }

    #[test]
    fn unknown_segment_is_none() {
        assert_eq!(display_name("nope"), None);
        assert_eq!(install_command("nope"), None);
    }
}
