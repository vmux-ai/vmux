use std::fs::{Metadata, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::PathBuf;
use std::time::SystemTime;

use bevy_ecs::prelude::Component;
use vmux_ecs::{BlockingCancellation, Job};

use super::{PathChanged, PathReceipt};

#[derive(Clone)]
pub struct ObservePath {
    pub path: PathBuf,
}

impl Job for ObservePath {
    type Output = Result<PathFacts, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("path observation canceled".into());
        }
        let path = if self.path.is_absolute() {
            self.path
        } else {
            std::env::current_dir()
                .map_err(|error| error.to_string())?
                .join(self.path)
        };
        let metadata = match path.symlink_metadata() {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        Ok(PathFacts {
            path: vmux_path::PathIdentity::resolve(path)
                .as_path()
                .to_path_buf(),
            metadata,
        })
    }
}

#[derive(Component, Clone)]
pub struct PathFacts {
    pub path: PathBuf,
    pub metadata: Option<Metadata>,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct FileIdentity {
    device: u64,
    inode: u64,
    revision: Option<(u64, SystemTime)>,
}

impl FileIdentity {
    #[cfg(unix)]
    pub fn from_metadata(metadata: &Metadata) -> Result<Self, String> {
        use std::os::unix::fs::MetadataExt;
        let revision = if metadata.is_file() {
            Some((
                metadata.len(),
                metadata.modified().map_err(|error| error.to_string())?,
            ))
        } else {
            None
        };
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            revision,
        })
    }

    #[cfg(not(unix))]
    pub fn from_metadata(_metadata: &Metadata) -> Result<Self, String> {
        Err("native file identity is unsupported on this platform".into())
    }

    fn matches(&self, metadata: &Metadata) -> bool {
        Self::from_metadata(metadata).is_ok_and(|observed| &observed == self)
    }
}

#[derive(Clone)]
pub struct MovePath {
    pub source: PathBuf,
    pub target: PathBuf,
    pub expected: FileIdentity,
    pub receipt: PathReceipt,
}

impl Job for MovePath {
    type Output = Result<PathChanged, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("path move canceled".into());
        }
        if !self.source.is_absolute() || !self.target.is_absolute() {
            return Err("move paths must be absolute".into());
        }
        let metadata = self
            .source
            .symlink_metadata()
            .map_err(|error| error.to_string())?;
        if !self.expected.matches(&metadata) {
            return Err(format!("move source changed: {}", self.source.display()));
        }
        let parent = self.target.parent().ok_or("move target has no parent")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        if BlockingCancellation::canceled() {
            return Err("path move canceled".into());
        }
        self.exclusive()?;
        self.receipt.record();
        Ok(PathChanged(true))
    }
}

impl MovePath {
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "linux"))]
    fn exclusive(&self) -> Result<(), String> {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let source =
            CString::new(self.source.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
        let target =
            CString::new(self.target.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let result =
            unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                target.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux")))]
    fn exclusive(&self) -> Result<(), String> {
        Err("exclusive native moves are unsupported on this platform".into())
    }
}

#[derive(Clone)]
pub struct CopyFile {
    pub source: PathBuf,
    pub target: PathBuf,
    pub expected: FileIdentity,
}

impl Job for CopyFile {
    type Output = Result<BytesCopied, String>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Long
    }

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("file copy canceled".into());
        }
        if !self.source.is_absolute() || !self.target.is_absolute() {
            return Err("copy paths must be absolute".into());
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
        }
        let mut source = options
            .open(&self.source)
            .map_err(|error| error.to_string())?;
        let metadata = source.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_file() || !self.expected.matches(&metadata) {
            return Err("copy source is not the accepted regular file".into());
        }
        if metadata.len() > 8 * 1024 * 1024 * 1024 {
            return Err("file copy exceeds 8589934592 bytes".into());
        }
        let parent = self.target.parent().ok_or("copy target has no parent")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let mut target = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.target)
            .map_err(|error| error.to_string())?;
        let mut bytes = 0_u64;
        let mut buffer = [0; 65536];
        loop {
            if BlockingCancellation::canceled() {
                return Err("file copy canceled".into());
            }
            let count = source
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            bytes += count as u64;
            if bytes > 8 * 1024 * 1024 * 1024 {
                return Err("file copy exceeds 8589934592 bytes".into());
            }
            target
                .write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
        }
        if !self
            .expected
            .matches(&source.metadata().map_err(|error| error.to_string())?)
        {
            return Err("copy source changed before drain".into());
        }
        target
            .set_permissions(metadata.permissions())
            .map_err(|error| error.to_string())?;
        Ok(BytesCopied(bytes))
    }
}

#[derive(Component)]
pub struct BytesCopied(pub u64);

#[derive(Clone)]
pub struct CreateDirectory {
    pub path: PathBuf,
}

impl Job for CreateDirectory {
    type Output = Result<DirectoryCreated, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("directory creation canceled".into());
        }
        std::fs::create_dir_all(&self.path).map_err(|error| error.to_string())?;
        Ok(DirectoryCreated(self.path))
    }
}

#[derive(Component, Clone)]
pub struct DirectoryCreated(pub PathBuf);

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((
                bevy::app::TaskPoolPlugin::default(),
                crate::io::ObservationIoPlugin,
            ));
            Self {
                app,
                directory: tempfile::tempdir().unwrap(),
            }
        }

        fn run<J: Job>(&mut self, job: J) -> J::Output {
            let entity = self
                .app
                .world_mut()
                .spawn((
                    vmux_ecs::Operation::bundle("test.native.file")
                        .unwrap()
                        .running(),
                    vmux_ecs::OperationRetained,
                    vmux_ecs::JobRequest(job),
                ))
                .id();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                self.app.update();
                if let Some(result) = self
                    .app
                    .world_mut()
                    .entity_mut(entity)
                    .take::<vmux_ecs::JobResult<J>>()
                {
                    self.app.world_mut().despawn(entity);
                    return result.0.unwrap();
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "native file job did not return"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn file_copy_preserves_exact_bytes_and_refuses_occupied_targets_and_replaced_sources() {
        let mut fixture = Fixture::new();
        let source = fixture.directory.path().join("source");
        let target = fixture.directory.path().join("target");
        std::fs::write(&source, [0, 1, 255, 13, 10]).unwrap();
        let identity = FileIdentity::from_metadata(&source.metadata().unwrap()).unwrap();
        fixture
            .run(CopyFile {
                source: source.clone(),
                target: target.clone(),
                expected: identity.clone(),
            })
            .unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), [0, 1, 255, 13, 10]);
        assert!(
            fixture
                .run(CopyFile {
                    source: source.clone(),
                    target: target.clone(),
                    expected: identity.clone()
                })
                .is_err()
        );
        std::fs::rename(&source, fixture.directory.path().join("original")).unwrap();
        std::fs::write(&source, b"replacement").unwrap();
        let refused = fixture.directory.path().join("refused");
        assert!(
            fixture
                .run(CopyFile {
                    source,
                    target: refused.clone(),
                    expected: identity
                })
                .is_err()
        );
        assert!(!refused.exists());
        assert_eq!(std::fs::read(target).unwrap(), [0, 1, 255, 13, 10]);
    }

    #[test]
    fn exclusive_move_preserves_foreign_targets_and_rejects_replaced_sources() {
        let mut fixture = Fixture::new();
        let source = fixture.directory.path().join("source");
        let target = fixture.directory.path().join("target");
        std::fs::write(&source, b"original").unwrap();
        std::fs::write(&target, b"foreign").unwrap();
        let identity = FileIdentity::from_metadata(&source.metadata().unwrap()).unwrap();
        let receipt = PathReceipt::default();
        assert!(
            fixture
                .run(MovePath {
                    source: source.clone(),
                    target: target.clone(),
                    expected: identity.clone(),
                    receipt: receipt.clone()
                })
                .is_err()
        );
        assert!(!receipt.changed());
        assert_eq!(std::fs::read(&target).unwrap(), b"foreign");
        std::fs::remove_file(&target).unwrap();
        std::fs::rename(&source, fixture.directory.path().join("held")).unwrap();
        std::fs::write(&source, b"replacement").unwrap();
        assert!(
            fixture
                .run(MovePath {
                    source: source.clone(),
                    target: target.clone(),
                    expected: identity,
                    receipt
                })
                .is_err()
        );
        assert_eq!(std::fs::read(source).unwrap(), b"replacement");
        assert!(!target.exists());
    }
}
