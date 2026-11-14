use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc;
use std::time::Duration;

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};

const ENV_BEGIN: &str = "__VMUX_LOGIN_ENV_BEGIN_7Qz9__";
const ENV_END: &str = "__VMUX_LOGIN_ENV_END_7Qz9__";

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

static CACHE: OnceLock<Vec<(String, String)>> = OnceLock::new();

pub struct LoginShellEnvironment;

impl LoginShellEnvironment {
    pub fn get(shell: &str) -> &'static [(String, String)] {
        CACHE.get_or_init(|| Self::capture(shell)).as_slice()
    }

    pub fn prewarm(shell: String) {
        if CACHE.get().is_some() {
            return;
        }
        let _ = std::thread::Builder::new()
            .name("login-shell-env-prewarm".to_string())
            .spawn(move || {
                Self::get(&shell);
            });
    }

    pub fn merge(env: &mut Vec<(String, String)>, shell: &str) {
        env.extend(Self::get(shell).iter().cloned());
        Self::dedup(env);
    }

    pub fn apply_isolation(command: &mut Command) {
        let paths = vmux_profile::ProfilePaths::current();
        if paths.isolated() {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
            command.env_clear().envs(Self::get(&shell).iter().cloned());
        }
    }

    pub fn apply_pty_isolation(command: &mut CommandBuilder) {
        if vmux_profile::ProfilePaths::current().isolated() {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
            Self::isolate_pty(command, Self::get(&shell));
        }
    }

    fn isolate_pty(command: &mut CommandBuilder, environment: &[(String, String)]) {
        command.env_clear();
        for (key, value) in environment {
            command.env(key, value);
        }
    }

    fn capture(shell: &str) -> Vec<(String, String)> {
        let paths = vmux_profile::ProfilePaths::current();
        if paths.isolated() {
            return Self::isolated(&paths.tool_home(), std::env::vars());
        }
        Self::capture_via_pty(shell).unwrap_or_default()
    }

    fn isolated(
        home: &Path,
        inherited: impl IntoIterator<Item = (String, String)>,
    ) -> Vec<(String, String)> {
        let keys = std::env::var("VMUX_TEST_ENV_KEYS").ok().as_deref() == Some("1");
        let mut environment = Vec::new();
        let mut original_home = None;
        let mut rustup_home = None;
        for (key, value) in inherited {
            if key == "HOME" {
                original_home = Some(std::path::PathBuf::from(value));
                continue;
            }
            if key == "RUSTUP_HOME" {
                rustup_home = Some(value);
                continue;
            }
            if matches!(
                key.as_str(),
                "PATH"
                    | "TMPDIR"
                    | "LANG"
                    | "LC_ALL"
                    | "LC_CTYPE"
                    | "SHELL"
                    | "VMUX_DATA_DIR"
                    | "VMUX_PROFILE"
                    | "VMUX_TEST"
                    | "VMUX_TEST_ENV_KEYS"
            ) || (keys
                && matches!(
                    key.as_str(),
                    "ANTHROPIC_API_KEY" | "OPENAI_API_KEY" | "MISTRAL_API_KEY"
                ))
            {
                environment.push((key, value));
            }
        }
        if let Some(rustup_home) = rustup_home {
            environment.push(("RUSTUP_HOME".into(), rustup_home));
        } else if let Some(original_home) = original_home {
            environment.push((
                "RUSTUP_HOME".into(),
                original_home.join(".rustup").to_string_lossy().into_owned(),
            ));
        }
        environment.push((
            "CARGO_HOME".into(),
            home.join(".cargo").to_string_lossy().into_owned(),
        ));
        environment.push(("HOME".into(), home.to_string_lossy().into_owned()));
        environment.push((
            "XDG_CONFIG_HOME".into(),
            home.join(".config").to_string_lossy().into_owned(),
        ));
        environment.push((
            "XDG_CACHE_HOME".into(),
            home.join(".cache").to_string_lossy().into_owned(),
        ));
        environment.push((
            "XDG_DATA_HOME".into(),
            home.join(".local/share").to_string_lossy().into_owned(),
        ));
        environment.push((
            "XDG_STATE_HOME".into(),
            home.join(".local/state").to_string_lossy().into_owned(),
        ));
        environment
    }

    fn capture_via_pty(shell: &str) -> Option<Vec<(String, String)>> {
        let args = Self::capture_args(shell);

        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .ok()?;

        let reader = pair.master.try_clone_reader().ok()?;

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
        drop(pair.slave);

        let (tx, rx) = mpsc::channel();
        if std::thread::Builder::new()
            .name("login-shell-env-capture".to_string())
            .spawn(move || {
                let mut reader = reader;
                let mut buf = Vec::new();
                let _ = reader.read_to_end(&mut buf);
                let _ = tx.send(buf);
            })
            .is_err()
        {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }

        let bytes = match rx.recv_timeout(CAPTURE_TIMEOUT) {
            Ok(buf) => buf,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        };
        let _ = child.wait();

        Some(Self::extract(&bytes))
    }

    fn capture_args(shell: &str) -> Vec<String> {
        let base = Path::new(shell)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(shell);
        match base {
            "nu" | "nushell" => {
                let command = format!("print '{ENV_BEGIN}'; /usr/bin/env; print '{ENV_END}'");
                match Self::nu_config_paths(shell) {
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
                let command = format!(
                    "printf '%s\\n' '{ENV_BEGIN}'; /usr/bin/env; printf '%s\\n' '{ENV_END}'"
                );
                vec![
                    "-l".to_string(),
                    "-i".to_string(),
                    "-c".to_string(),
                    command,
                ]
            }
        }
    }

    fn nu_config_paths(shell: &str) -> Option<(String, String)> {
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

    fn extract(bytes: &[u8]) -> Vec<(String, String)> {
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
        Self::parse(body.as_bytes())
    }

    fn parse(bytes: &[u8]) -> Vec<(String, String)> {
        String::from_utf8_lossy(bytes)
            .lines()
            .filter_map(|line| line.split_once('='))
            .filter(|(key, _)| !key.is_empty())
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn dedup(env: &mut Vec<(String, String)>) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_agent_environment_replaces_dotfile_paths_and_omits_unrelated_credentials() {
        let fixture = std::path::Path::new("/fixture/home");
        let environment = LoginShellEnvironment::isolated(
            fixture,
            [
                ("HOME".into(), "/real/home".into()),
                ("XDG_CONFIG_HOME".into(), "/real/config".into()),
                ("CARGO_HOME".into(), "/real/cargo".into()),
                ("PATH".into(), "/usr/bin:/bin".into()),
                ("VMUX_DATA_DIR".into(), "/fixture/data".into()),
                ("VMUX_PROFILE".into(), "onboarding".into()),
                (
                    "AWS_SECRET_ACCESS_KEY".into(),
                    "must-not-be-inherited".into(),
                ),
            ],
        );
        let mut command = Command::new("/usr/bin/printenv");
        command.env_clear().envs(environment.clone());
        let output = command.output().unwrap();
        assert!(output.status.success());
        let output = String::from_utf8(output.stdout).unwrap();
        assert!(output.contains(&format!("HOME={}\n", fixture.display())));
        assert!(!output.contains("/real/config"));
        assert!(!output.contains("/real/cargo"));
        assert!(!output.contains("AWS_SECRET_ACCESS_KEY"));
        assert!(output.contains("PATH=/usr/bin:/bin\n"));
        assert!(output.contains("VMUX_DATA_DIR=/fixture/data\n"));
        assert!(output.contains("VMUX_PROFILE=onboarding\n"));
        assert!(output.contains("RUSTUP_HOME=/real/home/.rustup\n"));
        assert!(output.contains("CARGO_HOME=/fixture/home/.cargo\n"));
        let mut pty = CommandBuilder::new("/usr/bin/printenv");
        pty.cwd(std::env::temp_dir());
        pty.env("AWS_SECRET_ACCESS_KEY", "must-not-be-inherited");
        LoginShellEnvironment::isolate_pty(&mut pty, &environment);
        let pair = NativePtySystem::default()
            .openpty(PtySize::default())
            .unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut child = pair.slave.spawn_command(pty).unwrap();
        drop(pair.slave);
        let mut output = String::new();
        reader.read_to_string(&mut output).unwrap();
        assert!(child.wait().unwrap().success());
        assert!(output.contains("HOME=/fixture/home"));
        assert!(!output.contains("AWS_SECRET_ACCESS_KEY"));
    }

    #[test]
    fn isolated_installation_can_use_the_installed_toolchain_without_host_cargo_configuration() {
        let active = Command::new("rustup")
            .args(["show", "active-toolchain"])
            .output()
            .unwrap();
        assert!(active.status.success());
        let output = String::from_utf8(active.stdout).unwrap();
        let toolchain = output.split_whitespace().next().unwrap();
        let home = std::env::temp_dir().join(format!("vmux-rustup-fixture-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        let environment = LoginShellEnvironment::isolated(&home, std::env::vars());
        let output = Command::new("rustup")
            .args(["run", toolchain, "rustc", "--version"])
            .current_dir(&home)
            .env_clear()
            .envs(environment)
            .output()
            .unwrap();
        std::fs::remove_dir_all(home).unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .starts_with("rustc ")
        );
    }

    #[test]
    fn parse_env_splits_on_first_equals() {
        let out = b"PATH=/usr/bin:/bin\nKEY=a=b=c\nEMPTY=\nnovalue\n=novalue\n";
        assert_eq!(
            LoginShellEnvironment::parse(out),
            vec![
                ("PATH".to_string(), "/usr/bin:/bin".to_string()),
                ("KEY".to_string(), "a=b=c".to_string()),
                ("EMPTY".to_string(), String::new()),
            ]
        );
    }

    #[test]
    fn merge_overrides_existing_keys_keeping_order() {
        let mut env = vec![
            ("VIBE_MCP_SERVERS".to_string(), "[...]".to_string()),
            ("ANTHROPIC_FOUNDRY_API_KEY".to_string(), "stale".to_string()),
            ("ANTHROPIC_FOUNDRY_API_KEY".to_string(), "fresh".to_string()),
            ("PATH".to_string(), "/login/bin".to_string()),
        ];
        LoginShellEnvironment::dedup(&mut env);
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
            LoginShellEnvironment::extract(raw.as_bytes()),
            vec![
                ("PATH".to_string(), "/usr/bin:/bin".to_string()),
                ("FOO".to_string(), "bar".to_string()),
            ]
        );
    }

    #[test]
    fn extract_env_finds_marker_with_prompt_prefix() {
        let raw = format!("host% {ENV_BEGIN}\nKEY=val\n{ENV_END}\n");
        assert_eq!(
            LoginShellEnvironment::extract(raw.as_bytes()),
            vec![("KEY".to_string(), "val".to_string())]
        );
    }

    #[test]
    fn extract_env_without_markers_is_empty() {
        let raw = b"PATH=/usr/bin\nFOO=bar\n";
        assert!(LoginShellEnvironment::extract(raw).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn pty_capture_reads_env_from_login_shell() {
        let shell = ["/bin/zsh", "/bin/bash", "/bin/sh"]
            .into_iter()
            .find(|path| Path::new(path).exists())
            .expect("a POSIX login shell should exist on unix");
        let env = LoginShellEnvironment::capture(shell);
        assert!(
            env.iter().any(|(key, _)| key == "PATH"),
            "expected PATH in env captured from {shell}, got {} vars",
            env.len()
        );
    }
}
