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

#[test]
fn requires_homebrew_only_for_cask_agents() {
    assert!(requires_homebrew("claude"));
    assert!(requires_homebrew("codex"));
    assert!(!requires_homebrew("vibe"));
    assert!(!requires_homebrew("nope"));
}

#[test]
fn chained_command_prepends_homebrew_when_absent() {
    assert_eq!(
        install_command_chained("claude", false).as_deref(),
        Some(
            "bash -c '/bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\" && eval \"$(/opt/homebrew/bin/brew shellenv 2>/dev/null || /usr/local/bin/brew shellenv)\" && brew install --cask claude-code'"
        )
    );
    assert_eq!(
        install_command_chained("codex", false).as_deref(),
        Some(
            "bash -c '/bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\" && eval \"$(/opt/homebrew/bin/brew shellenv 2>/dev/null || /usr/local/bin/brew shellenv)\" && brew install --cask codex'"
        )
    );
}

#[test]
fn chained_command_plain_when_brew_present() {
    assert_eq!(
        install_command_chained("claude", true).as_deref(),
        Some("brew install --cask claude-code")
    );
}

#[test]
fn chained_command_never_wraps_vibe() {
    let absent = install_command_chained("vibe", false);
    let present = install_command_chained("vibe", true);
    assert_eq!(absent, present);
    assert_eq!(
        absent.as_deref(),
        Some("curl -LsSf https://mistral.ai/vibe/install.sh | bash")
    );
}

#[test]
fn chained_command_unknown_is_none() {
    assert_eq!(install_command_chained("nope", false), None);
    assert_eq!(install_command_chained("nope", true), None);
}
