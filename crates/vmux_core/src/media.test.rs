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
