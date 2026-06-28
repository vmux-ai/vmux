//! Extension-based media classification shared by the editor backend and the
//! `file://` page: decides whether a path is an image, video, audio, or PDF and
//! what MIME type to serve it as.

use serde::{Deserialize, Serialize};

/// The kind of media a `file://` path resolves to.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum MediaKind {
    /// Raster or vector image rendered with `<img>`.
    Image,
    /// Video rendered with `<video controls>`.
    Video,
    /// Audio rendered with `<audio controls>`.
    Audio,
    /// PDF shown via an info card (no inline render in v1).
    Pdf,
}

fn ext_of(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// MIME type for a media path, or `None` if the extension is not recognized media.
pub fn media_mime(path: &str) -> Option<&'static str> {
    Some(match ext_of(path).as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" | "opus" => "audio/ogg",
        "pdf" => "application/pdf",
        _ => return None,
    })
}

/// Classify a path into a [`MediaKind`], or `None` if not media.
pub fn media_kind(path: &str) -> Option<MediaKind> {
    Some(match ext_of(path).as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "ico" | "svg" => {
            MediaKind::Image
        }
        "mp4" | "m4v" | "mov" | "webm" | "ogv" => MediaKind::Video,
        "mp3" | "m4a" | "aac" | "wav" | "flac" | "ogg" | "opus" => MediaKind::Audio,
        "pdf" => MediaKind::Pdf,
        _ => return None,
    })
}

/// MIME type for an image path only (used by the dir-browser thumbnail path,
/// which renders raster previews and must not treat video/audio/pdf as images).
pub fn image_mime(path: &str) -> Option<&'static str> {
    match media_kind(path) {
        Some(MediaKind::Image) => media_mime(path),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_each_kind() {
        assert_eq!(media_kind("/a/b/c.PNG"), Some(MediaKind::Image));
        assert_eq!(media_kind("x.svg"), Some(MediaKind::Image));
        assert_eq!(media_kind("clip.mp4"), Some(MediaKind::Video));
        assert_eq!(media_kind("v.MOV"), Some(MediaKind::Video));
        assert_eq!(media_kind("song.flac"), Some(MediaKind::Audio));
        assert_eq!(media_kind("doc.pdf"), Some(MediaKind::Pdf));
        assert_eq!(media_kind("main.rs"), None);
        assert_eq!(media_kind("no_ext"), None);
    }

    #[test]
    fn mime_matches_kind() {
        assert_eq!(media_mime("a.webp"), Some("image/webp"));
        assert_eq!(media_mime("a.mp4"), Some("video/mp4"));
        assert_eq!(media_mime("a.mp3"), Some("audio/mpeg"));
        assert_eq!(media_mime("a.pdf"), Some("application/pdf"));
        assert_eq!(media_mime("a.rs"), None);
    }

    #[test]
    fn image_mime_excludes_non_images() {
        assert_eq!(image_mime("a.png"), Some("image/png"));
        assert_eq!(image_mime("a.mp4"), None);
        assert_eq!(image_mime("a.pdf"), None);
    }
}
