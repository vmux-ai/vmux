pub(super) fn windowed_pointer_inside_after_event(
    pointer_position_changed: bool,
    previous: bool,
    sampled: bool,
) -> bool {
    if pointer_position_changed {
        sampled
    } else {
        previous
    }
}

pub(super) fn native_scroll_should_wake(
    layout_pointer_inside: bool,
    sampled_over_windowed_page: bool,
) -> bool {
    layout_pointer_inside || !sampled_over_windowed_page
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct NativeWindowFrame {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) width: f64,
    pub(super) height: f64,
}

impl NativeWindowFrame {
    fn matches(self, other: Self) -> bool {
        const SLOP: f64 = 1.0;

        (self.x - other.x).abs() <= SLOP
            && (self.y - other.y).abs() <= SLOP
            && (self.width - other.width).abs() <= SLOP
            && (self.height - other.height).abs() <= SLOP
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct NativeResizeEdges {
    pub(super) left: bool,
    pub(super) right: bool,
    pub(super) bottom: bool,
    pub(super) top: bool,
}

impl NativeResizeEdges {
    pub(super) fn any(self) -> bool {
        self.left || self.right || self.bottom || self.top
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct NativeWindowResizeDrag {
    pub(super) frame: NativeWindowFrame,
    pub(super) cursor_x: f64,
    pub(super) cursor_y: f64,
    pub(super) min_width: f64,
    pub(super) min_height: f64,
    pub(super) edges: NativeResizeEdges,
}

pub(super) fn native_resize_edges(
    frame: NativeWindowFrame,
    cursor_x: f64,
    cursor_y: f64,
    grip: f64,
) -> NativeResizeEdges {
    let right = frame.x + frame.width;
    let top = frame.y + frame.height;
    let within_x = cursor_x >= frame.x - grip && cursor_x <= right + grip;
    let within_y = cursor_y >= frame.y - grip && cursor_y <= top + grip;
    NativeResizeEdges {
        left: within_y && (cursor_x - frame.x).abs() <= grip,
        right: within_y && (cursor_x - right).abs() <= grip,
        bottom: within_x && (cursor_y - frame.y).abs() <= grip,
        top: within_x && (cursor_y - top).abs() <= grip,
    }
}

pub(super) fn resized_native_window_frame(
    drag: NativeWindowResizeDrag,
    cursor_x: f64,
    cursor_y: f64,
) -> NativeWindowFrame {
    let mut frame = drag.frame;
    let delta_x = cursor_x - drag.cursor_x;
    let delta_y = cursor_y - drag.cursor_y;
    if drag.edges.left {
        let right = drag.frame.x + drag.frame.width;
        frame.x = drag.frame.x + delta_x;
        frame.width = drag.frame.width - delta_x;
        if frame.width < drag.min_width {
            frame.width = drag.min_width;
            frame.x = right - drag.min_width;
        }
    } else if drag.edges.right {
        frame.width = (drag.frame.width + delta_x).max(drag.min_width);
    }
    if drag.edges.bottom {
        let top = drag.frame.y + drag.frame.height;
        frame.y = drag.frame.y + delta_y;
        frame.height = drag.frame.height - delta_y;
        if frame.height < drag.min_height {
            frame.height = drag.min_height;
            frame.y = top - drag.min_height;
        }
    } else if drag.edges.top {
        frame.height = (drag.frame.height + delta_y).max(drag.min_height);
    }
    frame
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WindowTitlebarGesture {
    Drag,
    Zoom,
    Miniaturize,
    Ignore,
}

impl WindowTitlebarGesture {
    pub(super) fn of(click_count: isize, double_click_action: Option<&str>) -> Self {
        if click_count < 2 {
            return Self::Drag;
        }
        match double_click_action {
            Some("Minimize") => Self::Miniaturize,
            Some("None") => Self::Ignore,
            _ => Self::Zoom,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TitlebarClick {
    pub(super) at: f64,
    pub(super) x: f32,
    pub(super) y: f32,
}

impl TitlebarClick {
    fn repeats(self, earlier: Self, interval: f64, slop_px: f32) -> bool {
        self.at >= earlier.at
            && self.at - earlier.at <= interval
            && (self.x - earlier.x).abs() <= slop_px
            && (self.y - earlier.y).abs() <= slop_px
    }
}

#[derive(Default)]
pub(super) struct TitlebarClicks(Option<TitlebarClick>);

impl TitlebarClicks {
    pub(super) fn count(&mut self, click: TitlebarClick, interval: f64, slop_px: f32) -> isize {
        let Some(earlier) = self.0 else {
            self.0 = Some(click);
            return 1;
        };
        if !click.repeats(earlier, interval, slop_px) {
            self.0 = Some(click);
            return 1;
        }
        self.0 = None;
        2
    }

    pub(super) fn forget(&mut self) {
        self.0 = None;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct WindowZoom(Option<NativeWindowFrame>);

impl WindowZoom {
    pub(super) fn toggled(
        &mut self,
        current: NativeWindowFrame,
        zoomed: NativeWindowFrame,
    ) -> NativeWindowFrame {
        if let Some(restore) = self.0
            && current.matches(zoomed)
        {
            self.0 = None;
            return restore;
        }
        self.0 = Some(current);
        zoomed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_window_resize_detects_edges_and_corners() {
        let frame = NativeWindowFrame {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        };

        assert_eq!(
            native_resize_edges(frame, 100.0, 100.0, 8.0),
            NativeResizeEdges {
                left: true,
                bottom: true,
                ..Default::default()
            }
        );
        assert_eq!(
            native_resize_edges(frame, 900.0, 700.0, 8.0),
            NativeResizeEdges {
                right: true,
                top: true,
                ..Default::default()
            }
        );
        assert_eq!(
            native_resize_edges(frame, 500.0, 100.0, 8.0),
            NativeResizeEdges {
                bottom: true,
                ..Default::default()
            }
        );
        assert!(!native_resize_edges(frame, 500.0, 400.0, 8.0).any());
    }

    #[test]
    fn native_corner_resize_updates_both_axes_and_clamps_minimum() {
        let drag = NativeWindowResizeDrag {
            frame: NativeWindowFrame {
                x: 100.0,
                y: 100.0,
                width: 800.0,
                height: 600.0,
            },
            cursor_x: 100.0,
            cursor_y: 100.0,
            min_width: 200.0,
            min_height: 120.0,
            edges: NativeResizeEdges {
                left: true,
                bottom: true,
                ..Default::default()
            },
        };

        assert_eq!(
            resized_native_window_frame(drag, 150.0, 150.0),
            NativeWindowFrame {
                x: 150.0,
                y: 150.0,
                width: 750.0,
                height: 550.0,
            }
        );
        assert_eq!(
            resized_native_window_frame(drag, 850.0, 650.0),
            NativeWindowFrame {
                x: 700.0,
                y: 580.0,
                width: 200.0,
                height: 120.0,
            }
        );
    }

    #[test]
    fn scroll_preserves_windowed_page_pointer_ownership() {
        assert!(windowed_pointer_inside_after_event(false, true, false));
        assert!(!windowed_pointer_inside_after_event(false, false, true));
        assert!(!windowed_pointer_inside_after_event(true, true, false));
        assert!(windowed_pointer_inside_after_event(true, false, true));
    }

    #[test]
    fn native_scroll_wakes_bevy_only_for_layout_or_non_windowed_content() {
        assert!(!native_scroll_should_wake(false, true));
        assert!(native_scroll_should_wake(true, true));
        assert!(native_scroll_should_wake(false, false));
    }

    #[test]
    fn a_quick_second_click_in_the_same_spot_is_a_double_and_a_third_is_not() {
        let mut clicks = TitlebarClicks::default();
        let first = TitlebarClick {
            at: 10.0,
            x: 400.0,
            y: 20.0,
        };
        let again = TitlebarClick {
            at: 10.2,
            x: 403.0,
            y: 22.0,
        };
        let third = TitlebarClick { at: 10.3, ..again };

        let counts = [
            clicks.count(first, 0.5, 8.0),
            clicks.count(again, 0.5, 8.0),
            clicks.count(third, 0.5, 8.0),
        ];

        assert_eq!(counts, [1, 2, 1]);
    }

    #[test]
    fn a_second_click_too_late_or_too_far_away_starts_over() {
        let first = TitlebarClick {
            at: 10.0,
            x: 400.0,
            y: 20.0,
        };
        let mut late = TitlebarClicks::default();
        late.count(first, 0.5, 8.0);
        let mut far = TitlebarClicks::default();
        far.count(first, 0.5, 8.0);

        assert_eq!(late.count(TitlebarClick { at: 10.9, ..first }, 0.5, 8.0), 1);
        assert_eq!(
            far.count(
                TitlebarClick {
                    at: 10.2,
                    x: 440.0,
                    ..first
                },
                0.5,
                8.0
            ),
            1
        );
    }

    #[test]
    fn a_second_click_on_the_titlebar_follows_the_system_double_click_action() {
        assert_eq!(
            WindowTitlebarGesture::of(1, Some("Minimize")),
            WindowTitlebarGesture::Drag
        );
        assert_eq!(
            WindowTitlebarGesture::of(2, Some("Minimize")),
            WindowTitlebarGesture::Miniaturize
        );
        assert_eq!(
            WindowTitlebarGesture::of(2, Some("None")),
            WindowTitlebarGesture::Ignore
        );
        assert_eq!(
            WindowTitlebarGesture::of(2, Some("Maximize")),
            WindowTitlebarGesture::Zoom
        );
        assert_eq!(
            WindowTitlebarGesture::of(2, None),
            WindowTitlebarGesture::Zoom
        );
    }

    #[test]
    fn leaving_the_drag_region_forgets_the_first_titlebar_click() {
        let mut clicks = TitlebarClicks::default();
        let first = TitlebarClick {
            at: 10.0,
            x: 400.0,
            y: 20.0,
        };

        clicks.count(first, 0.5, 8.0);
        clicks.forget();

        assert_eq!(
            clicks.count(TitlebarClick { at: 10.2, ..first }, 0.5, 8.0),
            1
        );
    }

    const WINDOWED: NativeWindowFrame = NativeWindowFrame {
        x: 240.0,
        y: 180.0,
        width: 900.0,
        height: 600.0,
    };
    const VISIBLE: NativeWindowFrame = NativeWindowFrame {
        x: 0.0,
        y: 0.0,
        width: 1512.0,
        height: 944.0,
    };

    #[test]
    fn zooming_twice_puts_the_window_back_where_it_started() {
        let mut zoom = WindowZoom::default();

        let zoomed = zoom.toggled(WINDOWED, VISIBLE);
        let restored = zoom.toggled(zoomed, VISIBLE);

        assert_eq!(zoomed, VISIBLE);
        assert_eq!(restored, WINDOWED);
    }

    #[test]
    fn moving_a_zoomed_window_makes_the_next_zoom_remember_where_it_was_moved_to() {
        let mut zoom = WindowZoom::default();
        let moved = NativeWindowFrame {
            x: 40.0,
            y: 60.0,
            ..VISIBLE
        };
        zoom.toggled(WINDOWED, VISIBLE);

        let rezoomed = zoom.toggled(moved, VISIBLE);
        let restored = zoom.toggled(rezoomed, VISIBLE);

        assert_eq!(rezoomed, VISIBLE);
        assert_eq!(restored, moved);
    }
}
