#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use bevy::prelude::Component;
use vmux_editor::lsp::archive::ArchiveKind;
use vmux_editor::lsp::download::{self, RemoteArtifact};
pub(super) use vmux_editor::lsp::install_io::{Remove, Unlink};
use vmux_editor::lsp::store::Receipt;
use vmux_job::{BlockingCancellation, Job};
pub(super) use vmux_tool::io::{
    CloseTemporary as CloseStaging, CreateTemporary as CreateStaging,
    TemporaryDirectory as Staging, TemporaryTransfer as StagingTransfer,
};

#[derive(Clone)]
pub(super) struct Exists {
    pub(super) path: PathBuf,
}

impl Job for Exists {
    type Output = Result<FileExists, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("ACP file observation canceled".into());
        }
        match std::fs::metadata(self.path) {
            Ok(metadata) => Ok(FileExists(metadata.is_file())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(FileExists(false)),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[derive(Component)]
#[allow(unknown_lints, vmux_ecs_ownership)]
pub(super) struct FileExists(pub(super) bool);

#[derive(Clone)]
pub(super) struct Download {
    pub(super) artifact: RemoteArtifact,
    pub(super) staging: Staging,
    pub(super) filename: String,
    pub(super) progress: crossbeam_channel::Sender<Transferred>,
    pub(super) wake: Option<std::task::Waker>,
}

pub(super) struct Transferred {
    pub(super) bytes: u64,
    pub(super) total: Option<u64>,
}

impl Job for Download {
    type Output = Result<(), String>;

    fn lane(&self) -> vmux_job::blocking::Lane {
        vmux_job::blocking::Lane::Process
    }

    fn run(self) -> Self::Output {
        let mut updated = std::time::Instant::now() - std::time::Duration::from_millis(150);
        self.artifact.download_to(
            &self.staging.path().join(self.filename),
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
    pub(super) staging: Staging,
    pub(super) filename: String,
    pub(super) kind: ArchiveKind,
    pub(super) command: String,
}

impl Job for Extract {
    type Output = Result<(), String>;

    fn lane(&self) -> vmux_job::blocking::Lane {
        vmux_job::blocking::Lane::Process
    }

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("ACP archive extraction canceled".into());
        }
        self.kind.extract(
            &self.staging.path().join(self.filename),
            &self.staging.path().join("package"),
            &self.command,
        )
    }
}

#[derive(Clone)]
pub(super) struct MakeExecutable {
    pub(super) staging: Staging,
    pub(super) relative: PathBuf,
}

impl Job for MakeExecutable {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("ACP executable preparation canceled".into());
        }
        let path = self.staging.path().join("package").join(self.relative);
        let root = self
            .staging
            .path()
            .join("package")
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let path = path.canonicalize().map_err(|error| error.to_string())?;
        if !path.starts_with(root) {
            return Err("ACP executable escapes staging".into());
        }
        let metadata = std::fs::metadata(&path).map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err(format!("ACP executable is not a file: {}", path.display()));
        }
        #[cfg(unix)]
        {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(path, permissions).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct WriteReceipt {
    pub(super) directory: PathBuf,
    pub(super) receipt: Receipt,
    pub(super) staging: Option<Staging>,
}

impl Job for WriteReceipt {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("ACP receipt write canceled".into());
        }
        let _staging = self.staging;
        std::fs::create_dir_all(&self.directory).map_err(|error| error.to_string())?;
        self.receipt
            .write_to(&self.directory)
            .map_err(|error| error.to_string())
    }
}
