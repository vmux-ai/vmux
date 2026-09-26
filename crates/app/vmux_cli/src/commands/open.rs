use std::io;

use bevy_ecs::prelude::Component;

#[derive(Clone, Component, Copy, Debug)]
pub struct OpenRequest;

impl OpenRequest {
    pub(crate) fn execute(&self) -> io::Result<()> {
        #[cfg(target_os = "macos")]
        {
            let status = std::process::Command::new("open")
                .arg("-a")
                .arg("Vmux")
                .status()?;
            if status.success() {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "open -a Vmux exited with {status}"
                )))
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "launching the Vmux app is not supported on this platform yet",
            ))
        }
    }
}
