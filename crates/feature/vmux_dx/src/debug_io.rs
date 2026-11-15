use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::os::unix::fs::{OpenOptionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Waker;

use bevy_ecs::prelude::Component;
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

pub struct Guard {
    target: Option<Arc<TargetLock>>,
    temporary: Vec<Arc<Temporary>>,
    complete: Arc<AtomicBool>,
    wake: Waker,
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

pub trait IoJob: Job<Output = Result<Self::Data, Failure>> + Component {
    type Data: IoValue;
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
        }
    }
}

impl IoJob for SpawnProcess {
    type Data = ProcessValue;
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
        Ok(ProcessValue {
            _lease: Arc::new(Process(Mutex::new(child))),
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
        fs::write(&self.0, self.1).map_err(|error| Failure::message(error.to_string()))?;
        Ok(PendingValue {
            _lease: Arc::new(Pending(self.0)),
        })
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

pub struct Process(Mutex<std::process::Child>);

impl Drop for Process {
    fn drop(&mut self) {
        if let Ok(mut child) = self.0.lock() {
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
