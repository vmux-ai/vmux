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
    assert_eq!(
        r,
        CropRect {
            x: 60,
            y: 70,
            w: 80,
            h: 60
        }
    );

    let r = crop_rect_from_node(990.0, 990.0, 40.0, 40.0, 1000, 1000);
    assert_eq!(
        r,
        CropRect {
            x: 970,
            y: 970,
            w: 30,
            h: 30
        }
    );
}

#[test]
fn encode_downscaled_png_emits_png_header() {
    let img = image::RgbaImage::new(10, 10);
    let (png, w, h) = encode_downscaled_png(&img, 1568).unwrap();
    assert_eq!((w, h), (10, 10));
    assert_eq!(&png[..4], &[137, 80, 78, 71]);
}
