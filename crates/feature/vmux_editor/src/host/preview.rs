use std::path::Path;

use vmux_ecs::event::{FileLine, PreviewKind};

use super::file_lifecycle::FileDir;
use crate::host::highlight::{Highlighter, LoadError};

const TEXT_PREVIEW_LINES: usize = 200;

pub(crate) struct PreviewBuilder;

impl PreviewBuilder {
    pub(crate) const IMAGE_BYTES_CAP: u64 = 25 * 1024 * 1024;
    pub(crate) const THUMB_MAX_EDGE: u32 = 64;

    pub(crate) fn is_image(path: &Path) -> bool {
        Self::image_mime(path).is_some()
    }

    pub(crate) fn thumbnail(bytes: &[u8], max_edge: u32) -> Result<Vec<u8>, String> {
        let image = image::load_from_memory(bytes).map_err(|error| error.to_string())?;
        let thumbnail = image.thumbnail(max_edge, max_edge);
        let mut output = std::io::Cursor::new(Vec::new());
        thumbnail
            .write_to(&mut output, image::ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        Ok(output.into_inner())
    }

    pub(crate) fn build(path: &Path) -> PreviewKind {
        Self::build_with_cap(path, Self::IMAGE_BYTES_CAP)
    }

    fn build_with_cap(path: &Path, cap: u64) -> PreviewKind {
        if path.is_dir() {
            return PreviewKind::Dir(FileDir::read(path));
        }
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => return PreviewKind::Error(error.to_string()),
        };
        if let Some(mime) = Self::image_mime(path) {
            if metadata.len() > cap {
                return Self::info(&metadata, "image (too large to preview)");
            }
            return match std::fs::read(path) {
                Ok(bytes) => PreviewKind::Image {
                    mime: mime.to_string(),
                    bytes,
                },
                Err(error) => PreviewKind::Error(error.to_string()),
            };
        }
        if vmux_api::media::MediaKind::from_path(&path.to_string_lossy())
            == Some(vmux_api::media::MediaKind::Video)
        {
            let path_string = path.to_string_lossy();
            return PreviewKind::Video {
                url: Self::raw_url(path),
                path: path_string.clone().into_owned(),
                native: cfg!(target_os = "macos")
                    && vmux_api::media::MediaKind::requires_native_video(&path_string),
            };
        }
        match Highlighter::new().load_file(path) {
            Ok(output) => {
                let lines: Vec<FileLine> =
                    output.lines.into_iter().take(TEXT_PREVIEW_LINES).collect();
                PreviewKind::Text(lines)
            }
            Err(LoadError::Binary) => Self::info(&metadata, "binary"),
            Err(LoadError::Unreadable(_)) => Self::info(&metadata, "file"),
        }
    }

    fn image_mime(path: &Path) -> Option<&'static str> {
        vmux_api::media::MediaKind::image_mime(&path.to_string_lossy())
    }

    fn raw_url(path: &Path) -> String {
        url::Url::from_file_path(path)
            .map(|url| {
                let mut value = url.to_string();
                value.push_str("?vmux-raw=1");
                value
            })
            .unwrap_or_default()
    }

    fn info(metadata: &std::fs::Metadata, kind: &str) -> PreviewKind {
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_default();
        PreviewKind::Info {
            size: metadata.len(),
            modified,
            kind: kind.to_string(),
        }
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
        let thumb = PreviewBuilder::thumbnail(&src, 64).unwrap();
        let decoded = image::load_from_memory(&thumb).unwrap();
        assert!(decoded.width() <= 64 && decoded.height() <= 64);
        assert_eq!(decoded.width().max(decoded.height()), 64);
    }

    #[test]
    fn downscale_rejects_garbage() {
        assert!(PreviewBuilder::thumbnail(&[0, 1, 2, 3], 64).is_err());
    }

    #[test]
    fn build_preview_dir_text_image_info() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().join("sub");
        std::fs::create_dir(&d).unwrap();
        assert!(matches!(PreviewBuilder::build(&d), PreviewKind::Dir(_)));

        let t = tmp.path().join("a.rs");
        std::fs::write(&t, "fn main() {}\n").unwrap();
        assert!(matches!(PreviewBuilder::build(&t), PreviewKind::Text(_)));

        let p = tmp.path().join("p.png");
        std::fs::write(&p, png_bytes(8, 8)).unwrap();
        assert!(matches!(
            PreviewBuilder::build(&p),
            PreviewKind::Image { .. }
        ));

        let b = tmp.path().join("blob.bin");
        std::fs::write(&b, [0u8; 4]).unwrap();
        assert!(matches!(
            PreviewBuilder::build(&b),
            PreviewKind::Info { .. }
        ));
    }

    #[test]
    fn build_preview_image_over_cap_is_info() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("huge.png");
        std::fs::write(&p, png_bytes(8, 8)).unwrap();
        let k = PreviewBuilder::build_with_cap(&p, 1);
        assert!(matches!(k, PreviewKind::Info { .. }));
    }
}
