use tracing::warn;

use objc2_app_kit::{
    NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeTIFF,
};
use objc2_foundation::{NSArray, NSURL};
use std::io::Write;
use std::process::{Command, Stdio};

impl super::Clipboard {
    pub(super) fn write_blocking(text: &str) {
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

    pub fn read_text() -> Option<String> {
        let output = Command::new("/usr/bin/pbpaste").output().ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn has_image() -> bool {
        let png_type = unsafe { NSArray::from_slice(&[NSPasteboardTypePNG]) };
        NSPasteboard::generalPasteboard()
            .availableTypeFromArray(&png_type)
            .is_some()
    }

    pub fn read_image_png() -> Option<Vec<u8>> {
        let png_type = unsafe { NSPasteboardTypePNG };
        let data = NSPasteboard::generalPasteboard().dataForType(png_type)?;
        Some(data.to_vec())
    }

    pub fn read_image_tiff() -> Option<Vec<u8>> {
        let tiff_type = unsafe { NSPasteboardTypeTIFF };
        let data = NSPasteboard::generalPasteboard().dataForType(tiff_type)?;
        Some(data.to_vec())
    }

    pub fn image_file_path() -> Option<String> {
        let url_type = unsafe { NSPasteboardTypeFileURL };
        let pasteboard = NSPasteboard::generalPasteboard();
        let mut candidates = Vec::new();
        if let Some(text) = pasteboard.stringForType(url_type) {
            candidates.push(text);
        }
        if let Some(items) = pasteboard.pasteboardItems() {
            for index in 0..items.count() {
                if let Some(text) = items.objectAtIndex(index).stringForType(url_type) {
                    candidates.push(text);
                }
            }
        }
        for candidate in candidates {
            let Some(url) = NSURL::URLWithString(&candidate) else {
                continue;
            };
            let resolved = url.filePathURL().unwrap_or(url);
            let Some(path) = resolved.path() else {
                continue;
            };
            let path = std::path::PathBuf::from(path.to_string());
            if path_looks_like_image(&path) {
                return Some(path.to_string_lossy().into_owned());
            }
        }
        None
    }
}

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
