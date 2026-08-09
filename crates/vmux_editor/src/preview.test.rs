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
