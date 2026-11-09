use std::collections::HashMap;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::ffi::CString;
use std::io;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::prelude::*;
use bevy::winit::EventLoopProxyWrapper;

use super::repository::GitCommand;
use super::runtime::{GitRuntime, GitTask};

const PROVISION_TIMEOUT: Duration = Duration::from_secs(900);
const PROVISION_BACKLOG_LIMIT: usize = 8;

pub(super) struct RepositoryProvisionPlugin;

impl Plugin for RepositoryProvisionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GitRuntime>()
            .add_message::<RepositoryProvisionRequest>()
            .add_message::<RepositoryProvisionCompleted>()
            .add_systems(Update, (queue, start, finish).chain());
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
    fn destination(&self) -> &Path {
        match self {
            Self::Initialize { destination, .. } | Self::Clone { destination, .. } => destination,
        }
    }

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
    requesters: Vec<Entity>,
    destination: PathBuf,
    operation: RepositoryProvisionOperation,
    task: GitTask<Result<(), String>>,
}

#[derive(Component, Clone)]
struct PendingRepositoryProvision {
    requesters: Vec<Entity>,
    operation: RepositoryProvisionOperation,
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
        let (stdout, stderr, ok) = GitCommand::run_with_timeout(
            self.directory.path(),
            &["init", "--quiet", "--initial-branch", initial_branch],
            PROVISION_TIMEOUT,
        )
        .map_err(|error| error.0)?;
        self.finish(&stdout, &stderr, ok)
    }

    fn clone_from(self, remote: &str) -> Result<(), String> {
        let (stdout, stderr, ok) = GitCommand::run_with_timeout(
            self.directory.path(),
            &["clone", "--", remote, "."],
            PROVISION_TIMEOUT,
        )
        .map_err(|error| error.0)?;
        self.finish(&stdout, &stderr, ok)
    }

    fn finish(self, stdout: &str, stderr: &str, ok: bool) -> Result<(), String> {
        if !ok {
            return Err(GitCommand::error(stdout, stderr).0);
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

fn queue(
    mut requests: MessageReader<RepositoryProvisionRequest>,
    mut pending: Query<&mut PendingRepositoryProvision>,
    mut running: Query<&mut RepositoryProvisionTask>,
    mut completed: MessageWriter<RepositoryProvisionCompleted>,
    mut commands: Commands,
) {
    let mut queued = HashMap::<PathBuf, (Entity, PendingRepositoryProvision)>::new();
    for request in requests.read() {
        let destination =
            vmux_path::PathIdentity::lexical(request.operation.destination()).into_path_buf();
        let mut accepted = false;
        for mut existing in &mut pending {
            if vmux_path::PathIdentity::lexical(existing.operation.destination()).as_path()
                != destination.as_path()
            {
                continue;
            }
            if existing.operation == request.operation {
                existing.requesters.push(request.requester);
            } else {
                completed.write(RepositoryProvisionCompleted {
                    requester: request.requester,
                    result: Err("repository destination is already being provisioned".into()),
                });
            }
            accepted = true;
            break;
        }
        if accepted {
            continue;
        }
        for mut existing in &mut running {
            if existing.destination != destination {
                continue;
            }
            if existing.operation == request.operation {
                existing.requesters.push(request.requester);
            } else {
                completed.write(RepositoryProvisionCompleted {
                    requester: request.requester,
                    result: Err("repository destination is already being provisioned".into()),
                });
            }
            accepted = true;
            break;
        }
        if accepted {
            continue;
        }
        if let Some((entity, existing)) = queued.get_mut(&destination) {
            if existing.operation == request.operation {
                existing.requesters.push(request.requester);
                commands.entity(*entity).insert(existing.clone());
            } else {
                completed.write(RepositoryProvisionCompleted {
                    requester: request.requester,
                    result: Err("repository destination is already being provisioned".into()),
                });
            }
            continue;
        }
        if pending.iter().count() + running.iter().count() + queued.len() >= PROVISION_BACKLOG_LIMIT
        {
            completed.write(RepositoryProvisionCompleted {
                requester: request.requester,
                result: Err("repository provision queue is full".into()),
            });
            continue;
        }
        let provision = PendingRepositoryProvision {
            requesters: vec![request.requester],
            operation: request.operation.clone(),
        };
        let entity = commands.spawn(provision.clone()).id();
        queued.insert(destination, (entity, provision));
    }
}

fn start(
    pending: Query<(Entity, &PendingRepositoryProvision)>,
    runtime: Res<GitRuntime>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let Some((entity, pending)) = pending.iter().next() else {
        return;
    };
    let operation = pending.operation.clone();
    let destination = vmux_path::PathIdentity::lexical(operation.destination()).into_path_buf();
    let queued_operation = operation.clone();
    let Some(task) = runtime.spawn_operation(wake, move || operation.run()) else {
        return;
    };
    commands
        .entity(entity)
        .remove::<PendingRepositoryProvision>()
        .insert(RepositoryProvisionTask {
            requesters: pending.requesters.clone(),
            destination,
            operation: queued_operation,
            task,
        });
}

fn finish(
    mut tasks: Query<(Entity, &mut RepositoryProvisionTask)>,
    mut completed: MessageWriter<RepositoryProvisionCompleted>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = task.task.poll() else {
            continue;
        };
        let result = result.unwrap_or_else(|_| Err("repository provision worker panicked".into()));
        for requester in &task.requesters {
            completed.write(RepositoryProvisionCompleted {
                requester: *requester,
                result: result.clone(),
            });
        }
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_requests_share_one_pending_operation() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("repo");
        let operation = RepositoryProvisionOperation::Initialize {
            destination,
            initial_branch: "main".to_string(),
        };
        let first = Entity::from_bits(1);
        let second = Entity::from_bits(2);
        let mut app = App::new();
        app.add_message::<RepositoryProvisionRequest>()
            .add_message::<RepositoryProvisionCompleted>()
            .add_systems(Update, queue);
        app.world_mut().write_message(RepositoryProvisionRequest {
            requester: first,
            operation: operation.clone(),
        });
        app.world_mut().write_message(RepositoryProvisionRequest {
            requester: second,
            operation,
        });

        app.update();

        let mut pending = app.world_mut().query::<&PendingRepositoryProvision>();
        let pending = pending.iter(app.world()).collect::<Vec<_>>();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].requesters, vec![first, second]);
    }

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
