//! The macOS half: AppKit draws the windowed views, so its monitors do the asking.

use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy_cef_core::prelude::NativeMouseButtons;
use std::sync::{LazyLock, Mutex};

use super::NativeBridge;
use crate::present::WindowedFrameRect;
use crate::{command_bar_windowed_frame_contains, native_command_bar_route};

/// The frames the ECS published last, for the monitors to hit-test against.
static WINDOWED_PAGE_FRAMES: LazyLock<Mutex<Vec<WindowedFrameRect>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Pointer input the monitors captured, waiting for the next frame to deliver it.
static COMMAND_BAR_POINTER_EVENTS: LazyLock<Mutex<Vec<CommandBarPointerEvent>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

#[derive(Clone, Copy, Debug)]
pub(crate) enum CommandBarPointerEvent {
    Move {
        position: Vec2,
        buttons: NativeMouseButtons,
    },
    Button {
        position: Vec2,
        button: PointerButton,
        released: bool,
    },
}

impl NativeBridge {
    pub fn windowed_page_contains_point(x_px: f32, y_px: f32) -> bool {
        let point = Vec2::new(x_px, y_px);
        WINDOWED_PAGE_FRAMES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .copied()
            .any(|frame| Self::frame_contains(frame, point))
    }

    pub fn command_bar_contains_point(x_px: f32, y_px: f32) -> bool {
        Self::command_bar_local_position(x_px, y_px).is_some()
    }

    /// Publishes `frames` and hands back the previous vector, emptied, for the caller to refill.
    pub(crate) fn set_windowed_page_frames(
        mut frames: Vec<WindowedFrameRect>,
    ) -> Vec<WindowedFrameRect> {
        let mut published = WINDOWED_PAGE_FRAMES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::swap(&mut *published, &mut frames);
        frames.clear();
        frames
    }

    pub(crate) fn windowed_page_bounds() -> Option<WindowedFrameRect> {
        let frames = WINDOWED_PAGE_FRAMES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::frames_union(&frames)
    }

    pub(crate) fn drain_command_bar_pointer_events() -> Vec<CommandBarPointerEvent> {
        std::mem::take(
            &mut *COMMAND_BAR_POINTER_EVENTS
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
    }

    /// Where the point lands inside the command bar's own coordinate space, or `None` when the
    /// bar does not own input or the point is outside it.
    fn command_bar_local_position(x_px: f32, y_px: f32) -> Option<Vec2> {
        let route = native_command_bar_route();
        if !route.owns_input {
            return None;
        }
        let frame = route
            .frame
            .filter(|frame| command_bar_windowed_frame_contains(*frame, Vec2::new(x_px, y_px)))?;
        let scale = route.scale.max(1.0e-6);
        Some(Vec2::new(
            (x_px - frame.left_px) / scale,
            (y_px - frame.top_px) / scale,
        ))
    }
}

/// Called by the AppKit monitor. Returns whether the command bar claimed the event.
pub fn queue_command_bar_pointer_move(x_px: f32, y_px: f32, buttons: NativeMouseButtons) -> bool {
    let Some(position) = NativeBridge::command_bar_local_position(x_px, y_px) else {
        return false;
    };
    COMMAND_BAR_POINTER_EVENTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(CommandBarPointerEvent::Move { position, buttons });
    true
}

/// Called by the AppKit monitor. Returns whether the command bar claimed the event.
pub fn queue_command_bar_pointer_button(x_px: f32, y_px: f32, button: u8, released: bool) -> bool {
    let Some(position) = NativeBridge::command_bar_local_position(x_px, y_px) else {
        return false;
    };
    let button = match button {
        1 => PointerButton::Secondary,
        2 => PointerButton::Middle,
        _ => PointerButton::Primary,
    };
    COMMAND_BAR_POINTER_EVENTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(CommandBarPointerEvent::Button {
            position,
            button,
            released,
        });
    true
}
