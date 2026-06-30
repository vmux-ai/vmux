//! Capture the user's login-shell environment so agent processes inherit it.
//!
//! A vmux terminal runs the user's shell, which sources its config (`env.nu`,
//! `.zshrc`, …) and thus has the user's exported vars (API keys, etc.). An
//! *agent* (vibe/claude/codex) is launched as a bare executable, so it only
//! inherits the daemon's environment — which is missing those vars when the
//! daemon was started by launchd rather than from a shell. We capture the login
//! shell's exported environment once and merge it into agent spawns.
//!
//! The capture runs the shell **under a pty**, in login + interactive mode, and
//! reads the environment back between two sentinels. The pty matters: real shell
//! configs routinely call commands that need a controlling terminal (e.g.
//! `$env.GPG_TTY = (tty)` in `env.nu`, `tput`/`[[ -t 0 ]]` guards in `.zshrc`).
//! Without a pty those abort or skip, dropping every export that follows. Each
//! shell sources its own config files, so this works for zsh, bash, fish, and
//! nushell alike.

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc;
use std::time::Duration;

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};

/// Markers the capture command prints around `/usr/bin/env` output so we can
/// recover the environment from a pty stream that also carries shell banners,
/// prompts, and control sequences. The random suffix avoids colliding with a
/// real environment value.
const ENV_BEGIN: &str = "__VMUX_LOGIN_ENV_BEGIN_7Qz9__";
const ENV_END: &str = "__VMUX_LOGIN_ENV_END_7Qz9__";

/// How long to wait for the login shell to source its config and dump the
/// environment before giving up (a misbehaving config could block forever).
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

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

/// Run the login shell under a pty so it sources its config (where exports
/// live), then dump the environment it hands to child processes. Returns empty
/// on any failure (callers keep their env).
fn capture_login_shell_env(shell: &str) -> Vec<(String, String)> {
    capture_via_pty(shell).unwrap_or_default()
}

/// Spawn `shell` on a pty with the per-shell capture arguments, read its output
/// until EOF (or the timeout elapses), and parse the environment dumped between
/// the sentinels. `None` on any spawn/read failure or timeout.
fn capture_via_pty(shell: &str) -> Option<Vec<(String, String)>> {
    let args = shell_capture_args(shell);

    let pty_system = NativePtySystem::default();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .ok()?;

    let mut cmd = CommandBuilder::new(shell);
    cmd.args(&args);
    for (key, value) in std::env::vars() {
        cmd.env(key, value);
    }
    cmd.env("TERM", "xterm-256color");
    if let Some(home) = std::env::var_os("HOME") {
        cmd.cwd(home);
    }

    let mut child = pair.slave.spawn_command(cmd).ok()?;
    let reader = pair.master.try_clone_reader().ok()?;
    drop(pair.slave);

    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("login-shell-env-capture".to_string())
        .spawn(move || {
            let mut reader = reader;
            let mut buf = Vec::new();
            let _ = reader.read_to_end(&mut buf);
            let _ = tx.send(buf);
        })
        .ok()?;

    let bytes = match rx.recv_timeout(CAPTURE_TIMEOUT) {
        Ok(buf) => buf,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };
    let _ = child.wait();

    Some(extract_env_between_sentinels(&bytes))
}

/// Per-shell arguments that source the shell's config and print the environment
/// fenced by [`ENV_BEGIN`]/[`ENV_END`]. nushell does not load `env.nu`/`config.nu`
/// with a bare `-c`, so it is pointed at them explicitly (`-l` if the paths can't
/// be resolved); POSIX-style shells and fish need `-l -i` to source their login +
/// interactive config.
fn shell_capture_args(shell: &str) -> Vec<String> {
    let base = Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(shell);
    match base {
        "nu" | "nushell" => {
            let command = format!("print '{ENV_BEGIN}'; /usr/bin/env; print '{ENV_END}'");
            match resolve_nu_config_paths(shell) {
                Some((env_path, config_path)) => vec![
                    "--env-config".to_string(),
                    env_path,
                    "--config".to_string(),
                    config_path,
                    "-c".to_string(),
                    command,
                ],
                None => vec!["-l".to_string(), "-c".to_string(), command],
            }
        }
        _ => {
            let command =
                format!("printf '%s\\n' '{ENV_BEGIN}'; /usr/bin/env; printf '%s\\n' '{ENV_END}'");
            vec![
                "-l".to_string(),
                "-i".to_string(),
                "-c".to_string(),
                command,
            ]
        }
    }
}

/// Resolve nushell's active `env.nu` and `config.nu` paths via `$nu.env-path` /
/// `$nu.config-path` (parse-time constants, so a bare `-c` suffices and sources
/// nothing). `None` if either path can't be read.
fn resolve_nu_config_paths(shell: &str) -> Option<(String, String)> {
    let output = Command::new(shell)
        .args(["-c", "[$nu.env-path $nu.config-path] | to text"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let env_path = lines.next()?.to_string();
    let config_path = lines.next()?.to_string();
    Some((env_path, config_path))
}

/// Collect the `KEY=VALUE` lines that appear between [`ENV_BEGIN`] and
/// [`ENV_END`] in pty output, ignoring banners/prompts/control sequences outside
/// the fence. Marker detection is substring-based so a prompt printed on the
/// same line as the marker doesn't hide it.
fn extract_env_between_sentinels(bytes: &[u8]) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(bytes);
    let mut started = false;
    let mut body = String::new();
    for line in text.lines() {
        if !started {
            if line.contains(ENV_BEGIN) {
                started = true;
            }
            continue;
        }
        if line.contains(ENV_END) {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    parse_env(body.as_bytes())
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

    #[test]
    fn extract_env_ignores_noise_outside_sentinels() {
        let raw = format!(
            "Welcome banner\r\nuser@host prompt $\r\n{ENV_BEGIN}\r\nPATH=/usr/bin:/bin\r\nFOO=bar\r\n{ENV_END}\r\nexit noise\r\n"
        );
        assert_eq!(
            extract_env_between_sentinels(raw.as_bytes()),
            vec![
                ("PATH".to_string(), "/usr/bin:/bin".to_string()),
                ("FOO".to_string(), "bar".to_string()),
            ]
        );
    }

    #[test]
    fn extract_env_finds_marker_with_prompt_prefix() {
        // An interactive shell may print a prompt on the same line as the marker.
        let raw = format!("host% {ENV_BEGIN}\nKEY=val\n{ENV_END}\n");
        assert_eq!(
            extract_env_between_sentinels(raw.as_bytes()),
            vec![("KEY".to_string(), "val".to_string())]
        );
    }

    #[test]
    fn extract_env_without_markers_is_empty() {
        let raw = b"PATH=/usr/bin\nFOO=bar\n";
        assert!(extract_env_between_sentinels(raw).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn pty_capture_reads_env_from_login_shell() {
        let shell = ["/bin/zsh", "/bin/bash", "/bin/sh"]
            .into_iter()
            .find(|path| Path::new(path).exists())
            .expect("a POSIX login shell should exist on unix");
        let env = capture_login_shell_env(shell);
        assert!(
            env.iter().any(|(key, _)| key == "PATH"),
            "expected PATH in env captured from {shell}, got {} vars",
            env.len()
        );
    }
}
