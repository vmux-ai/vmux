use super::archive::ArchiveKind;
use super::download::{self, RemoteArtifact};
use super::package_path::{PackageName, PackagePath};
use super::store::{PackageStore, Receipt};
use std::path::{Path, PathBuf};
use vmux_ecs::{BlockingCancellation, Job};
use vmux_tool::io::TemporaryDirectory;

#[derive(Clone)]
pub(super) struct Prepare {
    pub staging: TemporaryDirectory,
}

impl Job for Prepare {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("LSP package preparation canceled".into());
        }
        std::fs::create_dir_all(self.staging.path().join("package"))
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone)]
pub(super) struct Download {
    pub artifact: RemoteArtifact,
    pub staging: TemporaryDirectory,
    pub file: PackagePath,
    pub progress: crossbeam_channel::Sender<Transferred>,
    pub wake: Option<std::task::Waker>,
}

pub(super) struct Transferred {
    pub bytes: u64,
    pub total: Option<u64>,
}

impl Job for Download {
    type Output = Result<(), String>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Process
    }

    fn run(self) -> Self::Output {
        let mut updated = std::time::Instant::now() - std::time::Duration::from_millis(150);
        self.artifact.download_to(
            &self.staging.path().join(self.file.as_path()),
            download::PACKAGE_MAX_BYTES,
            |bytes, total| {
                if updated.elapsed() >= std::time::Duration::from_millis(150)
                    || total == Some(bytes)
                {
                    let _ = self.progress.try_send(Transferred { bytes, total });
                    if let Some(wake) = &self.wake {
                        wake.wake_by_ref();
                    }
                    updated = std::time::Instant::now();
                }
            },
        )
    }
}

#[derive(Clone)]
pub(super) struct Extract {
    pub staging: TemporaryDirectory,
    pub file: PackagePath,
    pub binary: PackagePath,
}

impl Job for Extract {
    type Output = Result<(), String>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Process
    }

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("LSP extraction canceled".into());
        }
        ArchiveKind::for_file(self.file.as_str()).extract(
            &self.staging.path().join(self.file.as_path()),
            &self.staging.path().join("package"),
            self.binary.as_str(),
        )
    }
}

#[derive(Clone)]
pub(super) struct MakeExecutable {
    pub staging: TemporaryDirectory,
    pub file: PackagePath,
}

impl Job for MakeExecutable {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("LSP executable preparation canceled".into());
        }
        let root = self
            .staging
            .path()
            .join("package")
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let path = root
            .join(self.file.as_path())
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !path.starts_with(root) {
            return Err("LSP binary escapes staging".into());
        }
        let metadata = std::fs::metadata(&path).map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err(format!(
                "LSP binary is not a regular file: {}",
                path.display()
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(path, permissions).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct WriteReceipt {
    pub staging: TemporaryDirectory,
    pub receipt: Receipt,
}

impl Job for WriteReceipt {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("LSP receipt write canceled".into());
        }
        self.receipt
            .write_to(&self.staging.path().join("package"))
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone)]
pub(super) struct Link {
    pub store: PackageStore,
    pub package: PackageName,
    pub file: PackagePath,
    pub name: PackageName,
}

impl Job for Link {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("LSP bin linking canceled".into());
        }
        self.store
            .link_bin(&self.package, &self.file, &self.name)
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone)]
pub struct Remove {
    pub store: PackageStore,
    pub name: PackageName,
}

impl Job for Remove {
    type Output = Result<(), String>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Long
    }

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("package removal canceled".into());
        }
        self.store
            .remove_package_dir(&self.name)
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone)]
pub struct Unlink {
    pub path: PathBuf,
    pub expected: PathBuf,
}

impl Unlink {
    fn target(path: &Path) -> PathBuf {
        let Some(parent) = path.parent() else {
            return path.to_path_buf();
        };
        let parent = vmux_path::PathIdentity::resolve(parent)
            .as_path()
            .to_path_buf();
        match path.file_name() {
            Some(name) => parent.join(name),
            None => parent,
        }
    }
}

impl Job for Unlink {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("package bin unlink canceled".into());
        }
        let metadata = match self.path.symlink_metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.to_string()),
        };
        if !metadata.file_type().is_symlink() {
            return Ok(());
        }
        let target = std::fs::read_link(&self.path).map_err(|error| error.to_string())?;
        let target = if target.is_absolute() {
            target
        } else {
            self.path.parent().unwrap_or(Path::new("/")).join(target)
        };
        if Self::target(&target) != Self::target(&self.expected) {
            return Ok(());
        }
        match std::fs::remove_file(self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}
