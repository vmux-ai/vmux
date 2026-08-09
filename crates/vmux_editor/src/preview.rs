use std::path::Path;

use vmux_core::event::{FileLine, PreviewKind};

use crate::dir::list_dir;
use crate::highlight::Highlighter;

pub const IMAGE_BYTES_CAP: u64 = 25 * 1024 * 1024;
pub const THUMB_MAX_EDGE: u32 = 64;
const TEXT_PREVIEW_LINES: usize = 200;

pub fn image_mime(path: &Path) -> Option<&'static str> {
    vmux_core::media::image_mime(&path.to_string_lossy())
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

fn raw_preview_url(path: &Path) -> String {
    url::Url::from_file_path(path)
        .map(|u| {
            let mut s = u.to_string();
            s.push_str("?vmux-raw=1");
            s
        })
        .unwrap_or_default()
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
    if vmux_core::media::media_kind(&path.to_string_lossy())
        == Some(vmux_core::media::MediaKind::Video)
    {
        let path_str = path.to_string_lossy();
        return PreviewKind::Video {
            url: raw_preview_url(path),
            path: path_str.clone().into_owned(),
            native: cfg!(target_os = "macos") && vmux_core::media::is_proprietary_video(&path_str),
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
#[path = "preview.test.rs"]
mod tests;
