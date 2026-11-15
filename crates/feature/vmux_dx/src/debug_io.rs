use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::task::Waker;

use bevy_ecs::prelude::{Component, Entity, Resource};
use sha2::{Digest, Sha256};
use vmux_ecs::blocking::Lane;
use vmux_ecs::jobs::Job;

use crate::environment::Environment;
use crate::execution::{Execution, Failure};
use crate::signals::Signals;
use crate::target_lock::TargetLock;
use crate::temporary::Temporary;

pub struct Task<J: Job> {
    pub job: J,
    pub guard: Arc<Guard>,
}

impl<J: Job> Task<J> {
    pub fn new(
        job: J,
        target: Option<Arc<TargetLock>>,
        temporary: Vec<Arc<Temporary>>,
        wake: Waker,
    ) -> (Self, Flight) {
        let (guard, flight) = Guard::new(target, temporary, wake);
        (Self { job, guard }, flight)
    }
}

impl<J: Job> Clone for Task<J> {
    fn clone(&self) -> Self {
        Self {
            job: self.job.clone(),
            guard: self.guard.clone(),
        }
    }
}

impl<J: Job> Job for Task<J> {
    type Output = Report<J::Output>;

    fn run(self) -> Self::Output {
        Report {
            result: self.job.run(),
            _guard: self.guard,
        }
    }
    fn lane(&self) -> Lane {
        self.job.lane()
    }
}

pub struct Report<T> {
    pub result: T,
    _guard: Arc<Guard>,
}

impl<J: IoJob> Task<J> {
    pub fn tracked(
        mut job: J,
        target: Option<Arc<TargetLock>>,
        temporary: Vec<Arc<Temporary>>,
        wake: Waker,
        cleanup: CleanupRoute,
    ) -> (Self, Flight, CleanupFlight) {
        let cleanup_flight = cleanup.flight.clone();
        let (mut guard, flight) = Guard::new(target, temporary, wake);
        Arc::get_mut(&mut guard).unwrap().cleanup = Some(cleanup);
        job.retain(&guard);
        (Self { job, guard }, flight, cleanup_flight)
    }
}

pub struct Guard {
    target: Option<Arc<TargetLock>>,
    temporary: Vec<Arc<Temporary>>,
    complete: Arc<AtomicBool>,
    wake: Waker,
    cleanup: Option<CleanupRoute>,
}

impl Guard {
    pub fn new(
        target: Option<Arc<TargetLock>>,
        temporary: Vec<Arc<Temporary>>,
        wake: Waker,
    ) -> (Arc<Self>, Flight) {
        let complete = Arc::new(AtomicBool::new(false));
        let flight = Flight(complete.clone());
        (
            Arc::new(Self {
                target,
                temporary,
                complete,
                wake,
                cleanup: None,
            }),
            flight,
        )
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.temporary.clear();
        self.target.take();
        self.complete.store(true, Ordering::Release);
        self.wake.wake_by_ref();
    }
}

#[derive(Component, Clone)]
pub struct Flight(Arc<AtomicBool>);

impl Flight {
    pub fn pending(&self) -> bool {
        !self.0.load(Ordering::Acquire)
    }
}

#[derive(Component, Clone)]
pub struct CleanupFlight(Arc<AtomicBool>);

impl CleanupFlight {
    pub fn pending(&self) -> bool {
        !self.0.load(Ordering::Acquire)
    }
}

#[derive(Resource)]
pub struct CleanupQueue {
    sender: mpsc::Sender<(Entity, CleanupData)>,
    receiver: Mutex<mpsc::Receiver<(Entity, CleanupData)>>,
}

impl Default for CleanupQueue {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver: Mutex::new(receiver),
        }
    }
}

impl CleanupQueue {
    pub fn route(&self, entity: Entity, root: Entity, wake: Waker) -> CleanupRoute {
        CleanupRoute {
            entity,
            root,
            sender: self.sender.clone(),
            flight: CleanupFlight(Arc::new(AtomicBool::new(true))),
            wake,
        }
    }

    pub fn receive(&self) -> Option<(Entity, CleanupData)> {
        self.receiver.lock().unwrap().try_recv().ok()
    }
}

impl Drop for CleanupQueue {
    fn drop(&mut self) {
        while let Ok((_entity, data)) = self.receiver.get_mut().unwrap().try_recv() {
            drop(data);
        }
    }
}

#[derive(Clone)]
pub struct CleanupRoute {
    entity: Entity,
    root: Entity,
    sender: mpsc::Sender<(Entity, CleanupData)>,
    flight: CleanupFlight,
    wake: Waker,
}

#[derive(Clone)]
struct Retained {
    target: Option<Arc<TargetLock>>,
    temporary: Vec<Arc<Temporary>>,
    io: Flight,
    route: CleanupRoute,
}

pub struct CleanupData {
    child: Option<std::process::Child>,
    retained: Retained,
}

impl CleanupData {
    pub fn root(&self) -> Entity {
        self.retained.route.root
    }

    pub fn io(&self) -> Flight {
        self.retained.io.clone()
    }

    pub fn flight(&self) -> CleanupFlight {
        self.retained.route.flight.clone()
    }
}

impl Drop for CleanupData {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            Signals::terminate_group(child.id());
            let _ = child.wait();
        }
        self.retained.temporary.clear();
        self.retained.target.take();
        self.retained.route.flight.0.store(true, Ordering::Release);
        self.retained.route.wake.wake_by_ref();
    }
}

#[derive(Component)]
pub struct ReapRequest {
    data: Arc<Mutex<Option<CleanupData>>>,
    flight: CleanupFlight,
}

impl ReapRequest {
    pub fn new(data: CleanupData) -> Self {
        Self {
            flight: data.flight(),
            data: Arc::new(Mutex::new(Some(data))),
        }
    }

    pub fn job(&self) -> impl FnOnce() + Send + 'static {
        let state = self.data.clone();
        move || {
            let data = state.lock().unwrap().take();
            drop(data);
        }
    }
}

impl Drop for ReapRequest {
    fn drop(&mut self) {
        let data = self.data.lock().unwrap().take();
        drop(data);
        while self.flight.pending() {
            std::thread::park_timeout(std::time::Duration::from_millis(10));
        }
    }
}

pub trait IoJob: Job<Output = Result<Self::Data, Failure>> + Component {
    type Data: IoValue;
    fn retain(&mut self, _guard: &Arc<Guard>) {}
}

pub trait IoValue: Component + Clone {
    fn validate(&self, _optional: bool) -> Result<(), Failure> {
        Ok(())
    }
}

#[derive(Component, Clone)]
#[require(crate::debug::Io)]
pub struct ToolOutput(pub std::process::Output);

#[derive(Component, Clone)]
#[require(crate::debug::Io)]
pub struct FileBytes(pub Vec<u8>);

#[derive(Component, Clone)]
#[require(crate::debug::Io)]
pub struct FileDigest(pub String);

#[derive(Component, Clone)]
#[require(crate::debug::Io)]
pub struct PathMetadata(pub Option<fs::Metadata>);

#[derive(Component, Clone)]
#[require(crate::debug::Io)]
pub struct DirectoryEntries(pub Vec<(PathBuf, fs::Metadata)>);

#[derive(Component, Clone)]
#[require(crate::debug::Io)]
pub struct TemporaryValue(pub Arc<Temporary>);

#[derive(Component, Clone)]
#[require(crate::debug::Io)]
pub struct ProcessValue {
    _lease: Arc<Process>,
}

#[derive(Component, Clone)]
#[require(crate::debug::Io)]
pub struct PendingValue {
    _lease: Arc<Pending>,
}

#[derive(Component, Clone)]
#[require(crate::debug::Io)]
pub struct Done;

impl IoValue for ToolOutput {
    fn validate(&self, optional: bool) -> Result<(), Failure> {
        if !self.0.status.success() && !optional {
            let mut failure = Failure::status(self.0.status);
            let diagnostic = String::from_utf8_lossy(&self.0.stderr);
            if !diagnostic.trim().is_empty() {
                failure = failure.context(diagnostic.trim());
            }
            return Err(failure);
        }
        std::str::from_utf8(&self.0.stdout).map_err(|error| Failure::message(error.to_string()))?;
        Ok(())
    }
}

impl IoValue for FileBytes {
    fn validate(&self, _optional: bool) -> Result<(), Failure> {
        std::str::from_utf8(&self.0).map_err(|error| Failure::message(error.to_string()))?;
        Ok(())
    }
}

impl IoValue for FileDigest {}

impl IoValue for PathMetadata {}

impl IoValue for DirectoryEntries {}

impl IoValue for TemporaryValue {}

impl IoValue for ProcessValue {}

impl IoValue for PendingValue {}

impl IoValue for Done {}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub struct CaptureOutput {
    pub environment: Environment,
    pub cwd: PathBuf,
    pub program: OsString,
    pub arguments: Vec<OsString>,
}

impl CaptureOutput {
    pub fn new(
        environment: &Environment,
        cwd: &Path,
        program: OsString,
        arguments: Vec<OsString>,
    ) -> Self {
        Self {
            environment: environment.clone(),
            cwd: cwd.to_owned(),
            program,
            arguments,
        }
    }
}

impl IoJob for CaptureOutput {
    type Data = ToolOutput;
}

impl Job for CaptureOutput {
    type Output = Result<ToolOutput, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        let mut command = self.environment.command(self.program, &self.cwd);
        command.args(self.arguments);
        let output = Execution::new(command)
            .output_any(None)
            .map_err(Failure::from_error)?;
        Ok(ToolOutput(output))
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub struct RunProcess {
    pub environment: Environment,
    pub cwd: PathBuf,
    pub program: OsString,
    pub arguments: Vec<OsString>,
}

impl RunProcess {
    pub fn new(
        environment: &Environment,
        cwd: &Path,
        program: OsString,
        arguments: Vec<OsString>,
    ) -> Self {
        Self {
            environment: environment.clone(),
            cwd: cwd.to_owned(),
            program,
            arguments,
        }
    }
}

impl IoJob for RunProcess {
    type Data = Done;
}

impl Job for RunProcess {
    type Output = Result<Done, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        let mut command = self.environment.command(self.program, &self.cwd);
        command.args(self.arguments);
        Execution::new(command).run().map_err(Failure::from_error)?;
        Ok(Done)
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub struct SpawnProcess {
    pub environment: Environment,
    pub cwd: PathBuf,
    pub program: OsString,
    pub arguments: Vec<OsString>,
    retained: Option<Retained>,
}

impl SpawnProcess {
    pub fn new(
        environment: &Environment,
        cwd: &Path,
        program: OsString,
        arguments: Vec<OsString>,
    ) -> Self {
        Self {
            environment: environment.clone(),
            cwd: cwd.to_owned(),
            program,
            arguments,
            retained: None,
        }
    }
}

impl IoJob for SpawnProcess {
    type Data = ProcessValue;
    fn retain(&mut self, guard: &Arc<Guard>) {
        if let Some(route) = &guard.cleanup {
            self.retained = Some(Retained {
                target: guard.target.clone(),
                temporary: guard.temporary.clone(),
                io: Flight(guard.complete.clone()),
                route: route.clone(),
            });
        }
    }
}

impl Job for SpawnProcess {
    type Output = Result<ProcessValue, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        let mut command = self.environment.command(self.program, &self.cwd);
        command.args(self.arguments).process_group(0);
        let child = command
            .spawn()
            .map_err(|error| Failure::message(error.to_string()))?;
        if let Some(retained) = &self.retained {
            retained.route.flight.0.store(false, Ordering::Release);
        }
        Ok(ProcessValue {
            _lease: Arc::new(Process {
                child: Some(child),
                retained: self.retained,
            }),
        })
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub struct ReadFile(pub PathBuf);

impl IoJob for ReadFile {
    type Data = FileBytes;
}

impl Job for ReadFile {
    type Output = Result<FileBytes, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(self.0)
            .map_err(|error| Failure::message(error.to_string()))?;
        if !file
            .metadata()
            .map_err(|error| Failure::message(error.to_string()))?
            .is_file()
        {
            return Err(Failure::message("Debug input must be a regular file"));
        }
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| Failure::message(error.to_string()))?;
        if bytes.len() > 1024 * 1024 {
            return Err(Failure::message("Debug metadata exceeds 1 MiB"));
        }
        Ok(FileBytes(bytes))
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub struct HashFile(pub PathBuf);

impl IoJob for HashFile {
    type Data = FileDigest;
}

impl Job for HashFile {
    type Output = Result<FileDigest, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(self.0)
            .map_err(|error| Failure::message(error.to_string()))?;
        if !file
            .metadata()
            .map_err(|error| Failure::message(error.to_string()))?
            .is_file()
        {
            return Err(Failure::message("Debug input must be a regular file"));
        }
        let mut file = file;
        let mut hash = Sha256::new();
        let mut bytes = [0; 65536];
        loop {
            if vmux_ecs::blocking::BlockingCancellation::canceled() {
                return Err(Failure::message("Debug IO canceled"));
            }
            let count = file
                .read(&mut bytes)
                .map_err(|error| Failure::message(error.to_string()))?;
            if count == 0 {
                break;
            }
            hash.update(&bytes[..count]);
        }
        Ok(FileDigest(format!("{:x}", hash.finalize())))
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub struct InspectPath(pub PathBuf);

impl IoJob for InspectPath {
    type Data = PathMetadata;
}

impl Job for InspectPath {
    type Output = Result<PathMetadata, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        match fs::symlink_metadata(self.0) {
            Ok(metadata) => Ok(PathMetadata(Some(metadata))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(PathMetadata(None)),
            Err(error) => Err(Failure::message(error.to_string())),
        }
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub struct ReadDirectory(pub PathBuf);

impl IoJob for ReadDirectory {
    type Data = DirectoryEntries;
}

impl Job for ReadDirectory {
    type Output = Result<DirectoryEntries, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(self.0).map_err(|error| Failure::message(error.to_string()))? {
            if entries.len() == 4096 {
                return Err(Failure::message(
                    "Local artifact directory exceeds its metadata bounds",
                ));
            }
            let entry = entry.map_err(|error| Failure::message(error.to_string()))?;
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|error| Failure::message(error.to_string()))?;
            entries.push((entry.path(), metadata));
        }
        Ok(DirectoryEntries(entries))
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub struct CreateDirectory(pub PathBuf);

impl IoJob for CreateDirectory {
    type Data = Done;
}

impl Job for CreateDirectory {
    type Output = Result<Done, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        fs::create_dir_all(self.0).map_err(|error| Failure::message(error.to_string()))?;
        Ok(Done)
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub struct CopyFile(pub PathBuf, pub PathBuf);

impl IoJob for CopyFile {
    type Data = Done;
}

impl Job for CopyFile {
    type Output = Result<Done, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        if self.0 != self.1
            && (!self.1.exists()
                || self
                    .0
                    .canonicalize()
                    .map_err(|error| Failure::message(error.to_string()))?
                    != self
                        .1
                        .canonicalize()
                        .map_err(|error| Failure::message(error.to_string()))?)
        {
            fs::copy(self.0, self.1).map_err(|error| Failure::message(error.to_string()))?;
        }
        Ok(Done)
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub struct RemoveFile(pub PathBuf);

impl IoJob for RemoveFile {
    type Data = Done;
}

impl Job for RemoveFile {
    type Output = Result<Done, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        fs::remove_file(self.0).map_err(|error| Failure::message(error.to_string()))?;
        Ok(Done)
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub struct LinkPath(pub PathBuf, pub PathBuf);

impl IoJob for LinkPath {
    type Data = Done;
}

impl Job for LinkPath {
    type Output = Result<Done, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        symlink(self.0, self.1).map_err(|error| Failure::message(error.to_string()))?;
        Ok(Done)
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub struct WriteFile(pub PathBuf, pub Vec<u8>);

impl IoJob for WriteFile {
    type Data = PendingValue;
}

impl Job for WriteFile {
    type Output = Result<PendingValue, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.0)
            .map_err(|error| Failure::message(error.to_string()))?;
        let pending = Arc::new(Pending(self.0));
        file.write_all(&self.1)
            .map_err(|error| Failure::message(error.to_string()))?;
        Ok(PendingValue { _lease: pending })
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub struct RenamePath(pub PathBuf, pub PathBuf);

impl IoJob for RenamePath {
    type Data = Done;
}

impl Job for RenamePath {
    type Output = Result<Done, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        fs::rename(self.0, self.1).map_err(|error| Failure::message(error.to_string()))?;
        Ok(Done)
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub struct CreateTemporary;

impl IoJob for CreateTemporary {
    type Data = TemporaryValue;
}

impl Job for CreateTemporary {
    type Output = Result<TemporaryValue, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        let directory = Temporary::create(&std::env::temp_dir(), "vmux-local-codesign")
            .map_err(|error| Failure::message(error.to_string()))?;
        Ok(TemporaryValue(Arc::new(directory)))
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

pub struct Process {
    child: Option<std::process::Child>,
    retained: Option<Retained>,
}

impl Drop for Process {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if let Some(retained) = self.retained.take() {
            let entity = retained.route.entity;
            let sender = retained.route.sender.clone();
            let wake = retained.route.wake.clone();
            let data = CleanupData {
                child: Some(child),
                retained,
            };
            if let Err(error) = sender.send((entity, data)) {
                drop(error.0.1);
            } else {
                wake.wake_by_ref();
            }
        } else {
            Signals::terminate_group(child.id());
            let _ = child.wait();
        }
    }
}

pub struct Pending(PathBuf);

impl Drop for Pending {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::task::Wake;
    use std::time::Duration;

    use super::*;

    #[test]
    fn staging_rejects_existing_files_and_links_without_changing_them() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        fs::write(&source, b"original").unwrap();
        let symbolic = directory.path().join("symbolic");
        symlink(&source, &symbolic).unwrap();
        let hard = directory.path().join("hard");
        fs::hard_link(&source, &hard).unwrap();
        for path in [&source, &symbolic, &hard] {
            assert!(
                WriteFile(path.to_owned(), b"replacement".to_vec())
                    .run()
                    .is_err()
            );
            assert_eq!(fs::read(&source).unwrap(), b"original");
            assert!(fs::symlink_metadata(path).is_ok());
        }
    }

    #[test]
    fn staging_refuses_a_fifo_without_waiting_for_a_reader() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("staging");
        let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            sender
                .send(WriteFile(path, b"replacement".to_vec()).run().is_err())
                .unwrap();
        });
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("FIFO open blocked")
        );
        thread.join().unwrap();
        assert!(fs::symlink_metadata(directory.path().join("staging")).is_ok());
    }

    #[test]
    fn failed_partial_staging_write_removes_the_owned_file() {
        if let Some(path) = std::env::var_os("VMUX_STAGING_QUOTA_FIXTURE") {
            let limit = libc::rlimit {
                rlim_cur: 1,
                rlim_max: 1,
            };
            unsafe {
                libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
                assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &limit), 0);
            }
            let path = PathBuf::from(path);
            assert!(
                WriteFile(path.clone(), b"more than one byte".to_vec())
                    .run()
                    .is_err()
            );
            assert!(!path.exists());
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "debug_io::tests::failed_partial_staging_write_removes_the_owned_file",
                "--nocapture",
            ])
            .env(
                "VMUX_STAGING_QUOTA_FIXTURE",
                directory.path().join("staging"),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    struct Released {
        path: PathBuf,
        result: mpsc::Sender<bool>,
        flight: Mutex<Option<Flight>>,
    }

    impl Wake for Released {
        fn wake(self: Arc<Self>) {
            self.result
                .send(
                    !self.path.exists() && !self.flight.lock().unwrap().as_ref().unwrap().pending(),
                )
                .unwrap();
        }
    }

    #[test]
    fn discarded_process_result_keeps_its_loan_until_worker_cleanup_finishes() {
        let directory = tempfile::tempdir().unwrap();
        let temporary = Arc::new(Temporary::create(directory.path(), "loan").unwrap());
        let path = temporary.path().to_owned();
        let queue = CleanupQueue::default();
        let wake = std::task::Waker::noop().clone();
        let route = queue.route(Entity::PLACEHOLDER, Entity::PLACEHOLDER, wake.clone());
        let environment = Environment::current();
        let process = SpawnProcess::new(
            &environment,
            directory.path(),
            "/bin/sleep".into(),
            vec!["60".into()],
        );
        let (task, io, cleanup) =
            Task::tracked(process, None, vec![temporary.clone()], wake, route);
        drop(temporary);
        let report = task.run();
        assert!(report.result.is_ok());
        drop(report);
        assert!(!io.pending());
        assert!(cleanup.pending());
        assert!(path.exists());
        let (_entity, data) = queue
            .receive()
            .expect("discarded process must request cleanup");
        let request = ReapRequest::new(data);
        let pool = vmux_ecs::blocking::BlockingPool::new("cleanup-fixture", 1, 1);
        let mut running = pool.try_spawn(None, request.job()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = running.poll() {
                assert!(result.is_ok());
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "process cleanup did not finish"
            );
            std::thread::park_timeout(Duration::from_millis(5));
        }
        assert!(!cleanup.pending());
        assert!(!path.exists());
    }

    #[test]
    fn native_result_cleanup_finishes_before_the_completion_notification() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pending-file");
        let (sender, receiver) = mpsc::channel();
        let wake = Arc::new(Released {
            path: path.clone(),
            result: sender,
            flight: Mutex::new(None),
        });
        let (guard, flight) = Guard::new(None, Vec::new(), wake.clone().into());
        *wake.flight.lock().unwrap() = Some(flight.clone());
        let report = Task {
            job: WriteFile(path.clone(), b"temporary result".to_vec()),
            guard,
        }
        .run();
        assert!(report.result.is_ok());
        assert_eq!(fs::read(&path).unwrap(), b"temporary result");
        assert!(flight.pending());
        drop(report);
        assert!(receiver.recv_timeout(Duration::from_secs(1)).unwrap());
        assert!(!flight.pending());
    }
}
