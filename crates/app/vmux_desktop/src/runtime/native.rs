//! Pure decision logic behind the macOS native event monitors: pointer ownership,
//! scroll wakes, render demand, and live-resize geometry.
//!
//! Only the macOS runtime drives these, but they are compiled under `test` on every
//! platform so CI exercises them away from AppKit.

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
}
