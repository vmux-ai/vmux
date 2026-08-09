//! Pure scroll-windowing math shared by the file editor and the terminal.
//! Rows are `u32`/`u16` counts; no Bevy, no DOM — compiles for wasm.

/// Multiple of the visible row count to buffer beyond the viewport, per side.
pub const EDITOR_OVERSCAN_K: f32 = 1.5;
/// Terminal buffers more: its window refill crosses an extra process hop.
pub const TERMINAL_OVERSCAN_K: f32 = 2.0;
/// Minimum overscan (small panes still get a usable runway).
pub const OVERSCAN_FLOOR: u32 = 48;
/// Maximum overscan (bound DOM node count on very tall panes).
pub const OVERSCAN_CAP: u32 = 512;
/// Refetch trigger margin as a multiple of the visible row count.
pub const EDGE_TRIGGER_K: f32 = 1.0;

/// Clamp a requested top line so the viewport never scrolls past the last page.
pub fn clamp_top_line(top_line: u32, total_lines: u32, rows: u16) -> u32 {
    let max_top = total_lines.saturating_sub(rows as u32);
    top_line.min(max_top)
}

/// `[first, end)` line range for a viewport of `rows` starting at `top_line`.
pub fn window_range(total_lines: u32, top_line: u32, rows: u16) -> (u32, u32) {
    let first = clamp_top_line(top_line, total_lines, rows);
    let end = first.saturating_add(rows as u32).min(total_lines);
    (first, end)
}

/// Number of whole rows that fit in `viewport_height` at `char_height`.
pub fn rows_from_viewport(char_height: f32, viewport_height: f32) -> u16 {
    if char_height <= 0.0 || viewport_height <= 0.0 {
        return 0;
    }
    (viewport_height / char_height).floor() as u16
}

/// `[first, end)` as a `usize` range, for slicing an in-memory line buffer.
pub fn visible_slice(total: u32, top_line: u32, rows: u16) -> std::ops::Range<usize> {
    let (first, end) = window_range(total, top_line, rows);
    (first as usize)..(end as usize)
}

/// Rows to hold beyond the visible region on EACH side, scaled to the viewport
/// and clamped to `[floor, cap]`.
pub fn overscan_for(visible: u16, k: f32, floor: u32, cap: u32) -> u32 {
    let scaled = (visible as f32 * k).ceil() as u32;
    scaled.clamp(floor, cap)
}

/// True when the visible region is within `trigger` rows of the loaded window
/// edge, i.e. a refill should be requested now.
pub fn needs_refetch(
    vis_first: u32,
    vis_rows: u32,
    loaded_first: u32,
    loaded_len: u32,
    trigger: u32,
) -> bool {
    let loaded_end = loaded_first.saturating_add(loaded_len);
    let near_top = vis_first < loaded_first.saturating_add(trigger);
    let near_bot = vis_first + vis_rows + trigger > loaded_end;
    near_top || near_bot
}

/// alacritty grid `Line` for a document row (row 0 = oldest scrollback line):
/// `Line(doc_row - history_size)`. Returned as `i32` (may be negative = history).
pub fn doc_row_to_line(doc_row: u32, history_size: u32) -> i32 {
    doc_row as i32 - history_size as i32
}

/// Bottom alignment pad (px) for a terminal that follows by pinning to the
/// scroll maximum.
///
/// A follow-to-bottom terminal scrolls to `scrollHeight`, so the fractional
/// sub-row left over when `client_h` is not a whole multiple of the cell
/// height `ch` lands as a clipped partial row at the *top* of the viewport.
/// Adding this many pixels below the last row shifts that remainder to the
/// bottom instead, so the pinned top edge falls on a row boundary and the top
/// line renders whole. `pad` is the container's top inner padding (the row
/// grid's y-origin). The result is in `[0, ch)`.
pub fn follow_bottom_pad(client_h: f32, pad: f32, ch: f32) -> f32 {
    if ch <= 0.0 {
        return 0.0;
    }
    (client_h - pad).rem_euclid(ch)
}

#[cfg(test)]
mod tests {
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
}
