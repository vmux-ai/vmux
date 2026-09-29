pub(super) struct WindowPointerPolicy;

impl WindowPointerPolicy {
    pub(super) fn windowed_presence(
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

    pub(super) fn scroll_should_wake(
        layout_pointer_inside: bool,
        sampled_over_windowed_page: bool,
    ) -> bool {
        layout_pointer_inside || !sampled_over_windowed_page
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct WindowFrame {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) width: f64,
    pub(super) height: f64,
}

impl WindowFrame {
    fn matches(self, other: Self) -> bool {
        const SLOP: f64 = 1.0;

        (self.x - other.x).abs() <= SLOP
            && (self.y - other.y).abs() <= SLOP
            && (self.width - other.width).abs() <= SLOP
            && (self.height - other.height).abs() <= SLOP
    }

    pub(super) fn resize_edges(self, cursor_x: f64, cursor_y: f64, grip: f64) -> ResizeEdges {
        let right = self.x + self.width;
        let top = self.y + self.height;
        let within_x = cursor_x >= self.x - grip && cursor_x <= right + grip;
        let within_y = cursor_y >= self.y - grip && cursor_y <= top + grip;
        ResizeEdges {
            left: within_y && (cursor_x - self.x).abs() <= grip,
            right: within_y && (cursor_x - right).abs() <= grip,
            bottom: within_x && (cursor_y - self.y).abs() <= grip,
            top: within_x && (cursor_y - top).abs() <= grip,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ResizeEdges {
    pub(super) left: bool,
    pub(super) right: bool,
    pub(super) bottom: bool,
    pub(super) top: bool,
}

impl ResizeEdges {
    pub(super) fn any(self) -> bool {
        self.left || self.right || self.bottom || self.top
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WindowResizeDrag {
    pub(super) frame: WindowFrame,
    pub(super) cursor_x: f64,
    pub(super) cursor_y: f64,
    pub(super) min_width: f64,
    pub(super) min_height: f64,
    pub(super) edges: ResizeEdges,
}

impl WindowResizeDrag {
    pub(super) fn resized_frame(self, cursor_x: f64, cursor_y: f64) -> WindowFrame {
        let mut frame = self.frame;
        let delta_x = cursor_x - self.cursor_x;
        let delta_y = cursor_y - self.cursor_y;
        if self.edges.left {
            let right = self.frame.x + self.frame.width;
            frame.x = self.frame.x + delta_x;
            frame.width = self.frame.width - delta_x;
            if frame.width < self.min_width {
                frame.width = self.min_width;
                frame.x = right - self.min_width;
            }
        } else if self.edges.right {
            frame.width = (self.frame.width + delta_x).max(self.min_width);
        }
        if self.edges.bottom {
            let top = self.frame.y + self.frame.height;
            frame.y = self.frame.y + delta_y;
            frame.height = self.frame.height - delta_y;
            if frame.height < self.min_height {
                frame.height = self.min_height;
                frame.y = top - self.min_height;
            }
        } else if self.edges.top {
            frame.height = (self.frame.height + delta_y).max(self.min_height);
        }
        frame
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WindowTitlebarGesture {
    Drag,
    Zoom,
    Miniaturize,
    Ignore,
}

impl WindowTitlebarGesture {
    pub(super) fn resolve(click_count: isize, double_click_action: Option<&str>) -> Self {
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
pub(super) struct WindowZoom(Option<WindowFrame>);

impl WindowZoom {
    pub(super) fn toggled(&mut self, current: WindowFrame, zoomed: WindowFrame) -> WindowFrame {
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
    fn window_resize_detects_edges_and_corners() {
        let frame = WindowFrame {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        };

        assert_eq!(
            frame.resize_edges(100.0, 100.0, 8.0),
            ResizeEdges {
                left: true,
                bottom: true,
                ..Default::default()
            }
        );
        assert_eq!(
            frame.resize_edges(900.0, 700.0, 8.0),
            ResizeEdges {
                right: true,
                top: true,
                ..Default::default()
            }
        );
        assert_eq!(
            frame.resize_edges(500.0, 100.0, 8.0),
            ResizeEdges {
                bottom: true,
                ..Default::default()
            }
        );
        assert!(!frame.resize_edges(500.0, 400.0, 8.0).any());
    }

    #[test]
    fn corner_resize_updates_both_axes_and_clamps_minimum() {
        let drag = WindowResizeDrag {
            frame: WindowFrame {
                x: 100.0,
                y: 100.0,
                width: 800.0,
                height: 600.0,
            },
            cursor_x: 100.0,
            cursor_y: 100.0,
            min_width: 200.0,
            min_height: 120.0,
            edges: ResizeEdges {
                left: true,
                bottom: true,
                ..Default::default()
            },
        };

        assert_eq!(
            drag.resized_frame(150.0, 150.0),
            WindowFrame {
                x: 150.0,
                y: 150.0,
                width: 750.0,
                height: 550.0,
            }
        );
        assert_eq!(
            drag.resized_frame(850.0, 650.0),
            WindowFrame {
                x: 700.0,
                y: 580.0,
                width: 200.0,
                height: 120.0,
            }
        );
    }

    #[test]
    fn scroll_preserves_windowed_page_pointer_ownership() {
        assert!(WindowPointerPolicy::windowed_presence(false, true, false));
        assert!(!WindowPointerPolicy::windowed_presence(false, false, true));
        assert!(!WindowPointerPolicy::windowed_presence(true, true, false));
        assert!(WindowPointerPolicy::windowed_presence(true, false, true));
    }

    #[test]
    fn native_scroll_wakes_bevy_only_for_layout_or_non_windowed_content() {
        assert!(!WindowPointerPolicy::scroll_should_wake(false, true));
        assert!(WindowPointerPolicy::scroll_should_wake(true, true));
        assert!(WindowPointerPolicy::scroll_should_wake(false, false));
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
            WindowTitlebarGesture::resolve(1, Some("Minimize")),
            WindowTitlebarGesture::Drag
        );
        assert_eq!(
            WindowTitlebarGesture::resolve(2, Some("Minimize")),
            WindowTitlebarGesture::Miniaturize
        );
        assert_eq!(
            WindowTitlebarGesture::resolve(2, Some("None")),
            WindowTitlebarGesture::Ignore
        );
        assert_eq!(
            WindowTitlebarGesture::resolve(2, Some("Maximize")),
            WindowTitlebarGesture::Zoom
        );
        assert_eq!(
            WindowTitlebarGesture::resolve(2, None),
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

    const WINDOWED: WindowFrame = WindowFrame {
        x: 240.0,
        y: 180.0,
        width: 900.0,
        height: 600.0,
    };
    const VISIBLE: WindowFrame = WindowFrame {
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
        let moved = WindowFrame {
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
