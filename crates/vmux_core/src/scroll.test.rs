use super::*;

#[test]
fn window_clamps_at_end() {
    assert_eq!(window_range(10, 8, 4), (6, 10));
}

#[test]
fn window_from_top() {
    assert_eq!(window_range(10, 0, 4), (0, 4));
}

#[test]
fn window_smaller_than_viewport() {
    assert_eq!(window_range(3, 0, 10), (0, 3));
}

#[test]
fn clamp_caps_at_max_scroll() {
    assert_eq!(clamp_top_line(99, 10, 4), 6);
    assert_eq!(clamp_top_line(2, 10, 4), 2);
    assert_eq!(clamp_top_line(5, 3, 10), 0);
}

#[test]
fn overscan_scales_and_clamps() {
    // 50 rows * 2.0 = 100, within [48, 512].
    assert_eq!(overscan_for(50, 2.0, 48, 512), 100);
    // small pane hits the floor.
    assert_eq!(overscan_for(10, 2.0, 48, 512), 48);
    // huge pane hits the cap.
    assert_eq!(overscan_for(400, 2.0, 48, 512), 512);
}

#[test]
fn refetch_fires_near_edges_only() {
    // Loaded [100, 300), visible 50 rows, trigger 50.
    assert!(needs_refetch(120, 50, 100, 200, 50)); // near top
    assert!(needs_refetch(220, 50, 100, 200, 50)); // near bottom
    assert!(!needs_refetch(170, 50, 100, 200, 50)); // middle: no refetch
}

#[test]
fn doc_row_maps_to_line() {
    // history 100: oldest doc row 0 -> Line(-100); newest visible -> >= 0.
    assert_eq!(doc_row_to_line(0, 100), -100);
    assert_eq!(doc_row_to_line(100, 100), 0);
    assert_eq!(doc_row_to_line(149, 100), 49);
}

#[test]
fn follow_bottom_pad_aligns_pinned_top_edge_to_row_boundary() {
    // For any viewport/pad/cell size, adding follow_bottom_pad below the
    // rows must make the pinned (max-scroll) top edge land exactly on a
    // row top: (max_scroll - pad) % ch == 0.
    let cases = [
        (790.0_f32, 4.0_f32, 18.0_f32),
        (800.0, 4.0, 18.0), // already aligned (rem 0)
        (1013.0, 6.0, 21.0),
        (601.0, 8.0, 16.5),
        (1234.0, 0.0, 19.0),
    ];
    for (client_h, pad, ch) in cases {
        let e = follow_bottom_pad(client_h, pad, ch);
        assert!((0.0..ch).contains(&e), "e={e} out of [0,{ch})");
        // Enough rows to force scrolling.
        let total = 200.0_f32;
        let scroll_height = total * ch + 2.0 * pad + e;
        let max_scroll = scroll_height - client_h;
        let misalign = (max_scroll - pad).rem_euclid(ch);
        let misalign = misalign.min(ch - misalign); // distance to nearest boundary
        assert!(
            misalign < 1e-2,
            "client_h={client_h} pad={pad} ch={ch} e={e} misalign={misalign}"
        );
    }
}

#[test]
fn follow_bottom_pad_zero_ch_is_safe() {
    assert_eq!(follow_bottom_pad(800.0, 4.0, 0.0), 0.0);
}
