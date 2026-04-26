use semver::Version;
use std::path::{Path, PathBuf};

use super::stage;

/// Derive the .app bundle path from the current executable.
/// e.g. /Applications/Vmux.app/Contents/MacOS/Vmux -> /Applications/Vmux.app
pub fn current_app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // Walk up: Vmux -> MacOS -> Contents -> Vmux.app
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;

    // Sanity check: must end with .app
    if bundle.extension().and_then(|e| e.to_str()) == Some("app") {
        Some(bundle.to_path_buf())
    } else {
        None
    }
}

/// Check for a staged update and apply it if valid.
/// This runs in main() before Bevy starts.
/// Returns `true` if the process should re-exec (update was applied).
pub fn apply_staged_update() -> bool {
    if !stage::has_staged_update() {
        return false;
    }

    let meta = match stage::read_meta() {
        Some(m) => m,
        None => return false,
    };

    let current_version = match Version::parse(env!("CARGO_PKG_VERSION")) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let staged_version = match Version::parse(&meta.version) {
        Ok(v) => v,
        Err(_) => {
            stage::cleanup();
            return false;
        }
    };

    // Don't apply if staged version is same or older
    if staged_version <= current_version {
        stage::cleanup();
        return false;
    }

    let bundle_path = match current_app_bundle() {
        Some(p) => p,
        None => {
            eprintln!("[updater] cannot determine .app bundle path, skipping update");
            return false;
        }
    };

    let staged_app = match stage::staged_dir() {
        Some(d) => d.join("Vmux.app"),
        None => return false,
    };

    if !staged_app.exists() {
        stage::cleanup();
        return false;
    }

    eprintln!(
        "[updater] applying update v{} -> v{}",
        current_version, staged_version
    );

    match swap_app_bundle(&bundle_path, &staged_app) {
        Ok(()) => {
            stage::cleanup();
            true
        }
        Err(e) => {
            eprintln!("[updater] failed to apply update: {e}");
            false
        }
    }
}

/// Atomically swap the current .app bundle with the staged one.
fn swap_app_bundle(current: &Path, staged: &Path) -> Result<(), Error> {
    let old = current.with_extension("app.old");

    // Step 1: rename current -> .old
    std::fs::rename(current, &old).map_err(|e| Error::Rename {
        from: current.to_path_buf(),
        to: old.clone(),
        source: e,
    })?;

    // Step 2: move staged -> current
    if let Err(e) = std::fs::rename(staged, current) {
        // Rollback: restore old
        eprintln!("[updater] move staged -> current failed, rolling back");
        let _ = std::fs::rename(&old, current);
        return Err(Error::Rename {
            from: staged.to_path_buf(),
            to: current.to_path_buf(),
            source: e,
        });
    }

    // Step 3: remove old (non-fatal)
    if let Err(e) = std::fs::remove_dir_all(&old) {
        eprintln!("[updater] warning: failed to remove old bundle: {e}");
    }

    Ok(())
}

/// Re-exec the current binary (replaces the current process).
pub fn re_exec() -> ! {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe().expect("cannot determine current executable");
    let args: Vec<String> = std::env::args().collect();
    let err = std::process::Command::new(&exe).args(&args[1..]).exec();
    // exec() only returns on error
    eprintln!("[updater] re-exec failed: {err}");
    std::process::exit(1);
}

#[derive(Debug)]
pub enum Error {
    Rename {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Rename { from, to, source } => {
                write!(
                    f,
                    "failed to rename {} -> {}: {}",
                    from.display(),
                    to.display(),
                    source
                )
            }
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[test]
    fn derive_bundle_path_from_exe() {
        let exe = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/Vmux");
        let macos = exe.parent().unwrap();
        let contents = macos.parent().unwrap();
        let bundle = contents.parent().unwrap();
        assert_eq!(bundle, PathBuf::from("/Applications/Vmux.app").as_path());
        assert_eq!(bundle.extension().unwrap(), "app");
    }

    #[test]
    fn non_app_bundle_returns_none_from_logic() {
        let exe = PathBuf::from("/usr/local/bin/vmux");
        let macos = exe.parent();
        let contents = macos.and_then(|p| p.parent());
        let bundle = contents.and_then(|p| p.parent());
        if let Some(b) = bundle {
            assert_ne!(b.extension().and_then(|e| e.to_str()), Some("app"));
        }
    }
}
