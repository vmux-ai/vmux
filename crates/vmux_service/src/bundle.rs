use std::path::{Path, PathBuf};

/// True if `exe` lives at `<X>.app/Contents/MacOS/<binary>`.
pub fn is_bundled_path(exe: &Path) -> bool {
    bundle_root_for(exe).is_some()
}

/// Returns the `.app` root if `exe` lives inside a macOS bundle.
pub fn bundle_root_for(exe: &Path) -> Option<PathBuf> {
    let parent = exe.parent()?;
    if parent.file_name()?.to_str()? != "MacOS" {
        return None;
    }
    let contents = parent.parent()?;
    if contents.file_name()?.to_str()? != "Contents" {
        return None;
    }
    let app = contents.parent()?;
    if app.extension()?.to_str()? == "app" {
        Some(app.to_path_buf())
    } else {
        None
    }
}

/// Resolve bundle root for the currently-running executable.
pub fn current_bundle_root() -> Option<PathBuf> {
    bundle_root_for(&std::env::current_exe().ok()?)
}

/// True if the running process lives inside a `.app` bundle.
pub fn is_bundled() -> bool {
    current_bundle_root().is_some()
}

/// Plist filename of the embedded launchd agent (matches packaging/macos/ai.vmux.service.plist).
pub const EMBEDDED_AGENT_PLIST: &str = "ai.vmux.service.plist";
