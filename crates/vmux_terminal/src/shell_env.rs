//! Capture the user's login-shell environment so agent processes inherit it.
//!
//! A vmux terminal runs the user's shell, which sources its config (`env.nu`,
//! `.zshrc`, …) and thus has the user's exported vars (API keys, etc.). An
//! *agent* (vibe/claude/codex) is launched as a bare executable, so it only
//! inherits the daemon's environment — which is missing those vars when the
//! daemon was started by launchd rather than from a shell. We capture the login
//! shell's exported environment once and merge it into agent spawns.

use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// The environment the user's login shell exports, captured once per process.
pub fn login_shell_env(shell: &str) -> &'static [(String, String)] {
    static CACHE: OnceLock<Vec<(String, String)>> = OnceLock::new();
    CACHE
        .get_or_init(|| capture_login_shell_env(shell))
        .as_slice()
}

/// Merge the login-shell env into `env` (shell values win), then drop duplicate
/// keys keeping the last. Use when spawning an agent so it gets the same
/// environment a terminal would, regardless of how the daemon was launched.
pub fn merge_login_shell_env(env: &mut Vec<(String, String)>, shell: &str) {
    env.extend(login_shell_env(shell).iter().cloned());
    dedup_env_keep_last(env);
}

/// Run the login shell so it sources its config (where exports live), then dump
/// the environment it hands to child processes. Shell-specific flags: nushell
/// auto-loads `env.nu` with a plain `-c` (and `-l -i` can suppress it), while
/// POSIX-style shells (bash/zsh) and fish need `-l -i` to source their login +
/// interactive config. Returns empty on any failure (callers keep their env).
fn capture_login_shell_env(shell: &str) -> Vec<(String, String)> {
    let base = Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(shell);
    let args: &[&str] = match base {
        "nu" | "nushell" => &["-c", "/usr/bin/env"],
        _ => &["-l", "-i", "-c", "/usr/bin/env"],
    };
    let Ok(output) = Command::new(shell)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_env(&output.stdout)
}

/// Parse `KEY=VALUE` lines from `env` output. Splits on the first `=` and skips
/// lines without one (e.g. wrapped multi-line values).
fn parse_env(bytes: &[u8]) -> Vec<(String, String)> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(key, _)| !key.is_empty())
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// Keep only the last occurrence of each key, preserving order.
fn dedup_env_keep_last(env: &mut Vec<(String, String)>) {
    let mut seen = HashSet::new();
    let mut deduped = Vec::with_capacity(env.len());
    for (key, value) in std::mem::take(env).into_iter().rev() {
        if seen.insert(key.clone()) {
            deduped.push((key, value));
        }
    }
    deduped.reverse();
    *env = deduped;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_env_splits_on_first_equals() {
        let out = b"PATH=/usr/bin:/bin\nKEY=a=b=c\nEMPTY=\nnovalue\n=novalue\n";
        assert_eq!(
            parse_env(out),
            vec![
                ("PATH".to_string(), "/usr/bin:/bin".to_string()),
                ("KEY".to_string(), "a=b=c".to_string()),
                ("EMPTY".to_string(), String::new()),
            ]
        );
    }

    #[test]
    fn merge_overrides_existing_keys_keeping_order() {
        // Simulate a base env (daemon) merged with a login env via dedup.
        let mut env = vec![
            ("VIBE_MCP_SERVERS".to_string(), "[...]".to_string()),
            ("ANTHROPIC_FOUNDRY_API_KEY".to_string(), "stale".to_string()),
            // login env appended (would come from login_shell_env):
            ("ANTHROPIC_FOUNDRY_API_KEY".to_string(), "fresh".to_string()),
            ("PATH".to_string(), "/login/bin".to_string()),
        ];
        dedup_env_keep_last(&mut env);
        assert_eq!(
            env,
            vec![
                ("VIBE_MCP_SERVERS".to_string(), "[...]".to_string()),
                ("ANTHROPIC_FOUNDRY_API_KEY".to_string(), "fresh".to_string()),
                ("PATH".to_string(), "/login/bin".to_string()),
            ]
        );
    }
}
