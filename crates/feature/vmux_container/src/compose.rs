use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use vmux_vm::{EngineClient, Executable, Settings, Status};

pub(crate) struct ComposeProject {
    directory: PathBuf,
    configuration: PathBuf,
}

impl ComposeProject {
    pub(crate) async fn select(initial: PathBuf) -> io::Result<Option<Self>> {
        let selected = rfd::AsyncFileDialog::new()
            .set_directory(initial)
            .pick_folder()
            .await;
        let Some(selected) = selected else {
            return Ok(None);
        };
        Ok(Some(Self::at(selected.path())?))
    }

    pub(crate) fn at(directory: &Path) -> io::Result<Self> {
        let directory = directory.canonicalize()?;
        for name in [
            "compose.yaml",
            "compose.yml",
            "docker-compose.yaml",
            "docker-compose.yml",
        ] {
            let configuration = directory.join(name);
            if configuration.is_file() {
                return Ok(Self {
                    directory,
                    configuration,
                });
            }
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "container-project-invalid",
        ))
    }

    pub(crate) fn directory(&self) -> PathBuf {
        self.directory.clone()
    }

    pub(crate) fn start(&self, mut settings: Settings, helper: &Path) -> io::Result<Status> {
        settings.validate()?;
        if !self.directory.starts_with(&settings.shared_directory) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "container-project-not-shared",
            ));
        }
        let status = EngineClient::new(settings).start(helper)?;
        let output = Command::new(Executable::find("docker")?)
            .arg("--host")
            .arg(format!("unix://{}", status.socket.display()))
            .arg("compose")
            .arg("--project-directory")
            .arg(&self.directory)
            .arg("-f")
            .arg(&self.configuration)
            .arg("up")
            .arg("-d")
            .env_remove("DOCKER_CONTEXT")
            .env_remove("DOCKER_HOST")
            .stdin(Stdio::null())
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }
        Ok(status)
    }
}
