use std::path::{Path, PathBuf};

const LABEL_PREFIX: &str = "ai.vmux.service";

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
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
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
        if stem == LABEL_PREFIX || stem.starts_with(&format!("{LABEL_PREFIX}.")) {
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
            match std::fs::remove_file(path) {
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
        let _ = std::process::Command::new("launchctl")
            .args(["bootout", &format!("gui/{uid}/{label}")])
            .status();
    }
}
