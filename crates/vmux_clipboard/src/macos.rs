//! macOS clipboard: `pbcopy`/`pbpaste` for text, `NSPasteboard` for image data.

use tracing::warn;

impl super::Clipboard {
    pub(super) fn write_blocking(text: &str) {
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

    pub(super) fn read_text() -> Option<String> {
        use std::process::Command;
        let output = Command::new("/usr/bin/pbpaste").output().ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub(super) fn has_png() -> bool {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG};
        use objc2_foundation::NSArray;
        let png_type = unsafe { NSArray::from_slice(&[NSPasteboardTypePNG]) };
        NSPasteboard::generalPasteboard()
            .availableTypeFromArray(&png_type)
            .is_some()
    }

    pub(super) fn read_png() -> Option<Vec<u8>> {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG};
        let png_type = unsafe { NSPasteboardTypePNG };
        let data = NSPasteboard::generalPasteboard().dataForType(png_type)?;
        Some(data.to_vec())
    }

    pub(super) fn read_tiff() -> Option<Vec<u8>> {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeTIFF};
        let tiff_type = unsafe { NSPasteboardTypeTIFF };
        let data = NSPasteboard::generalPasteboard().dataForType(tiff_type)?;
        Some(data.to_vec())
    }

    pub(super) fn image_file_path() -> Option<String> {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeFileURL};
        let url_type = unsafe { NSPasteboardTypeFileURL };
        let url_str = NSPasteboard::generalPasteboard()
            .stringForType(url_type)?
            .to_string();
        let path = url::Url::parse(&url_str).ok()?.to_file_path().ok()?;
        path_looks_like_image(&path).then(|| path.to_string_lossy().into_owned())
    }
}

/// Whether `path` has a known raster-image extension.
fn path_looks_like_image(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "tiff" | "tif" | "bmp" | "heic")
    )
}

#[cfg(test)]
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
