use std::path::{Path, PathBuf};

const LABEL_PREFIX: &str = "ai.vmux.service";

pub fn launch_agents_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/LaunchAgents"))
}

pub fn label_from_filename(name: &str) -> Option<&str> {
    let stem = name.strip_suffix(".plist")?;
    if stem == LABEL_PREFIX || stem.starts_with(&format!("{LABEL_PREFIX}.")) {
        Some(stem)
    } else {
        None
    }
}

pub fn find_legacy_plists_in(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if label_from_filename(name).is_some() {
            out.push(path);
        }
    }
    Ok(out)
}

pub fn remove_plist_files(paths: &[PathBuf]) -> std::io::Result<()> {
    for path in paths {
        match std::fs::remove_file(path) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn bootout_label(label: &str) {
    let uid = unsafe { libc::getuid() };
    let _ = std::process::Command::new("launchctl")
        .args(["bootout", &format!("gui/{uid}/{label}")])
        .status();
}

pub fn cleanup_legacy_registrations() -> std::io::Result<usize> {
    let Some(dir) = launch_agents_dir() else {
        return Ok(0);
    };
    let paths = find_legacy_plists_in(&dir)?;
    #[cfg(target_os = "macos")]
    for path in &paths {
        if let Some(name) = path.file_name().and_then(|s| s.to_str())
            && let Some(label) = label_from_filename(name)
        {
            bootout_label(label);
        }
    }
    let count = paths.len();
    remove_plist_files(&paths)?;
    Ok(count)
}
