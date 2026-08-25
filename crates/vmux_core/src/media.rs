use serde::{Deserialize, Serialize};

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
    Image,
    Video,
    Audio,
    Pdf,
}

fn ext_of(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

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
        "mp4" | "m4v" | "mov" => "video/mp4",
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

pub fn is_proprietary_video(path: &str) -> bool {
    matches!(ext_of(path).as_str(), "mp4" | "m4v" | "mov")
}

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
    fn proprietary_video_only_mp4_family() {
        assert!(is_proprietary_video("a.mov"));
        assert!(is_proprietary_video("A.MP4"));
        assert!(is_proprietary_video("clip.m4v"));
        assert!(!is_proprietary_video("a.webm"));
        assert!(!is_proprietary_video("a.ogv"));
        assert!(!is_proprietary_video("a.png"));
    }

    #[test]
    fn image_mime_excludes_non_images() {
        assert_eq!(image_mime("a.png"), Some("image/png"));
        assert_eq!(image_mime("a.mp4"), None);
        assert_eq!(image_mime("a.pdf"), None);
    }
}
