use std::path::Path;

use vmux_core::event::{FileLine, PreviewKind};

use crate::dir::list_dir;
use crate::highlight::Highlighter;

pub const IMAGE_BYTES_CAP: u64 = 25 * 1024 * 1024;
pub const THUMB_MAX_EDGE: u32 = 64;
const TEXT_PREVIEW_LINES: usize = 200;

pub fn image_mime(path: &Path) -> Option<&'static str> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

pub fn is_image_path(path: &Path) -> bool {
    image_mime(path).is_some()
}

pub fn downscale_to_png(bytes: &[u8], max_edge: u32) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let thumb = img.thumbnail(max_edge, max_edge);
    let mut out = std::io::Cursor::new(Vec::new());
    thumb
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

pub fn build_preview_sync(path: &Path) -> PreviewKind {
    build_preview_with_cap(path, false, IMAGE_BYTES_CAP)
}

pub fn build_preview_with_cap(path: &Path, _thumb: bool, cap: u64) -> PreviewKind {
    if path.is_dir() {
        return PreviewKind::Dir(list_dir(path));
    }
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return PreviewKind::Error(e.to_string()),
    };
    if let Some(mime) = image_mime(path) {
        if meta.len() > cap {
            return info_kind(&meta, "image (too large to preview)");
        }
        return match std::fs::read(path) {
            Ok(bytes) => PreviewKind::Image {
                mime: mime.to_string(),
                bytes,
            },
            Err(e) => PreviewKind::Error(e.to_string()),
        };
    }
    if is_probably_binary(path) {
        return info_kind(&meta, "binary");
    }
    match Highlighter::new().load_file(path) {
        Ok(out) => {
            let lines: Vec<FileLine> = out.lines.into_iter().take(TEXT_PREVIEW_LINES).collect();
            PreviewKind::Text(lines)
        }
        Err(_) => info_kind(&meta, "file"),
    }
}

fn is_probably_binary(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 8192];
    match f.read(&mut buf) {
        Ok(n) => buf[..n].contains(&0),
        Err(_) => false,
    }
}

fn info_kind(meta: &std::fs::Metadata, kind: &str) -> PreviewKind {
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default();
    PreviewKind::Info {
        size: meta.len(),
        modified,
        kind: kind.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn downscale_caps_longest_edge_and_is_valid_png() {
        let src = png_bytes(200, 100);
        let thumb = downscale_to_png(&src, 64).unwrap();
        let decoded = image::load_from_memory(&thumb).unwrap();
        assert!(decoded.width() <= 64 && decoded.height() <= 64);
        assert_eq!(decoded.width().max(decoded.height()), 64);
    }

    #[test]
    fn downscale_rejects_garbage() {
        assert!(downscale_to_png(&[0, 1, 2, 3], 64).is_err());
    }

    #[test]
    fn build_preview_dir_text_image_info() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().join("sub");
        std::fs::create_dir(&d).unwrap();
        assert!(matches!(build_preview_sync(&d), PreviewKind::Dir(_)));

        let t = tmp.path().join("a.rs");
        std::fs::write(&t, "fn main() {}\n").unwrap();
        assert!(matches!(build_preview_sync(&t), PreviewKind::Text(_)));

        let p = tmp.path().join("p.png");
        std::fs::write(&p, png_bytes(8, 8)).unwrap();
        assert!(matches!(build_preview_sync(&p), PreviewKind::Image { .. }));

        let b = tmp.path().join("blob.bin");
        std::fs::write(&b, [0u8; 4]).unwrap();
        assert!(matches!(build_preview_sync(&b), PreviewKind::Info { .. }));
    }

    #[test]
    fn build_preview_image_over_cap_is_info() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("huge.png");
        std::fs::write(&p, png_bytes(8, 8)).unwrap();
        let k = build_preview_with_cap(&p, false, 1);
        assert!(matches!(k, PreviewKind::Info { .. }));
    }
}
