//! OS clipboard read/write, isolated by platform.
//!
//! Uses absolute paths to system binaries (no `$PATH` lookup) to defend
//! against PATH-hijack on shared systems. Writes happen on a background
//! thread so the Bevy main thread never blocks on subprocess I/O.

use bevy::log::warn;

/// Asynchronously write `text` to the system clipboard. Returns immediately;
/// errors are logged.
pub fn write(text: String) {
    if text.is_empty() {
        return;
    }
    std::thread::spawn(move || write_blocking(&text));
}

/// Read text from the system clipboard, blocking. Returns None on any error.
pub fn read_blocking() -> Option<String> {
    read_inner()
}

#[cfg(target_os = "macos")]
fn write_blocking(text: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    match Command::new("/usr/bin/pbcopy")
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
        Err(e) => warn!("pbcopy failed: {e}"),
    }
}

#[cfg(target_os = "macos")]
fn read_inner() -> Option<String> {
    use std::process::Command;
    let output = Command::new("/usr/bin/pbpaste").output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(target_os = "linux")]
fn write_blocking(text: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    // Try wl-copy first (Wayland), fall back to xclip (X11).
    let candidates: &[(&str, &[&str])] = &[
        ("/usr/bin/wl-copy", &[]),
        ("/usr/bin/xclip", &["-selection", "clipboard"]),
    ];
    for (bin, args) in candidates {
        if std::path::Path::new(bin).exists() {
            match Command::new(bin).args(*args).stdin(Stdio::piped()).spawn() {
                Ok(mut child) => {
                    if let Some(stdin) = child.stdin.as_mut() {
                        let _ = stdin.write_all(text.as_bytes());
                    }
                    let _ = child.wait();
                    return;
                }
                Err(e) => warn!("{bin} failed: {e}"),
            }
        }
    }
    warn!("no clipboard helper found (need wl-copy or xclip)");
}

#[cfg(target_os = "linux")]
fn read_inner() -> Option<String> {
    use std::process::Command;
    let candidates: &[(&str, &[&str])] = &[
        ("/usr/bin/wl-paste", &[]),
        ("/usr/bin/xclip", &["-selection", "clipboard", "-o"]),
    ];
    for (bin, args) in candidates {
        if std::path::Path::new(bin).exists()
            && let Ok(output) = Command::new(bin).args(*args).output()
            && output.status.success()
        {
            return Some(String::from_utf8_lossy(&output.stdout).into_owned());
        }
    }
    None
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn write_blocking(_text: &str) {
    warn!("clipboard write not implemented on this platform");
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn read_inner() -> Option<String> {
    None
}
