pub(crate) const MAX_INLINE_EDGE: u32 = 1568;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CropRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

pub(crate) fn downscale_dims(w: u32, h: u32, max_edge: u32) -> (u32, u32) {
    let long = w.max(h);
    if long == 0 {
        return (1, 1);
    }
    if long <= max_edge {
        return (w.max(1), h.max(1));
    }
    let scale = max_edge as f64 / long as f64;
    (
        ((w as f64 * scale).round() as u32).max(1),
        ((h as f64 * scale).round() as u32).max(1),
    )
}

pub(crate) fn crop_rect_from_node(
    center_x: f32,
    center_y: f32,
    size_x: f32,
    size_y: f32,
    img_w: u32,
    img_h: u32,
) -> CropRect {
    let left = (center_x - size_x * 0.5).round().max(0.0) as u32;
    let top = (center_y - size_y * 0.5).round().max(0.0) as u32;
    let left = left.min(img_w.saturating_sub(1));
    let top = top.min(img_h.saturating_sub(1));
    let w = (size_x.round().max(1.0) as u32).min(img_w - left);
    let h = (size_y.round().max(1.0) as u32).min(img_h - top);
    CropRect { x: left, y: top, w, h }
}

pub(crate) fn encode_downscaled_png(
    img: &image::RgbaImage,
    max_edge: u32,
) -> Result<(Vec<u8>, u32, u32), String> {
    let (dw, dh) = downscale_dims(img.width(), img.height(), max_edge);
    let dynimg = image::DynamicImage::ImageRgba8(img.clone());
    let scaled = if (dw, dh) == (img.width(), img.height()) {
        dynimg
    } else {
        dynimg.resize_exact(dw, dh, image::imageops::FilterType::Lanczos3)
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    scaled
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("png encode failed: {e}"))?;
    Ok((buf.into_inner(), dw, dh))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downscale_never_upscales() {
        assert_eq!(downscale_dims(800, 600, 1568), (800, 600));
        assert_eq!(downscale_dims(0, 0, 1568), (1, 1));
    }

    #[test]
    fn downscale_caps_long_edge() {
        assert_eq!(downscale_dims(3136, 1568, 1568), (1568, 784));
        assert_eq!(downscale_dims(1568, 3136, 1568), (784, 1568));
    }

    #[test]
    fn crop_rect_clamps_to_image() {
        let r = crop_rect_from_node(100.0, 100.0, 80.0, 60.0, 1000, 1000);
        assert_eq!(r, CropRect { x: 60, y: 70, w: 80, h: 60 });

        let r = crop_rect_from_node(990.0, 990.0, 40.0, 40.0, 1000, 1000);
        assert_eq!(r, CropRect { x: 970, y: 970, w: 30, h: 30 });
    }

    #[test]
    fn encode_downscaled_png_emits_png_header() {
        let img = image::RgbaImage::new(10, 10);
        let (png, w, h) = encode_downscaled_png(&img, 1568).unwrap();
        assert_eq!((w, h), (10, 10));
        assert_eq!(&png[..4], &[137, 80, 78, 71]);
    }
}
