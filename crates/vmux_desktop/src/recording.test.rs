use super::*;

#[test]
fn output_paths_default_dir_and_name() {
    let default_dir = Path::new("/tmp/def");
    let (mp4, gif) = resolve_output_paths(None, None, false, "20260623-101010-001", default_dir);
    assert_eq!(mp4, PathBuf::from("/tmp/def/vmux-20260623-101010-001.mp4"));
    assert!(gif.is_none());
}

#[test]
fn output_paths_custom_dir_name_and_gif() {
    let (mp4, gif) = resolve_output_paths(
        Some("/tmp/out"),
        Some("feature-x"),
        true,
        "ts",
        Path::new("/tmp/def"),
    );
    assert_eq!(mp4, PathBuf::from("/tmp/out/feature-x.mp4"));
    assert_eq!(gif, Some(PathBuf::from("/tmp/out/feature-x.gif")));
}

#[test]
fn gif_sampling_respects_fps() {
    assert!(should_sample_gif_frame(0, None, 12));
    assert!(!should_sample_gif_frame(40, Some(0), 12));
    assert!(should_sample_gif_frame(90, Some(0), 12));
}

#[test]
fn bgra_to_rgba_swaps_channels() {
    let bgra = vec![1u8, 2, 3, 4];
    assert_eq!(bgra_to_rgba(&bgra), vec![3, 2, 1, 4]);
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
}

#[test]
fn downscale_caps_long_edge_without_upscaling() {
    assert_eq!(downscale_to(800, 600, 800), (800, 600));
    assert_eq!(downscale_to(1600, 800, 800), (800, 400));
    assert_eq!(downscale_to(0, 0, 800), (1, 1));
}
