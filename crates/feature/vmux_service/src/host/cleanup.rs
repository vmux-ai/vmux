use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct LegacyRegistrations(Vec<PathBuf>);

impl LegacyRegistrations {
    pub fn current() -> std::io::Result<Self> {
        let Some(home) = std::env::var_os("HOME") else {
            return Ok(Self(Vec::new()));
        };
        Self::in_directory(&PathBuf::from(home).join("Library/LaunchAgents"))
    }

    pub fn in_directory(dir: &Path) -> std::io::Result<Self> {
        let mut paths = Vec::new();
        if !dir.exists() {
            return Ok(Self(paths));
        }
        for path in super::service_io::ServiceFsIo::read_dir_paths(dir)? {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if Self::label(name).is_some() {
                paths.push(path);
            }
        }
        Ok(Self(paths))
    }

    pub fn label(name: &str) -> Option<&str> {
        let stem = name.strip_suffix(".plist")?;
        let prefix = vmux_profile::Product::service_id();
        if stem == prefix || stem.starts_with(&format!("{prefix}.")) {
            Some(stem)
        } else {
            None
        }
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.0
    }

    pub fn cleanup(self) -> std::io::Result<usize> {
        #[cfg(target_os = "macos")]
        for path in &self.0 {
            if let Some(name) = path.file_name().and_then(|name| name.to_str())
                && let Some(label) = Self::label(name)
            {
                Self::bootout(label);
            }
        }
        self.remove_files()
    }

    pub fn remove_files(self) -> std::io::Result<usize> {
        let count = self.0.len();
        for path in self.0 {
            match super::service_io::ServiceFsIo::remove_file(&path) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(count)
    }

    #[cfg(target_os = "macos")]
    fn bootout(label: &str) {
        let uid = unsafe { libc::getuid() };
        let target = format!("gui/{uid}/{label}");
        let _ = super::service_io::ServiceFsIo::run_launchctl(&[
            std::ffi::OsStr::new("bootout"),
            std::ffi::OsStr::new(&target),
        ]);
    }
}
