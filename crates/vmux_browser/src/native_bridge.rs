#[cfg(any(target_os = "macos", test))]
use bevy::math::Vec2;

#[cfg(any(target_os = "macos", test))]
use crate::present::WindowedFrameRect;

pub struct NativeBridge;

impl NativeBridge {
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn frame_contains(frame: WindowedFrameRect, point: Vec2) -> bool {
        point.x >= frame.left
            && point.x <= frame.right()
            && point.y >= frame.top
            && point.y <= frame.bottom()
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn frames_union(frames: &[WindowedFrameRect]) -> Option<WindowedFrameRect> {
        let first = *frames.first()?;
        let mut left = first.left;
        let mut top = first.top;
        let mut right = first.right();
        let mut bottom = first.bottom();
        for frame in &frames[1..] {
            left = left.min(frame.left);
            top = top.min(frame.top);
            right = right.max(frame.right());
            bottom = bottom.max(frame.bottom());
        }
        Some(WindowedFrameRect {
            left,
            top,
            width: right - left,
            height: bottom - top,
        })
    }
}

#[cfg(target_os = "macos")]
#[path = "native_bridge/macos.rs"]
mod platform;
#[cfg(not(target_os = "macos"))]
#[path = "native_bridge/other.rs"]
mod platform;

#[cfg(target_os = "macos")]
pub(crate) use platform::CommandBarPointerEvent;
#[cfg(target_os = "macos")]
pub use platform::{queue_command_bar_pointer_button, queue_command_bar_pointer_move};
