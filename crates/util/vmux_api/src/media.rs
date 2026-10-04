#[vmux_api::contract(Copy, Eq)]
pub enum MediaKind {
    Image,
    Video,
    Audio,
    Pdf,
}

impl MediaKind {
    pub fn from_path(path: &str) -> Option<Self> {
        Some(match MediaPath::extension(path).as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "ico" | "svg" => Self::Image,
            "mp4" | "m4v" | "mov" | "webm" | "ogv" => Self::Video,
            "mp3" | "m4a" | "aac" | "wav" | "flac" | "ogg" | "opus" => Self::Audio,
            "pdf" => Self::Pdf,
            _ => return None,
        })
    }

    pub fn mime(path: &str) -> Option<&'static str> {
        Some(match MediaPath::extension(path).as_str() {
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

    pub fn image_mime(path: &str) -> Option<&'static str> {
        match Self::from_path(path) {
            Some(Self::Image) => Self::mime(path),
            _ => None,
        }
    }

    pub fn requires_native_video(path: &str) -> bool {
        matches!(MediaPath::extension(path).as_str(), "mp4" | "m4v" | "mov")
    }
}

struct MediaPath;

impl MediaPath {
    fn extension(path: &str) -> String {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        match name.rsplit_once('.') {
            Some((_, extension)) if !extension.is_empty() => extension.to_ascii_lowercase(),
            _ => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_each_kind() {
        assert_eq!(MediaKind::from_path("/a/b/c.PNG"), Some(MediaKind::Image));
        assert_eq!(MediaKind::from_path("x.svg"), Some(MediaKind::Image));
        assert_eq!(MediaKind::from_path("clip.mp4"), Some(MediaKind::Video));
        assert_eq!(MediaKind::from_path("v.MOV"), Some(MediaKind::Video));
        assert_eq!(MediaKind::from_path("song.flac"), Some(MediaKind::Audio));
        assert_eq!(MediaKind::from_path("doc.pdf"), Some(MediaKind::Pdf));
        assert_eq!(MediaKind::from_path("main.rs"), None);
        assert_eq!(MediaKind::from_path("no_ext"), None);
    }

    #[test]
    fn mime_matches_kind() {
        assert_eq!(MediaKind::mime("a.webp"), Some("image/webp"));
        assert_eq!(MediaKind::mime("a.mp4"), Some("video/mp4"));
        assert_eq!(MediaKind::mime("a.mp3"), Some("audio/mpeg"));
        assert_eq!(MediaKind::mime("a.pdf"), Some("application/pdf"));
        assert_eq!(MediaKind::mime("a.rs"), None);
    }

    #[test]
    fn proprietary_video_only_mp4_family() {
        assert!(MediaKind::requires_native_video("a.mov"));
        assert!(MediaKind::requires_native_video("A.MP4"));
        assert!(MediaKind::requires_native_video("clip.m4v"));
        assert!(!MediaKind::requires_native_video("a.webm"));
        assert!(!MediaKind::requires_native_video("a.ogv"));
        assert!(!MediaKind::requires_native_video("a.png"));
    }

    #[test]
    fn image_mime_excludes_non_images() {
        assert_eq!(MediaKind::image_mime("a.png"), Some("image/png"));
        assert_eq!(MediaKind::image_mime("a.mp4"), None);
        assert_eq!(MediaKind::image_mime("a.pdf"), None);
    }
}
