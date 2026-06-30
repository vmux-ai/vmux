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

/// Whether the system clipboard currently holds PNG image data.
///
/// On ⌘V in a terminal this decides whether to forward `Ctrl+V` (`0x16`) so the
/// focused agent CLI grabs the image from the pasteboard itself, instead of a
/// text paste. Scoped to PNG so it stays consistent with [`read_image_png`] (the
/// Vibe/boot-draft paths extract PNG); returns `false` otherwise.
pub fn has_image() -> bool {
    has_image_inner()
}

#[cfg(target_os = "macos")]
fn has_image_inner() -> bool {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG};
    use objc2_foundation::NSArray;
    let png_type = unsafe { NSArray::from_slice(&[NSPasteboardTypePNG]) };
    NSPasteboard::generalPasteboard()
        .availableTypeFromArray(&png_type)
        .is_some()
}

#[cfg(not(target_os = "macos"))]
fn has_image_inner() -> bool {
    false
}

/// Read PNG image bytes from the system clipboard, if present.
///
/// Used for the Vibe fallback, which cannot read the pasteboard itself: vmux
/// writes these bytes to a temp file and pastes its path instead of `Ctrl+V`.
pub fn read_image_png() -> Option<Vec<u8>> {
    read_image_png_inner()
}

#[cfg(target_os = "macos")]
fn read_image_png_inner() -> Option<Vec<u8>> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG};
    let png_type = unsafe { NSPasteboardTypePNG };
    let data = NSPasteboard::generalPasteboard().dataForType(png_type)?;
    Some(data.to_vec())
}

#[cfg(not(target_os = "macos"))]
fn read_image_png_inner() -> Option<Vec<u8>> {
    None
}

/// Absolute path of an image *file* on the clipboard (a copied file, e.g. a
/// saved screenshot), if any.
///
/// Distinct from [`has_image`], which reports raw image *data*. Agent CLIs
/// auto-detect an image path pasted as text, so this lets ⌘V attach a copied
/// image file without raw clipboard image data.
pub fn image_file_path() -> Option<String> {
    image_file_path_inner()
}

#[cfg(target_os = "macos")]
fn image_file_path_inner() -> Option<String> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeFileURL};
    let url_type = unsafe { NSPasteboardTypeFileURL };
    let url_str = NSPasteboard::generalPasteboard()
        .stringForType(url_type)?
        .to_string();
    let path = url::Url::parse(&url_str).ok()?.to_file_path().ok()?;
    path_looks_like_image(&path).then(|| path.to_string_lossy().into_owned())
}

#[cfg(not(target_os = "macos"))]
fn image_file_path_inner() -> Option<String> {
    None
}

/// Whether `path` has a known raster-image extension.
#[cfg(target_os = "macos")]
fn path_looks_like_image(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "tiff" | "tif" | "bmp" | "heic")
    )
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn image_extensions_detected_case_insensitively() {
        assert!(path_looks_like_image(Path::new("/tmp/Screenshot.png")));
        assert!(path_looks_like_image(Path::new("/tmp/a.JPG")));
        assert!(path_looks_like_image(Path::new("/tmp/a.jpeg")));
        assert!(!path_looks_like_image(Path::new("/tmp/notes.txt")));
        assert!(!path_looks_like_image(Path::new("/tmp/noext")));
    }
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
