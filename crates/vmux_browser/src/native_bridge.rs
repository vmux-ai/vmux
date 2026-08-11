//! What the platform's native views ask the ECS, and what they hand back.
//!
//! The event monitors that drive windowed views run on the main thread outside Bevy, so the
//! frames the ECS publishes are parked here for them to hit-test against, and the pointer events
//! they capture are queued for the next frame to drain. Every operation is implemented once per
//! platform in a sibling module — exactly one of which is compiled.

use bevy::prelude::*;

#[cfg(any(target_os = "macos", test))]
use crate::present::WindowedFrameRect;

/// The windowed views the platform draws outside the Bevy window, as its event monitors see them.
pub struct NativeBridge;

impl NativeBridge {
    /// Whether `point` falls inside `frame`. Pure geometry, so it lives here rather than in a
    /// platform half — but only AppKit hit-tests, so off macOS nothing but the tests call it.
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn frame_contains(frame: WindowedFrameRect, point: Vec2) -> bool {
        point.x >= frame.left
            && point.x <= frame.right()
            && point.y >= frame.top
            && point.y <= frame.bottom()
    }

    /// The smallest frame covering all of `frames`, or `None` when there are none.
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

/// Queued pointer input has no counterpart off macOS: nothing else draws views the Bevy window
/// does not own, so no monitor exists to capture it.
#[cfg(target_os = "macos")]
pub(crate) use platform::CommandBarPointerEvent;
#[cfg(target_os = "macos")]
pub use platform::{queue_command_bar_pointer_button, queue_command_bar_pointer_move};
