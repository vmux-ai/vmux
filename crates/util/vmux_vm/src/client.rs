use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use crate::Settings;
use crate::engine::{Phase, State};
use crate::http::UnixHttp;
use crate::image::GuestImage;
use crate::process::Executable;
use crate::ssh::Ssh;

pub struct EngineClient {
    settings: Settings,
}

#[derive(Clone)]
pub struct Owner {
    scope: PathBuf,
    cancelled: Arc<AtomicBool>,
}

pub(crate) struct Preparation<'a> {
    owner: Option<&'a Owner>,
}

impl<'a> Preparation<'a> {
    pub(crate) fn new(owner: Option<&'a Owner>) -> Self {
        Self { owner }
    }

    pub(crate) fn check(&self) -> io::Result<()> {
        if let Some(owner) = self.owner {
            owner.check()?;
        }
        Ok(())
    }

    pub(crate) fn run(&self, command: &mut Command) -> io::Result<std::process::ExitStatus> {
        self.check()?;
        command.process_group(0);
        vmux_spawn::Descriptors::isolate(command)?;
        let mut child = command.spawn()?;
        let group = vmux_spawn::ProcessGroup::of(child.id())?;
        let result = loop {
            if let Err(error) = self.check() {
                break Err(error);
            }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(error) => break Err(error),
            }
        };
        if result.is_err() {
            let _ = group.kill();
            let _ = child.kill();
            let _ = child.wait();
        }
        result
    }
}

impl Owner {
    pub fn new(scope: PathBuf) -> Self {
        Self {
            scope: Self::normalize(scope),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn normalize(scope: PathBuf) -> PathBuf {
        for ancestor in scope.ancestors() {
            if let Ok(resolved) = ancestor.canonicalize()
                && let Ok(suffix) = scope.strip_prefix(ancestor)
            {
                return resolved.join(suffix);
            }
        }
        scope
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn check(&self) -> io::Result<()> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "application VM startup cancelled",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Status {
    pub running: bool,
    pub docker_ready: bool,
    pub socket: PathBuf,
    pub log: PathBuf,
    pub phase: Phase,
    pub failure: Option<String>,
}

impl EngineClient {
    pub fn new(settings: Settings) -> Self {
        Self { settings }
    }

    pub fn prepare(&mut self) -> io::Result<()> {
        self.prepare_for(None)
    }

    fn prepare_for(&mut self, owner: Option<&Owner>) -> io::Result<()> {
        let preparation = Preparation::new(owner);
        preparation.check()?;
        self.settings.validate()?;
        #[cfg(unix)]
        {
            std::fs::set_permissions(
                &self.settings.directory,
                std::fs::Permissions::from_mode(0o700),
            )?;
        }
        let _lock = self.lock()?;
        let _machine = self.lock_named("machine.lock")?;
        let _network = self.lock_named("network.lock")?;
        GuestImage::prepare(&self.settings, &preparation)?;
        Ssh::prepare(&self.settings, &preparation)?;
        preparation.check()?;
        self.settings.save()?;
        Ok(())
    }

    pub fn configure(&mut self) -> io::Result<Settings> {
        self.settings.validate()?;
        let _start = self.lock_named("start.lock")?;
        let _engine = self.lock()?;
        let _machine = self.lock_named("machine.lock")?;
        let _network = self.lock_named("network.lock")?;
        self.settings.save()?;
        Ok(self.settings.clone())
    }

    pub fn start(&mut self, helper: &Path) -> io::Result<Status> {
        self.start_for(helper, None)
    }

    pub fn start_owned(&mut self, helper: &Path, owner: &Owner) -> io::Result<Status> {
        self.start_for(helper, Some(owner))
    }

    fn start_for(&mut self, helper: &Path, owner: Option<&Owner>) -> io::Result<Status> {
        if let Some(owner) = owner {
            owner.check()?;
        }
        if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the native VM backend currently requires Apple Silicon macOS",
            ));
        }
        self.settings.validate()?;
        let _start = self.lock_named("start.lock")?;
        if let Some(owner) = owner {
            owner.check()?;
        }
        if self.status().running {
            let saved = Settings::load(&self.settings.directory.join("settings.json"))?;
            if saved != self.settings {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "stop the running VM before changing its resources or shared directory",
                ));
            }
            return self.wait_ready(None, owner);
        }
        self.wait_retired()?;
        self.prepare_for(owner)?;
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.settings.log())?;
        let mut command = Command::new(Executable::named(helper, &self.settings, "")?);
        command
            .arg("run")
            .arg(self.settings.directory.join("settings.json"))
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        command.process_group(0);
        command.env_remove("VMUX_ENGINE_OWNER");
        if let Some(owner) = owner {
            command.env("VMUX_ENGINE_OWNER", &owner.scope);
            owner.check()?;
        }
        vmux_spawn::Descriptors::isolate(&mut command)?;
        let mut child = command.spawn()?;
        let result = self.wait_ready(Some(&mut child), owner);
        if result.is_err() {
            let _ = self.request_stop();
            let deadline = Instant::now() + Duration::from_secs(5);
            while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(25));
            }
            if !matches!(child.try_wait(), Ok(Some(_))) {
                let _ = child.kill();
            }
            let _ = child.wait();
            let _ = self.wait_retired();
        } else {
            if let Err(error) = vmux_spawn::Reaper::start(child) {
                let _ = self.request_stop();
                let _ = self.wait_retired();
                return Err(error);
            }
        }
        result
    }

    pub fn foreground(&mut self, helper: &Path) -> io::Result<()> {
        crate::engine::Runtime::run(self.settings.clone(), helper.to_path_buf())
    }

    pub fn stop(&self) -> io::Result<()> {
        if !self.status().running {
            return Ok(());
        }
        self.request_stop()?;
        let deadline = Instant::now() + Duration::from_secs(60);
        while self.status().running {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "VM did not finish shutdown",
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        self.wait_retired()
    }

    pub fn stop_owned(&self, owner: &Owner) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(75);
        let _start = loop {
            match self.lock_named("start.lock") {
                Ok(lock) => break lock,
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(error) => return Err(error),
            }
        };
        let bytes = match std::fs::read(self.settings.directory.join("engine-state.json")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let state: State = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if state.owner.as_deref() != Some(owner.scope.as_path()) {
            return Ok(());
        }
        if self.status().running {
            return self.stop();
        }
        if let Err(error) = crate::process::Lease::retired(&self.settings.directory) {
            if error.kind() != io::ErrorKind::WouldBlock {
                return Err(error);
            }
            let _ = self.request_stop();
            self.wait_retired()?;
        }
        Ok(())
    }

    pub(crate) fn request_stop(&self) -> io::Result<()> {
        let mut control = UnixStream::connect(self.settings.control_socket())?;
        control.set_read_timeout(Some(Duration::from_secs(2)))?;
        control.write_all(b"STOP\n")?;
        let mut reply = [0; 16];
        let read = control.read(&mut reply)?;
        if &reply[..read] != b"STOPPING\n" {
            return Err(io::Error::other("VM refused shutdown"));
        }
        Ok(())
    }

    pub fn status(&self) -> Status {
        let running = self.is_locked();
        let state = std::fs::read(self.settings.directory.join("engine-state.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<State>(&bytes).ok());
        let phase = if running {
            match state.as_ref().map(|state| state.phase) {
                Some(Phase::Stopped | Phase::Failed) | None => Phase::Created,
                Some(phase) => phase,
            }
        } else if state
            .as_ref()
            .is_some_and(|state| state.phase == Phase::Failed)
        {
            Phase::Failed
        } else {
            Phase::Stopped
        };
        Status {
            running,
            docker_ready: running && Self::ping(&self.settings.socket()),
            socket: self.settings.socket(),
            log: self.settings.log(),
            phase,
            failure: state.and_then(|state| state.failure),
        }
    }

    fn wait_ready(
        &self,
        mut child: Option<&mut Child>,
        owner: Option<&Owner>,
    ) -> io::Result<Status> {
        let deadline = Instant::now() + Duration::from_secs(240);
        loop {
            if let Some(owner) = owner {
                owner.check()?;
            }
            let status = self.status();
            if status.docker_ready {
                match std::fs::remove_file(self.settings.directory.join("data.new")) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
                return Ok(status);
            }
            if let Some(child) = child.as_deref_mut()
                && let Some(status) = child.try_wait()?
            {
                return Err(io::Error::other(format!(
                    "container engine exited: {status}; see {}",
                    self.settings.log().display()
                )));
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "Docker did not become ready; see {}",
                        self.settings.log().display()
                    ),
                ));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    pub(crate) fn ping(path: &Path) -> bool {
        match UnixHttp::new(path).get("/_ping") {
            Ok(response) => matches!(response.require_success(), Ok(body) if body == b"OK"),
            Err(_) => false,
        }
    }

    fn lock(&self) -> io::Result<File> {
        self.lock_named("engine.lock")
    }

    fn wait_retired(&self) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(65);
        loop {
            let result = crate::process::Lease::retired(&self.settings.directory);
            match result {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "previous VM did not finish cleanup",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn lock_named(&self, name: &str) -> io::Result<File> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(self.settings.directory.join(name))?;
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(file)
    }

    fn is_locked(&self) -> bool {
        match self.lock() {
            Ok(_) => false,
            Err(error) => error.kind() == io::ErrorKind::WouldBlock,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_preparation_kills_and_reaps_an_in_progress_child() {
        let temp = tempfile::tempdir().unwrap();
        let pid_path = temp.path().join("child.pid");
        let owner = Owner::new(temp.path().join("profile"));
        let running = owner.clone();
        let path = pid_path.clone();
        let worker = std::thread::spawn(move || {
            let mut command = Command::new("/bin/sh");
            command
                .args(["-c", "printf '%s' $$ > \"$1\"; sleep 30 & wait", "sh"])
                .arg(path);
            Preparation::new(Some(&running)).run(&mut command)
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        let pid: libc::pid_t = loop {
            if let Ok(value) = std::fs::read_to_string(&pid_path)
                && let Ok(pid) = value.parse()
            {
                break pid;
            }
            assert!(Instant::now() < deadline, "preparation child did not start");
            std::thread::sleep(Duration::from_millis(10));
        };
        owner.cancel();
        assert_eq!(
            worker.join().unwrap().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    }

    #[test]
    fn cancelled_application_start_does_not_prepare_files_or_spawn_a_helper() {
        let temp = tempfile::tempdir().unwrap();
        let settings = Settings::new(temp.path().join("engine"), temp.path());
        let owner = Owner::new(temp.path().join("profile"));
        owner.cancel();
        let error = EngineClient::new(settings)
            .start_owned(&temp.path().join("missing-helper"), &owner)
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(!temp.path().join("engine").exists());
    }

    #[test]
    fn an_owner_created_before_its_profile_matches_after_a_symlinked_profile_is_created() {
        let temp = tempfile::tempdir().unwrap();
        let alias = temp.path().join("alias");
        let real = temp.path().join("real");
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        let before = Owner::new(alias.join("profile"));
        std::fs::create_dir(real.join("profile")).unwrap();
        let after = Owner::new(real.join("profile"));
        assert_eq!(before.scope, after.scope);
    }

    #[test]
    fn application_exit_does_not_stop_cli_or_another_profile_engine() {
        for existing in [None, Some("another-profile")] {
            let temp = tempfile::tempdir().unwrap();
            let mut settings = Settings::new(temp.path().join("engine"), temp.path());
            settings.validate().unwrap();
            let client = EngineClient::new(settings.clone());
            let _lease = client.lock().unwrap();
            let listener =
                std::os::unix::net::UnixListener::bind(settings.control_socket()).unwrap();
            listener.set_nonblocking(true).unwrap();
            let state = State {
                phase: Phase::Ready,
                failure: None,
                owner: existing.map(|name| temp.path().join(name)),
            };
            std::fs::write(
                settings.directory.join("engine-state.json"),
                serde_json::to_vec(&state).unwrap(),
            )
            .unwrap();
            client
                .stop_owned(&Owner::new(temp.path().join("current-profile")))
                .unwrap();
            assert!(
                matches!(listener.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
            );
            assert!(client.status().running);
        }
    }

    #[test]
    fn application_exit_stops_only_its_owned_engine_and_waits_for_machine_retirement() {
        for supervisor_running in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let mut settings = Settings::new(temp.path().join("engine"), temp.path());
            settings.validate().unwrap();
            let client = EngineClient::new(settings.clone());
            let lease = client.lock().unwrap();
            let lease = if supervisor_running {
                Some(lease)
            } else {
                drop(lease);
                None
            };
            let machine = client.lock_named("machine.lock").unwrap();
            let owner = Owner::new(temp.path().join("profile"));
            let state = State {
                phase: Phase::Ready,
                failure: None,
                owner: Some(owner.scope.clone()),
            };
            std::fs::write(
                settings.directory.join("engine-state.json"),
                serde_json::to_vec(&state).unwrap(),
            )
            .unwrap();
            let listener =
                std::os::unix::net::UnixListener::bind(settings.control_socket()).unwrap();
            let (acknowledged, received) = std::sync::mpsc::channel();
            let (retire, retired) = std::sync::mpsc::channel();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 5];
                stream.read_exact(&mut request).unwrap();
                assert_eq!(&request, b"STOP\n");
                stream.write_all(b"STOPPING\n").unwrap();
                drop(lease);
                acknowledged.send(()).unwrap();
                retired.recv_timeout(Duration::from_secs(2)).unwrap();
                drop(machine);
            });
            let (done, finished) = std::sync::mpsc::channel();
            let worker = std::thread::spawn(move || done.send(client.stop_owned(&owner)).unwrap());
            received.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(matches!(
                finished.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Empty)
            ));
            retire.send(()).unwrap();
            finished
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap();
            server.join().unwrap();
            worker.join().unwrap();
        }
    }

    #[test]
    fn orphaned_machine_lease_protects_guest_files_after_the_engine_exits() {
        let temp = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(temp.path().join("engine"), temp.path());
        settings.validate().unwrap();
        std::fs::create_dir(settings.rootfs()).unwrap();
        let marker = settings.rootfs().join("untouched");
        std::fs::write(&marker, b"orphan still running").unwrap();
        let mut engine = EngineClient::new(settings);
        let _orphan = engine.lock_named("machine.lock").unwrap();
        assert_eq!(
            engine.prepare().unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(std::fs::read(marker).unwrap(), b"orphan still running");
    }

    #[test]
    fn configuring_a_running_engine_preserves_its_saved_resources() {
        let temp = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(temp.path().join("engine"), temp.path());
        settings.validate().unwrap();
        let path = settings.save().unwrap();
        let previous = std::fs::read(&path).unwrap();
        let mut engine = EngineClient::new(settings);
        let _running = engine.lock().unwrap();
        engine.settings.cpus = 4;
        assert_eq!(
            engine.configure().unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(std::fs::read(path).unwrap(), previous);
    }

    #[test]
    fn preparing_a_running_vm_does_not_change_its_guest_files() {
        let temp = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(temp.path().join("engine"), temp.path());
        settings.validate().unwrap();
        std::fs::create_dir(settings.rootfs()).unwrap();
        let marker = settings.rootfs().join("untouched");
        std::fs::write(&marker, b"running").unwrap();
        let mut engine = EngineClient::new(settings);
        let _running = engine.lock().unwrap();

        let error = engine.prepare().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(std::fs::read(marker).unwrap(), b"running");
        assert!(!temp.path().join("engine/data.raw").exists());
    }
}
