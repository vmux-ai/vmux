use std::io;

pub trait AppLauncher {
    fn launch(&self, app_name: &str) -> io::Result<()>;
}

pub fn run<L: AppLauncher>(launcher: &L) -> io::Result<()> {
    launcher.launch("Vmux")
}

pub struct OpenAppLauncher;

impl AppLauncher for OpenAppLauncher {
    #[cfg(target_os = "macos")]
    fn launch(&self, app_name: &str) -> io::Result<()> {
        let status = std::process::Command::new("open")
            .arg("-a")
            .arg(app_name)
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "open -a {app_name} exited with {status}"
            )))
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn launch(&self, _app_name: &str) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "launching the Vmux app is not supported on this platform yet",
        ))
    }
}

#[cfg(test)]
#[path = "open.test.rs"]
mod tests;
