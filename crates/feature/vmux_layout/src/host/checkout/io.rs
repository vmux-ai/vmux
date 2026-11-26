use bevy::prelude::*;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::sync::Arc;
use vmux_git::worktree::{CheckoutInfo, WorktreeCommand};
use vmux_job::blocking::Lane;
use vmux_job::{Job, JobPlugin};

pub(super) struct IoPlugin;

impl Plugin for IoPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JobPlugin::<Identify>::default(),
            JobPlugin::<Create>::default(),
            JobPlugin::<Canonicalize>::default(),
            JobPlugin::<Inspect>::default(),
            JobPlugin::<CreateDirectory>::default(),
            JobPlugin::<Acquire>::default(),
            JobPlugin::<WorktreeCommand>::default(),
            JobPlugin::<WriteJournal>::default(),
            JobPlugin::<RemoveJournal>::default(),
            JobPlugin::<CanonicalParent>::default(),
            JobPlugin::<DirectoryPaths>::default(),
            JobPlugin::<ReadFile>::default(),
            JobPlugin::<References>::default(),
        ));
    }
}

#[derive(Clone)]
pub(super) struct References {
    pub directory: PathBuf,
    pub arguments: Vec<String>,
    pub guard: Arc<File>,
}

impl Job for References {
    type Output = Result<std::process::Output, String>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        let process = ReferenceProcess::new(&self.directory, &self.arguments)?;
        let output = process.output();
        drop(self.guard);
        output
    }
}

struct ReferenceProcess {
    child: std::process::Child,
    stdout: File,
    stderr: File,
}

impl ReferenceProcess {
    fn new(directory: &std::path::Path, arguments: &[String]) -> Result<Self, String> {
        let stdout = tempfile::tempfile().map_err(|error| error.to_string())?;
        let stderr = tempfile::tempfile().map_err(|error| error.to_string())?;
        let mut command = std::process::Command::new("lsof");
        command
            .current_dir(directory)
            .args(arguments)
            .stdin(std::process::Stdio::null())
            .stdout(stdout.try_clone().map_err(|error| error.to_string())?)
            .stderr(stderr.try_clone().map_err(|error| error.to_string())?)
            .process_group(0);
        let child = command.spawn().map_err(|error| error.to_string())?;
        Ok(Self {
            child,
            stdout,
            stderr,
        })
    }

    fn output(mut self) -> Result<std::process::Output, String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let status = loop {
            let stdout = self
                .stdout
                .metadata()
                .map_err(|error| error.to_string())?
                .len();
            let stderr = self
                .stderr
                .metadata()
                .map_err(|error| error.to_string())?
                .len();
            if stdout > 2 * 1024 * 1024 || stderr > 2 * 1024 * 1024 {
                return Err("native reference output exceeded its limit".into());
            }
            if let Some(status) = self.child.try_wait().map_err(|error| error.to_string())? {
                break status;
            }
            if std::time::Instant::now() >= deadline {
                return Err("native reference inspection timed out".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        self.stdout
            .seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        self.stderr
            .seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        (&mut self.stdout)
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut stdout)
            .map_err(|error| error.to_string())?;
        (&mut self.stderr)
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut stderr)
            .map_err(|error| error.to_string())?;
        if stdout.len() > 2 * 1024 * 1024 || stderr.len() > 2 * 1024 * 1024 {
            return Err("native reference output exceeded its limit".into());
        }
        Ok(std::process::Output {
            status,
            stdout,
            stderr,
        })
    }
}

impl Drop for ReferenceProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
            let _ = self.child.wait();
        }
    }
}

#[derive(Clone)]
pub(crate) struct Identify(pub PathBuf);

impl Job for Identify {
    type Output = Result<CheckoutInfo, String>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        CheckoutInfo::try_from(self.0.as_path()).map_err(|error| error.0)
    }
}

#[derive(Clone)]
pub(super) struct Create {
    pub command: WorktreeCommand,
    pub destination: PathBuf,
}

pub(super) struct Created {
    pub command: Result<(String, String, bool), String>,
    pub directory: Result<(Arc<File>, Metadata), String>,
}

impl Job for Create {
    type Output = Created;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        let guard = self.command.guard.clone();
        let command = self.command.run().map_err(|error| error.0);
        let directory = File::open(self.destination)
            .and_then(|file| file.metadata().map(|metadata| (Arc::new(file), metadata)))
            .map_err(|error| error.to_string());
        drop(guard);
        Created { command, directory }
    }
}

#[derive(Clone)]
pub(crate) struct Canonicalize(pub PathBuf);

impl Job for Canonicalize {
    type Output = Result<PathBuf, String>;

    fn run(self) -> Self::Output {
        self.0.canonicalize().map_err(|error| error.to_string())
    }
}

#[derive(Clone)]
pub(crate) struct Inspect(pub PathBuf);

#[derive(Clone)]
pub(super) struct CanonicalParent(pub PathBuf);

impl Job for CanonicalParent {
    type Output = Result<PathBuf, String>;

    fn run(self) -> Self::Output {
        let parent = self
            .0
            .parent()
            .ok_or_else(|| "worktree path has no parent".to_string())?;
        let name = self
            .0
            .file_name()
            .ok_or_else(|| "worktree path has no file name".to_string())?;
        let parent = parent.canonicalize().map_err(|error| error.to_string())?;
        Ok(parent.join(name))
    }
}

impl Job for Inspect {
    type Output = Result<Option<Metadata>, String>;

    fn run(self) -> Self::Output {
        match fs::symlink_metadata(self.0) {
            Ok(metadata) => Ok(Some(metadata)),
            Err(error)
                if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) =>
            {
                Ok(None)
            }
            Err(error) => Err(error.to_string()),
        }
    }
}

#[derive(Clone)]
pub(super) struct CreateDirectory(pub PathBuf);

impl Job for CreateDirectory {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        fs::create_dir_all(self.0).map_err(|error| error.to_string())
    }
}

#[derive(Clone)]
pub(super) struct Acquire(pub PathBuf);

impl Job for Acquire {
    type Output = Result<Option<Arc<File>>, String>;

    fn run(self) -> Self::Output {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(self.0.join("vmux-worktrees.lock"))
            .map_err(|error| error.to_string())?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == ErrorKind::WouldBlock {
                return Ok(None);
            }
            return Err(error.to_string());
        }
        Ok(Some(Arc::new(file)))
    }
}

#[derive(Clone)]
pub(super) struct WriteJournal {
    pub path: PathBuf,
    pub staging: PathBuf,
    pub bytes: Vec<u8>,
    pub guard: Option<Arc<File>>,
}

impl Job for WriteJournal {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.staging)
            .map_err(|error| error.to_string())?;
        let result = match file.write_all(&self.bytes) {
            Ok(()) => file.sync_all(),
            Err(error) => Err(error),
        };
        drop(file);
        if let Err(error) = result {
            let _ = fs::remove_file(&self.staging);
            return Err(error.to_string());
        }
        if let Err(error) = fs::rename(&self.staging, &self.path) {
            let _ = fs::remove_file(&self.staging);
            return Err(error.to_string());
        }
        let directory = self
            .path
            .parent()
            .ok_or_else(|| "journal has no parent directory".to_string())?;
        File::open(directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
        drop(self.guard);
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct RemoveJournal(pub PathBuf);

impl Job for RemoveJournal {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        match fs::remove_file(self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[derive(Clone)]
pub(super) struct DirectoryPaths(pub PathBuf);

impl Job for DirectoryPaths {
    type Output = Result<Option<Vec<PathBuf>>, String>;

    fn run(self) -> Self::Output {
        let entries = match fs::read_dir(self.0) {
            Ok(entries) => entries,
            Err(error)
                if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error.to_string()),
        };
        let mut paths = Vec::new();
        for entry in entries {
            paths.push(entry.map_err(|error| error.to_string())?.path());
        }
        Ok(Some(paths))
    }
}

#[derive(Clone)]
pub(super) struct ReadFile {
    pub path: PathBuf,
    pub max_bytes: usize,
}

impl Job for ReadFile {
    type Output = Result<Vec<u8>, String>;

    fn run(self) -> Self::Output {
        let file = File::open(self.path).map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        file.take(self.max_bytes.saturating_add(1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        Ok(bytes)
    }
}
