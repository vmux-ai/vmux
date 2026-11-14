use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::os::unix::fs::symlink;
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

#[derive(Component, Clone)]
#[component(immutable)]
pub enum Effect {
    Command {
        environment: Environment,
        cwd: PathBuf,
        program: OsString,
        arguments: Vec<OsString>,
    },
    Spawn {
        environment: Environment,
        cwd: PathBuf,
        program: OsString,
        arguments: Vec<OsString>,
    },
    Run {
        environment: Environment,
        cwd: PathBuf,
        program: OsString,
        arguments: Vec<OsString>,
    },
    Read(PathBuf),
    Hash(PathBuf),
    Inspect(PathBuf),
    Directory(PathBuf),
    Create(PathBuf),
    Copy(PathBuf, PathBuf),
    Remove(PathBuf),
    Link(PathBuf, PathBuf),
    Write(PathBuf, Vec<u8>),
    Rename(PathBuf, PathBuf),
    Temporary,
}

impl Effect {
    pub fn command(
        environment: &Environment,
        cwd: &Path,
        program: OsString,
        arguments: Vec<OsString>,
    ) -> Self {
        Self::Command {
            environment: environment.clone(),
            cwd: cwd.to_owned(),
            program,
            arguments,
        }
    }

    pub fn spawn(
        environment: &Environment,
        cwd: &Path,
        program: OsString,
        arguments: Vec<OsString>,
    ) -> Self {
        Self::Spawn {
            environment: environment.clone(),
            cwd: cwd.to_owned(),
            program,
            arguments,
        }
    }

    pub fn process(
        environment: &Environment,
        cwd: &Path,
        program: OsString,
        arguments: Vec<OsString>,
    ) -> Self {
        Self::Run {
            environment: environment.clone(),
            cwd: cwd.to_owned(),
            program,
            arguments,
        }
    }
}

impl Job for Effect {
    type Output = Result<Observation, Failure>;

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Debug IO canceled"));
        }
        match self {
            Self::Run {
                environment,
                cwd,
                program,
                arguments,
            } => {
                let mut command = environment.command(program, &cwd);
                command.args(arguments);
                Execution::new(command).run().map_err(Failure::from_error)?;
                Ok(Observation::Done)
            }
            Self::Command {
                environment,
                cwd,
                program,
                arguments,
            } => {
                let mut command = environment.command(program, &cwd);
                command.args(arguments);
                let output = Execution::new(command)
                    .output_any(None)
                    .map_err(Failure::from_error)?;
                Ok(Observation::Output(output))
            }
            Self::Spawn {
                environment,
                cwd,
                program,
                arguments,
            } => {
                let mut command = environment.command(program, &cwd);
                command.args(arguments).process_group(0);
                let child = command
                    .spawn()
                    .map_err(|error| Failure::message(error.to_string()))?;
                Ok(Observation::Process {
                    _lease: Arc::new(Process(Mutex::new(child))),
                })
            }
            Self::Read(path) => {
                let mut bytes = Vec::new();
                fs::File::open(path)
                    .map_err(|error| Failure::message(error.to_string()))?
                    .take(1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|error| Failure::message(error.to_string()))?;
                if bytes.len() > 1024 * 1024 {
                    return Err(Failure::message("Debug metadata exceeds 1 MiB"));
                }
                Ok(Observation::Bytes(bytes))
            }
            Self::Hash(path) => {
                let mut file =
                    fs::File::open(path).map_err(|error| Failure::message(error.to_string()))?;
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
                Ok(Observation::Digest(format!("{:x}", hash.finalize())))
            }
            Self::Inspect(path) => match fs::symlink_metadata(path) {
                Ok(metadata) => Ok(Observation::Metadata(Some(metadata))),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    Ok(Observation::Metadata(None))
                }
                Err(error) => Err(Failure::message(error.to_string())),
            },
            Self::Directory(path) => {
                let mut entries = Vec::new();
                for entry in
                    fs::read_dir(path).map_err(|error| Failure::message(error.to_string()))?
                {
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
                Ok(Observation::Entries(entries))
            }
            Self::Create(path) => {
                fs::create_dir_all(path).map_err(|error| Failure::message(error.to_string()))?;
                Ok(Observation::Done)
            }
            Self::Copy(source, destination) => {
                if source != destination
                    && (!destination.exists()
                        || source
                            .canonicalize()
                            .map_err(|error| Failure::message(error.to_string()))?
                            != destination
                                .canonicalize()
                                .map_err(|error| Failure::message(error.to_string()))?)
                {
                    fs::copy(source, destination)
                        .map_err(|error| Failure::message(error.to_string()))?;
                }
                Ok(Observation::Done)
            }
            Self::Remove(path) => {
                fs::remove_file(path).map_err(|error| Failure::message(error.to_string()))?;
                Ok(Observation::Done)
            }
            Self::Link(source, destination) => {
                symlink(source, destination)
                    .map_err(|error| Failure::message(error.to_string()))?;
                Ok(Observation::Done)
            }
            Self::Write(path, bytes) => {
                fs::write(&path, bytes).map_err(|error| Failure::message(error.to_string()))?;
                Ok(Observation::Pending {
                    _lease: Arc::new(Pending(path)),
                })
            }
            Self::Rename(source, destination) => {
                fs::rename(source, destination)
                    .map_err(|error| Failure::message(error.to_string()))?;
                Ok(Observation::Done)
            }
            Self::Temporary => Temporary::create(&std::env::temp_dir(), "vmux-local-codesign")
                .map(|directory| Observation::Temporary(Arc::new(directory)))
                .map_err(|error| Failure::message(error.to_string())),
        }
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

#[derive(Component, Clone)]
pub enum Observation {
    Output(std::process::Output),
    Bytes(Vec<u8>),
    Digest(String),
    Metadata(Option<fs::Metadata>),
    Entries(Vec<(PathBuf, fs::Metadata)>),
    Temporary(Arc<Temporary>),
    Process { _lease: Arc<Process> },
    Pending { _lease: Arc<Pending> },
    Done,
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
            job: Effect::Write(path.clone(), b"temporary result".to_vec()),
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
