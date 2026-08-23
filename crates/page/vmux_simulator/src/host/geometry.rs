use bevy::prelude::*;

/// Maps a cursor position in the vmux window onto a tap coordinate for AXe.
///
/// Three spaces are in play and only the middle one is self-describing:
/// the window is logical px, the MJPEG frame is device *pixels*, and `axe tap` wants device
/// *points*. On a 3x phone those last two differ by a factor of three, so the pixel/point ratio
/// is derived from the measured pair rather than assumed — it also absorbs the stream's
/// `--scale` flag and any rotation for free.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Mapping {
    fit_scale: f32,
    offset: Vec2,
    frame_px: Vec2,
    device_points: Vec2,
}

impl Mapping {
    pub fn new(window: Vec2, frame_px: Vec2, device_points: Vec2) -> Option<Self> {
        if frame_px.x <= 0.0 || frame_px.y <= 0.0 || window.x <= 0.0 || window.y <= 0.0 {
            return None;
        }
        if device_points.x <= 0.0 || device_points.y <= 0.0 {
            return None;
        }
        let fit_scale = (window.x / frame_px.x).min(window.y / frame_px.y);
        let drawn = frame_px * fit_scale;
        Some(Self {
            fit_scale,
            offset: (window - drawn) / 2.0,
            frame_px,
            device_points,
        })
    }

    /// Size the frame is drawn at, in window logical px.
    pub fn drawn_size(&self) -> Vec2 {
        self.frame_px * self.fit_scale
    }

    /// `None` when the cursor is in the letterbox rather than on the device.
    ///
    /// The far edge is clamped rather than rejected: dividing by `fit_scale` overshoots the
    /// corner by a fraction of a pixel, and a strict bound would leave the last row and column
    /// of the mirror dead.
    pub fn cursor_to_device(&self, cursor: Vec2) -> Option<Vec2> {
        const EDGE_TOLERANCE: f32 = 1.0;

        let in_frame = (cursor - self.offset) / self.fit_scale;
        if in_frame.x < -EDGE_TOLERANCE
            || in_frame.y < -EDGE_TOLERANCE
            || in_frame.x > self.frame_px.x + EDGE_TOLERANCE
            || in_frame.y > self.frame_px.y + EDGE_TOLERANCE
        {
            return None;
        }
        let clamped = in_frame.clamp(Vec2::ZERO, self.frame_px);
        Some(clamped * self.device_points / self.frame_px)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Vec2 = Vec2::new(1206.0, 2622.0);
    const POINTS: Vec2 = Vec2::new(402.0, 874.0);

    impl Mapping {
        fn phone_in(window: Vec2) -> Self {
            Mapping::new(window, FRAME, POINTS).expect("valid mapping")
        }
    }

    #[test]
    fn centre_of_a_pillarboxed_window_is_centre_of_the_device() {
        let mapping = Mapping::phone_in(Vec2::new(1600.0, 900.0));

        let device = mapping
            .cursor_to_device(Vec2::new(800.0, 450.0))
            .expect("on device");

        assert!((device.x - 201.0).abs() < 0.01, "got {device:?}");
        assert!((device.y - 437.0).abs() < 0.01, "got {device:?}");
    }

    #[test]
    fn pillarbox_margins_are_not_on_the_device() {
        let window = Vec2::new(1600.0, 900.0);
        let mapping = Mapping::phone_in(window);
        let drawn = mapping.drawn_size();
        let margin = (window.x - drawn.x) / 2.0;

        assert!(margin > 1.0, "expected pillarboxing, drawn {drawn:?}");
        assert_eq!(
            mapping.cursor_to_device(Vec2::new(margin - 1.0, 450.0)),
            None
        );
        assert_eq!(
            mapping.cursor_to_device(Vec2::new(window.x - margin + 1.0, 450.0)),
            None
        );
    }

    #[test]
    fn top_left_of_the_drawn_image_is_the_device_origin() {
        let window = Vec2::new(1600.0, 900.0);
        let mapping = Mapping::phone_in(window);
        let drawn = mapping.drawn_size();
        let origin = (window - drawn) / 2.0;

        let device = mapping.cursor_to_device(origin).expect("on device");

        assert!(device.length() < 0.01, "got {device:?}");
    }

    #[test]
    fn bottom_right_of_the_drawn_image_is_the_far_corner_in_points() {
        let window = Vec2::new(1600.0, 900.0);
        let mapping = Mapping::phone_in(window);
        let corner = (window + mapping.drawn_size()) / 2.0;

        let device = mapping.cursor_to_device(corner).expect("on device");

        assert!((device.x - POINTS.x).abs() < 0.01, "got {device:?}");
        assert!((device.y - POINTS.y).abs() < 0.01, "got {device:?}");
    }

    #[test]
    fn halving_the_stream_scale_does_not_move_the_tap() {
        let window = Vec2::new(1600.0, 900.0);
        let full = Mapping::phone_in(window);
        let halved = Mapping::new(window, FRAME / 2.0, POINTS).expect("valid");
        let cursor = Vec2::new(770.0, 500.0);

        let a = full.cursor_to_device(cursor).expect("on device");
        let b = halved.cursor_to_device(cursor).expect("on device");

        assert!((a - b).length() < 0.01, "{a:?} vs {b:?}");
    }

    #[test]
    fn landscape_frame_letterboxes_vertically() {
        let window = Vec2::new(1600.0, 900.0);
        let mapping = Mapping::new(window, Vec2::new(2622.0, 1206.0), Vec2::new(874.0, 402.0))
            .expect("valid");
        let drawn = mapping.drawn_size();

        assert!(drawn.y < window.y, "expected letterboxing, drawn {drawn:?}");
        assert!(
            (drawn.x - window.x).abs() < 0.01,
            "expected full width, drawn {drawn:?}"
        );
    }

    #[test]
    fn degenerate_sizes_have_no_mapping() {
        assert_eq!(Mapping::new(Vec2::ZERO, FRAME, POINTS), None);
        assert_eq!(Mapping::new(Vec2::splat(100.0), Vec2::ZERO, POINTS), None);
        assert_eq!(Mapping::new(Vec2::splat(100.0), FRAME, Vec2::ZERO), None);
    }
}
