/// Largest valid `top_line` so a full viewport stays in range.
pub fn clamp_top_line(top_line: u32, total_lines: u32, rows: u16) -> u32 {
    let max_top = total_lines.saturating_sub(rows as u32);
    top_line.min(max_top)
}

/// Visible line range `[first, end)` after clamping the scroll offset.
pub fn window_range(total_lines: u32, top_line: u32, rows: u16) -> (u32, u32) {
    let first = clamp_top_line(top_line, total_lines, rows);
    let end = first.saturating_add(rows as u32).min(total_lines);
    (first, end)
}

/// Whole rows that fit in `viewport_height` at `char_height` px per row.
pub fn rows_from_viewport(char_height: f32, viewport_height: f32) -> u16 {
    if char_height <= 0.0 || viewport_height <= 0.0 {
        return 0;
    }
    (viewport_height / char_height).floor() as u16
}

/// Index range into a buffer of `total` lines for the visible window.
pub fn visible_slice(total: u32, top_line: u32, rows: u16) -> std::ops::Range<usize> {
    let (first, end) = window_range(total, top_line, rows);
    (first as usize)..(end as usize)
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
    fn window_empty_file() {
        assert_eq!(window_range(0, 5, 10), (0, 0));
    }

    #[test]
    fn clamp_top_caps_at_max_scroll() {
        assert_eq!(clamp_top_line(99, 10, 4), 6);
        assert_eq!(clamp_top_line(2, 10, 4), 2);
        assert_eq!(clamp_top_line(5, 3, 10), 0);
    }

    #[test]
    fn rows_from_viewport_floors() {
        assert_eq!(rows_from_viewport(16.0, 480.0), 30);
        assert_eq!(rows_from_viewport(0.0, 480.0), 0);
        assert_eq!(rows_from_viewport(16.0, 8.0), 0);
    }

    #[test]
    fn visible_slice_indices() {
        assert_eq!(visible_slice(10, 8, 4), 6..10);
        assert_eq!(visible_slice(0, 0, 10), 0..0);
    }
}
