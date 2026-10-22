#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::ffi::CString;
use std::io;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};

pub(super) struct RepositoryProvisionPlugin;

impl Plugin for RepositoryProvisionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RepositoryProvisionRequest>()
            .add_message::<RepositoryProvisionCompleted>()
            .add_systems(Update, (start, finish).chain());
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepositoryProvisionOperation {
    Initialize {
        destination: PathBuf,
        initial_branch: String,
    },
    Clone {
        remote: String,
        destination: PathBuf,
    },
}

impl RepositoryProvisionOperation {
    fn run(self) -> Result<(), String> {
        match self {
            Self::Initialize {
                destination,
                initial_branch,
            } => RepositoryStaging::for_destination(&destination)?.initialize(&initial_branch),
            Self::Clone {
                remote,
                destination,
            } => RepositoryStaging::for_destination(&destination)?.clone_from(&remote),
        }
    }
}

#[derive(Message, Clone, Debug)]
pub struct RepositoryProvisionRequest {
    pub requester: Entity,
    pub operation: RepositoryProvisionOperation,
}

#[derive(Message, Clone, Debug)]
pub struct RepositoryProvisionCompleted {
    pub requester: Entity,
    pub result: Result<(), String>,
}

#[derive(Component)]
struct RepositoryProvisionTask {
    requester: Entity,
    task: Task<Result<(), String>>,
}

struct RepositoryStaging {
    destination: PathBuf,
    directory: tempfile::TempDir,
}

impl RepositoryStaging {
    fn for_destination(destination: &Path) -> Result<Self, String> {
        if destination.exists() {
            return Err("repository destination already exists".to_string());
        }
        let parent = destination
            .parent()
            .ok_or_else(|| "repository destination has no parent".to_string())?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let directory = tempfile::Builder::new()
            .prefix(".vmux-project-")
            .tempdir_in(parent)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            destination: destination.to_path_buf(),
            directory,
        })
    }

    fn initialize(self, initial_branch: &str) -> Result<(), String> {
        let output = Command::new("git")
            .args(["init", "--quiet", "--initial-branch", initial_branch])
            .current_dir(self.directory.path())
            .output()
            .map_err(|error| error.to_string())?;
        self.finish(output)
    }

    fn clone_from(self, remote: &str) -> Result<(), String> {
        let output = Command::new("git")
            .arg("clone")
            .arg("--")
            .arg(remote)
            .arg(self.directory.path())
            .output()
            .map_err(|error| error.to_string())?;
        self.finish(output)
    }

    fn finish(self, output: std::process::Output) -> Result<(), String> {
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        self.move_into_place().map_err(|error| error.to_string())?;
        Ok(())
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn path_argument(path: &Path) -> io::Result<CString> {
        CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains a null byte"))
    }

    #[cfg(target_os = "macos")]
    fn move_into_place(&self) -> io::Result<()> {
        let source = Self::path_argument(self.directory.path())?;
        let destination = Self::path_argument(&self.destination)?;
        let result =
            unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
        if result == 0 {
            return Ok(());
        }
        Err(io::Error::last_os_error())
    }

    #[cfg(target_os = "linux")]
    fn move_into_place(&self) -> io::Result<()> {
        let source = Self::path_argument(self.directory.path())?;
        let destination = Self::path_argument(&self.destination)?;
        let result = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                destination.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if result == 0 {
            return Ok(());
        }
        Err(io::Error::last_os_error())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn move_into_place(&self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "atomic repository provisioning is unsupported on this platform",
        ))
    }
}

fn start(
    mut requests: MessageReader<RepositoryProvisionRequest>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let operation = request.operation.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = operation.run();
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            result
        });
        commands.spawn(RepositoryProvisionTask {
            requester: request.requester,
            task,
        });
    }
}

fn finish(
    mut tasks: Query<(Entity, &mut RepositoryProvisionTask)>,
    mut completed: MessageWriter<RepositoryProvisionCompleted>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        completed.write(RepositoryProvisionCompleted {
            requester: task.requester,
            result,
        });
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_clone_removes_staging_and_destination() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("repo");
        let operation = RepositoryProvisionOperation::Clone {
            remote: root.path().join("missing").to_string_lossy().into_owned(),
            destination: destination.clone(),
        };

        assert!(operation.run().is_err());
        assert!(!destination.exists());
        assert!(std::fs::read_dir(root.path()).unwrap().next().is_none());
    }

    #[test]
    fn initialize_moves_a_complete_repository_into_place() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("repo");
        let operation = RepositoryProvisionOperation::Initialize {
            destination: destination.clone(),
            initial_branch: "main".to_string(),
        };

        operation.run().unwrap();

        assert!(destination.join(".git").is_dir());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn destination_created_during_provision_is_not_replaced() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("repo");
        let staging = RepositoryStaging::for_destination(&destination).unwrap();
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(destination.join("existing"), "keep").unwrap();

        assert!(staging.move_into_place().is_err());
        assert_eq!(
            std::fs::read_to_string(destination.join("existing")).unwrap(),
            "keep"
        );
    }
}
