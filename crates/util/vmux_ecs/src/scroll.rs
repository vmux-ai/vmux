pub const EDITOR_OVERSCAN_K: f32 = 1.5;
pub const TERMINAL_OVERSCAN_K: f32 = 2.0;
pub const OVERSCAN_FLOOR: u32 = 48;
pub const OVERSCAN_CAP: u32 = 512;
pub const EDGE_TRIGGER_K: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollWindow {
    total_lines: u32,
    top_line: u32,
    rows: u32,
}

impl ScrollWindow {
    pub fn new(total_lines: u32, top_line: u32, rows: impl Into<u32>) -> Self {
        Self {
            total_lines,
            top_line,
            rows: rows.into(),
        }
    }

    pub fn top(self) -> u32 {
        self.top_line
            .min(self.total_lines.saturating_sub(self.rows))
    }

    pub fn range(self) -> (u32, u32) {
        let first = self.top();
        let end = first.saturating_add(self.rows).min(self.total_lines);
        (first, end)
    }

    pub fn needs_refetch(self, loaded_first: u32, loaded_len: u32, trigger: u32) -> bool {
        let loaded_end = loaded_first.saturating_add(loaded_len);
        let near_top = self.top_line < loaded_first.saturating_add(trigger);
        let near_bottom = self.top_line + self.rows + trigger > loaded_end;
        near_top || near_bottom
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportRows(u16);

impl ViewportRows {
    pub fn from_pixels(character_height: f32, viewport_height: f32) -> Self {
        if character_height <= 0.0 || viewport_height <= 0.0 {
            return Self(0);
        }
        Self((viewport_height / character_height).floor() as u16)
    }

    pub fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Overscan(u32);

impl Overscan {
    pub fn new(visible: u16, factor: f32, floor: u32, cap: u32) -> Self {
        let scaled = (visible as f32 * factor).ceil() as u32;
        Self(scaled.clamp(floor, cap))
    }

    pub fn rows(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BottomPadding(f32);

impl BottomPadding {
    pub fn aligned(client_height: f32, padding: f32, character_height: f32) -> Self {
        if character_height <= 0.0 {
            return Self(0.0);
        }
        Self((client_height - padding).rem_euclid(character_height))
    }

    pub fn pixels(self) -> f32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_clamps_at_end() {
        assert_eq!(ScrollWindow::new(10, 8, 4_u16).range(), (6, 10));
    }

    #[test]
    fn window_from_top() {
        assert_eq!(ScrollWindow::new(10, 0, 4_u16).range(), (0, 4));
    }

    #[test]
    fn window_smaller_than_viewport() {
        assert_eq!(ScrollWindow::new(3, 0, 10_u16).range(), (0, 3));
    }

    #[test]
    fn clamp_caps_at_max_scroll() {
        assert_eq!(ScrollWindow::new(10, 99, 4_u16).top(), 6);
        assert_eq!(ScrollWindow::new(10, 2, 4_u16).top(), 2);
        assert_eq!(ScrollWindow::new(3, 5, 10_u16).top(), 0);
    }

    #[test]
    fn overscan_scales_and_clamps() {
        assert_eq!(Overscan::new(50, 2.0, 48, 512).rows(), 100);
        assert_eq!(Overscan::new(10, 2.0, 48, 512).rows(), 48);
        assert_eq!(Overscan::new(400, 2.0, 48, 512).rows(), 512);
    }

    #[test]
    fn refetch_fires_near_edges_only() {
        assert!(ScrollWindow::new(u32::MAX, 120, 50_u32).needs_refetch(100, 200, 50));
        assert!(ScrollWindow::new(u32::MAX, 220, 50_u32).needs_refetch(100, 200, 50));
        assert!(!ScrollWindow::new(u32::MAX, 170, 50_u32).needs_refetch(100, 200, 50));
    }

    #[test]
    fn follow_bottom_pad_aligns_pinned_top_edge_to_row_boundary() {
        let cases = [
            (790.0_f32, 4.0_f32, 18.0_f32),
            (800.0, 4.0, 18.0),
            (1013.0, 6.0, 21.0),
            (601.0, 8.0, 16.5),
            (1234.0, 0.0, 19.0),
        ];
        for (client_h, pad, ch) in cases {
            let e = BottomPadding::aligned(client_h, pad, ch).pixels();
            assert!((0.0..ch).contains(&e), "e={e} out of [0,{ch})");
            let total = 200.0_f32;
            let scroll_height = total * ch + 2.0 * pad + e;
            let max_scroll = scroll_height - client_h;
            let misalign = (max_scroll - pad).rem_euclid(ch);
            let misalign = misalign.min(ch - misalign);
            assert!(
                misalign < 1e-2,
                "client_h={client_h} pad={pad} ch={ch} e={e} misalign={misalign}"
            );
        }
    }

    #[test]
    fn follow_bottom_pad_zero_ch_is_safe() {
        assert_eq!(BottomPadding::aligned(800.0, 4.0, 0.0).pixels(), 0.0);
    }
}
