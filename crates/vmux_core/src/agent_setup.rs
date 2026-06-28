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

/// True for agents installed via Homebrew casks (`claude`, `codex`).
pub fn requires_homebrew(segment: &str) -> bool {
    matches!(segment, "claude" | "codex")
}

/// The official Homebrew installer one-liner, run non-interactively.
///
/// `NONINTERACTIVE=1` skips the installer's "Press RETURN" prompt; `sudo` still
/// prompts for a password once on a fresh machine.
pub fn homebrew_install_command() -> &'static str {
    "NONINTERACTIVE=1 /bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
}

/// The command vmux runs in the terminal to install `segment`.
///
/// When the agent needs Homebrew (`claude`/`codex`) and it is absent
/// (`brew_present == false`), the command first installs Homebrew, loads it onto
/// `PATH` for the session, then installs the agent — wrapped in `bash -c '…'` so
/// it runs verbatim under nushell, zsh, or bash. Otherwise the plain per-agent
/// command is returned unchanged. Returns `None` for unknown segments.
pub fn install_command_chained(segment: &str, brew_present: bool) -> Option<String> {
    let base = install_command(segment)?;
    if requires_homebrew(segment) && !brew_present {
        Some(format!(
            "bash -c '{} && eval \"$(/opt/homebrew/bin/brew shellenv 2>/dev/null || /usr/local/bin/brew shellenv)\" && {base}'",
            homebrew_install_command()
        ))
    } else {
        Some(base.to_string())
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
                "bash -c 'NONINTERACTIVE=1 /bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\" && eval \"$(/opt/homebrew/bin/brew shellenv 2>/dev/null || /usr/local/bin/brew shellenv)\" && brew install --cask claude-code'"
            )
        );
        assert_eq!(
            install_command_chained("codex", false).as_deref(),
            Some(
                "bash -c 'NONINTERACTIVE=1 /bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\" && eval \"$(/opt/homebrew/bin/brew shellenv 2>/dev/null || /usr/local/bin/brew shellenv)\" && brew install --cask codex'"
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
}
