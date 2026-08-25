use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::paths::ServicePaths;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonBinary(PathBuf);

impl DaemonBinary {
    pub fn current() -> std::io::Result<Self> {
        Ok(Self::beside(&std::env::current_exe()?))
    }

    pub fn beside(exe: &Path) -> Self {
        if matches!(
            exe.file_name().and_then(|n| n.to_str()),
            Some("vmux_service" | "Vmux Service")
        ) {
            return Self(exe.to_path_buf());
        }

        if let Some(root) = crate::bundle::bundle_root_for(exe) {
            return Self(
                root.join("Contents")
                    .join("Library")
                    .join("LoginItems")
                    .join("Vmux Service.app")
                    .join("Contents")
                    .join("MacOS")
                    .join("Vmux Service"),
            );
        }

        let mut path = exe.to_path_buf();
        path.pop();
        path.push("vmux_service");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn into_path(self) -> PathBuf {
        self.0
    }

    pub fn identity(&self) -> std::io::Result<DaemonIdentity> {
        DaemonIdentity::of(&self.0)
    }

    pub fn record_identity(&self) -> std::io::Result<()> {
        std::fs::write(
            ServicePaths::current().identity(),
            self.identity()?.as_str(),
        )
    }
}

#[derive(Clone, Debug)]
pub struct DaemonIdentity(String);

impl DaemonIdentity {
    pub fn of(path: &Path) -> std::io::Result<Self> {
        let path = std::fs::canonicalize(path)?;
        let metadata = std::fs::metadata(&path)?;
        let modified = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Ok(Self(format!(
            "{}\n{}\n{modified}",
            path.display(),
            metadata.len()
        )))
    }

    pub fn recorded(text: &str) -> Self {
        Self(text.to_string())
    }

    pub fn matches(&self, other: &Self) -> bool {
        self.0.trim() == other.0.trim()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn executable_identity_changes_when_file_changes() {
        let path = std::env::temp_dir().join(format!("vmux-identity-test-{}", std::process::id()));
        {
            let mut file = std::fs::File::create(&path).expect("create identity test file");
            file.write_all(b"old").expect("write old identity bytes");
        }
        let old_identity = DaemonIdentity::of(&path).expect("old identity");

        std::thread::sleep(std::time::Duration::from_millis(2));
        {
            let mut file = std::fs::File::create(&path).expect("rewrite identity test file");
            file.write_all(b"newer").expect("write new identity bytes");
        }
        let new_identity = DaemonIdentity::of(&path).expect("new identity");
        let _ = std::fs::remove_file(&path);

        assert!(!old_identity.matches(&new_identity));
    }

    #[test]
    fn bundled_main_app_resolves_named_service_app_executable() {
        let exe = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/Vmux");

        assert_eq!(
            DaemonBinary::beside(&exe).path(),
            Path::new(
                "/Applications/Vmux.app/Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service"
            )
        );
    }

    #[test]
    fn bundled_service_app_resolves_to_self() {
        let exe = PathBuf::from(
            "/Applications/Vmux.app/Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service",
        );

        assert_eq!(DaemonBinary::beside(&exe).path(), exe);
    }

    #[test]
    fn unbundled_debug_app_resolves_legacy_service_binary() {
        let exe = PathBuf::from("/Users/x/repo/target/debug/vmux_desktop");

        assert_eq!(
            DaemonBinary::beside(&exe).path(),
            Path::new("/Users/x/repo/target/debug/vmux_service")
        );
    }

    #[test]
    fn service_identity_match_requires_exact_record() {
        assert!(
            DaemonIdentity::recorded("a\n1\n2\n").matches(&DaemonIdentity::recorded("a\n1\n2"))
        );
        assert!(!DaemonIdentity::recorded("a\n1\n2").matches(&DaemonIdentity::recorded("a\n1\n3")));
    }
}
