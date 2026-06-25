#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollKind {
    Line,
    Half,
}

pub const LINE_PX: f64 = 60.0;

pub fn scroll_delta(kind: ScrollKind, down: bool, viewport_h: f64) -> f64 {
    let mag = match kind {
        ScrollKind::Line => LINE_PX,
        ScrollKind::Half => viewport_h / 2.0,
    };
    if down { mag } else { -mag }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_scroll_is_fixed() {
        assert_eq!(scroll_delta(ScrollKind::Line, true, 800.0), 60.0);
        assert_eq!(scroll_delta(ScrollKind::Line, false, 800.0), -60.0);
    }

    #[test]
    fn half_scroll_uses_viewport() {
        assert_eq!(scroll_delta(ScrollKind::Half, true, 800.0), 400.0);
        assert_eq!(scroll_delta(ScrollKind::Half, false, 800.0), -400.0);
    }
}
