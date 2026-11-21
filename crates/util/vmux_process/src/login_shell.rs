use std::collections::HashSet;
use std::io::Read;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
#[cfg(not(unix))]
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock, TryLockError};
use std::time::Duration;

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};

const ENV_BEGIN: &str = "__VMUX_LOGIN_ENV_BEGIN_7Qz9__";
const ENV_END: &str = "__VMUX_LOGIN_ENV_END_7Qz9__";

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);
const CAPTURE_MAX_BYTES: usize = 1024 * 1024;

static CACHE: EnvironmentCache = EnvironmentCache::new();

struct EnvironmentCache {
    values: OnceLock<Vec<(String, String)>>,
    capture: Mutex<()>,
}

impl EnvironmentCache {
    const fn new() -> Self {
        Self {
            values: OnceLock::new(),
            capture: Mutex::new(()),
        }
    }

    fn get(&self, shell: &str) -> &[(String, String)] {
        if let Some(values) = self.values.get() {
            return values;
        }
        let _capture = loop {
            if vmux_ecs::BlockingCancellation::canceled() {
                return &[];
            }
            match self.capture.try_lock() {
                Ok(guard) => break guard,
                Err(TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(5)),
            }
        };
        if let Some(values) = self.values.get() {
            return values;
        }
        let Some(values) = LoginShellEnvironment::capture_via_pty(shell) else {
            return &[];
        };
        if vmux_ecs::BlockingCancellation::canceled() {
            return &[];
        }
        let _ = self.values.set(values);
        self.values.get().map(Vec::as_slice).unwrap_or(&[])
    }
}

pub struct LoginShellEnvironment;

impl LoginShellEnvironment {
    pub fn get(shell: &str) -> &'static [(String, String)] {
        CACHE.get(shell)
    }

    pub fn prewarm(shell: String) {
        if CACHE.values.get().is_some() {
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

    #[cfg(test)]
    fn capture(shell: &str) -> Vec<(String, String)> {
        Self::capture_via_pty(shell).unwrap_or_default()
    }

    #[cfg(unix)]
    fn capture_via_pty(shell: &str) -> Option<Vec<(String, String)>> {
        let args = Self::capture_args(shell);
        if vmux_ecs::BlockingCancellation::canceled() {
            return None;
        }
        let pair = NativePtySystem::default()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .ok()?;
        let descriptor = pair.master.as_raw_fd()?;
        let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
        {
            return None;
        }
        let mut reader = pair.master.try_clone_reader().ok()?;
        let mut command = CommandBuilder::new(shell);
        command.args(&args);
        for (key, value) in std::env::vars() {
            command.env(key, value);
        }
        command.env("TERM", "xterm-256color");
        if let Some(home) = std::env::var_os("HOME") {
            command.cwd(home);
        }
        let mut child = pair.slave.spawn_command(command).ok()?;
        drop(pair.slave);
        let deadline = std::time::Instant::now() + CAPTURE_TIMEOUT;
        let mut bytes = Vec::new();
        let mut buffer = [0; 8192];
        let mut accepted = true;
        loop {
            if vmux_ecs::BlockingCancellation::canceled() || std::time::Instant::now() >= deadline {
                accepted = false;
                break;
            }
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if bytes.len().saturating_add(count) > CAPTURE_MAX_BYTES {
                        accepted = false;
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
                Err(_) => {
                    accepted = false;
                    break;
                }
            }
        }
        while accepted && child.try_wait().ok().flatten().is_none() {
            if vmux_ecs::BlockingCancellation::canceled() || std::time::Instant::now() >= deadline {
                accepted = false;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        if !accepted {
            if let Some(id) = child.process_id() {
                unsafe {
                    libc::kill(-(id as libc::pid_t), libc::SIGKILL);
                }
            }
            let _ = child.kill();
        }
        let _ = child.wait();
        drop(reader);
        drop(pair.master);
        if !accepted {
            return None;
        }
        Some(Self::extract(&bytes))
    }

    #[cfg(not(unix))]
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

    #[cfg(unix)]
    fn nu_config_paths(shell: &str) -> Option<(String, String)> {
        if vmux_ecs::BlockingCancellation::canceled() {
            return None;
        }
        let mut child = Command::new(shell)
            .args(["-c", "[$nu.env-path $nu.config-path] | to text"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .ok()?;
        let mut stdout = child.stdout.take().unwrap();
        let descriptor = stdout.as_raw_fd();
        let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
        {
            unsafe {
                libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        let deadline = std::time::Instant::now() + CAPTURE_TIMEOUT;
        let mut bytes = Vec::new();
        let mut buffer = [0; 4096];
        let mut accepted = true;
        loop {
            if vmux_ecs::BlockingCancellation::canceled() || std::time::Instant::now() >= deadline {
                accepted = false;
                break;
            }
            match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if bytes.len().saturating_add(count) > 16 * 1024 {
                        accepted = false;
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => {
                    accepted = false;
                    break;
                }
            }
        }
        while accepted && child.try_wait().ok().flatten().is_none() {
            if vmux_ecs::BlockingCancellation::canceled() || std::time::Instant::now() >= deadline {
                accepted = false;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        if !accepted {
            unsafe {
                libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
            }
            let _ = child.kill();
        }
        let status = child.wait().ok()?;
        if !accepted || !status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&bytes);
        let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
        Some((lines.next()?.to_string(), lines.next()?.to_string()))
    }

    #[cfg(not(unix))]
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
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[cfg(unix)]
    struct CaptureFixture {
        app: bevy::prelude::App,
    }

    #[cfg(unix)]
    #[derive(bevy::prelude::Resource)]
    struct CaptureInput {
        name: String,
        body: String,
    }

    #[cfg(unix)]
    #[derive(bevy::prelude::Resource)]
    struct CapturePaths {
        _directory: tempfile::TempDir,
        shell: std::path::PathBuf,
        pid: std::path::PathBuf,
    }

    #[cfg(unix)]
    struct CaptureFixturePlugin;

    #[cfg(unix)]
    impl bevy::prelude::Plugin for CaptureFixturePlugin {
        fn build(&self, app: &mut bevy::prelude::App) {
            app.init_resource::<CaptureProcess>()
                .add_systems(bevy::prelude::Startup, create)
                .add_systems(bevy::prelude::Update, observe);
        }
    }

    #[cfg(unix)]
    fn create(input: bevy::prelude::Res<CaptureInput>, mut commands: bevy::prelude::Commands) {
        commands.insert_resource(CapturePaths::new(&input.name, &input.body));
    }

    #[cfg(unix)]
    #[derive(bevy::prelude::Resource, Default)]
    struct CaptureProcess(Option<libc::pid_t>);

    #[cfg(unix)]
    fn observe(
        paths: bevy::prelude::Res<CapturePaths>,
        mut process: bevy::prelude::ResMut<CaptureProcess>,
    ) {
        if let Ok(text) = std::fs::read_to_string(&paths.pid)
            && let Ok(pid) = text.parse::<libc::pid_t>()
            && pid > 0
        {
            process.0 = Some(pid);
        }
    }

    #[cfg(unix)]
    impl CapturePaths {
        fn new(name: &str, body: &str) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let shell = directory.path().join(name);
            let pid = directory.path().join("pid");
            let quoted = pid.to_string_lossy().replace('\'', "'\\''");
            std::fs::write(
                &shell,
                format!("#!/bin/sh\nprintf '%s' $$ > '{quoted}'\n{body}\n"),
            )
            .unwrap();
            std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
            Self {
                _directory: directory,
                shell,
                pid,
            }
        }
    }

    #[cfg(unix)]
    impl CaptureFixture {
        fn new(name: &str, body: &str) -> Self {
            let mut app = bevy::prelude::App::new();
            app.insert_resource(CaptureInput {
                name: name.to_string(),
                body: body.to_string(),
            })
            .add_plugins(CaptureFixturePlugin);
            app.update();
            Self { app }
        }

        fn paths(&self) -> &CapturePaths {
            self.app.world().resource::<CapturePaths>()
        }

        fn process(&mut self) -> libc::pid_t {
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            loop {
                self.app.update();
                if let Some(pid) = self.app.world().resource::<CaptureProcess>().0 {
                    return pid;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "fixture child must report its owned PID"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn oversized_pty_output_is_refused_and_the_owned_child_is_reaped() {
        let mut fixture = CaptureFixture::new("flood-shell", "exec head -c 2097152 /dev/zero");
        assert!(
            LoginShellEnvironment::capture_via_pty(fixture.paths().shell.to_str().unwrap())
                .is_none()
        );
        let pid = fixture.process();
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            -1,
            "owned capture child must already be reaped"
        );
    }

    #[cfg(unix)]
    #[test]
    fn configuration_probe_cancellation_drains_and_reaps_before_capture() {
        let mut fixture = CaptureFixture::new("nu", "exec sleep 600");
        let runtime = vmux_ecs::BlockingRuntime::default();
        let shell = fixture.paths().shell.to_string_lossy().into_owned();
        let mut task = runtime
            .spawn_process(None, move || LoginShellEnvironment::capture_via_pty(&shell))
            .unwrap();
        let pid = fixture.process();
        task.cancel();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !task.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        let canceled_before_timeout = task.is_finished();
        if !canceled_before_timeout {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
            let cleanup = std::time::Instant::now() + Duration::from_secs(3);
            while !task.is_finished() && std::time::Instant::now() < cleanup {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert!(
            task.is_finished(),
            "fixture native operation must drain even after a failed assertion"
        );
        assert!(
            canceled_before_timeout,
            "cancellation must not wait for configuration probe timeout"
        );
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            -1,
            "owned configuration process must be reaped"
        );
    }

    #[cfg(unix)]
    #[test]
    fn canceled_initial_capture_does_not_poison_later_native_cache_reads() {
        let mut blocked = CaptureFixture::new("nu", "exec sleep 600");
        let successful = CaptureFixture::new(
            "ready-shell",
            &format!("printf '%s\n' '{ENV_BEGIN}' 'VMUX_CAPTURE_FIXTURE=retained' '{ENV_END}'"),
        );
        let cache = std::sync::Arc::new(EnvironmentCache::new());
        let captured = cache.clone();
        let shell = blocked.paths().shell.to_string_lossy().into_owned();
        let runtime = vmux_ecs::BlockingRuntime::default();
        let mut task = runtime
            .spawn_process(None, move || captured.get(&shell).to_vec())
            .unwrap();
        let pid = blocked.process();
        task.cancel();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !task.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        let canceled = task.is_finished();
        if !canceled {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
            let cleanup = std::time::Instant::now() + Duration::from_secs(3);
            while !task.is_finished() && std::time::Instant::now() < cleanup {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert!(
            task.is_finished(),
            "fixture must drain its native capture before release"
        );
        assert!(
            canceled,
            "cancellation must terminate the first native capture"
        );
        let expected = [("VMUX_CAPTURE_FIXTURE".to_string(), "retained".to_string())];
        assert_eq!(
            cache.get(successful.paths().shell.to_str().unwrap()),
            expected
        );
        assert_eq!(
            cache.get(blocked.paths().shell.to_str().unwrap()),
            expected,
            "a successful snapshot stays cached"
        );
    }

    #[cfg(unix)]
    #[test]
    fn configuration_probe_waits_for_delayed_descendant_output_and_eof() {
        let fixture = CaptureFixture::new(
            "nu",
            "printf '%s\n' /fixture/env.nu; (sleep 0.1; printf '%s\n' /fixture/config.nu) &",
        );
        assert_eq!(
            LoginShellEnvironment::nu_config_paths(fixture.paths().shell.to_str().unwrap()),
            Some(("/fixture/env.nu".into(), "/fixture/config.nu".into()))
        );
    }

    #[cfg(unix)]
    #[test]
    fn pty_capture_retains_sparse_environment_output_until_stream_completion() {
        let fixture = CaptureFixture::new(
            "delayed-shell",
            &format!(
                "trap '' HUP; printf '%s\n' '{ENV_BEGIN}'; (sleep 0.1; printf '%s\n' 'VMUX_CAPTURE_FIXTURE=delayed' '{ENV_END}') & wait"
            ),
        );
        assert_eq!(
            LoginShellEnvironment::capture_via_pty(fixture.paths().shell.to_str().unwrap()),
            Some(vec![("VMUX_CAPTURE_FIXTURE".into(), "delayed".into())])
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
