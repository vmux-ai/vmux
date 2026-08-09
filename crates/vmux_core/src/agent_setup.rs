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

/// The official Homebrew installer one-liner.
///
/// Runs interactively in the terminal pane: Homebrew asks the user to press
/// Return, then `sudo` prompts for the password on the TTY. We deliberately do
/// not set `NONINTERACTIVE=1` — that mode refuses to prompt and aborts with
/// "Need sudo access" when credentials aren't already cached.
pub fn homebrew_install_command() -> &'static str {
    "/bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
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
#[path = "agent_setup.test.rs"]
mod tests;
