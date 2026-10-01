use bevy::prelude::*;
use bevy_cef::prelude::PointerButton;
use bevy_cef_core::prelude::NativeMouseButtons;
use std::sync::{LazyLock, Mutex};

use super::NativeBridge;
use crate::host::CommandBarRoute;
use crate::present::WindowedFrameRect;

static BRIDGE: LazyLock<Mutex<NativeBridgeState>> = LazyLock::new(Default::default);

#[derive(Default)]
struct NativeBridgeState {
    frames: Vec<WindowedFrameRect>,
    pointer_events: Vec<CommandBarPointerEvent>,
}

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
        BRIDGE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .frames
            .iter()
            .copied()
            .any(|frame| Self::frame_contains(frame, point))
    }

    pub fn command_bar_contains_point(x_px: f32, y_px: f32) -> bool {
        Self::command_bar_local_position(x_px, y_px).is_some()
    }

    pub(crate) fn set_windowed_page_frames(
        mut frames: Vec<WindowedFrameRect>,
    ) -> Vec<WindowedFrameRect> {
        let mut bridge = BRIDGE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::swap(&mut bridge.frames, &mut frames);
        frames.clear();
        frames
    }

    pub(crate) fn windowed_page_bounds() -> Option<WindowedFrameRect> {
        let bridge = BRIDGE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::frames_union(&bridge.frames)
    }

    pub(crate) fn drain_command_bar_pointer_events() -> Vec<CommandBarPointerEvent> {
        std::mem::take(
            &mut BRIDGE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .pointer_events,
        )
    }

    fn command_bar_local_position(x_px: f32, y_px: f32) -> Option<Vec2> {
        CommandBarRoute::current().local_position(Vec2::new(x_px, y_px))
    }
}

pub fn queue_command_bar_pointer_move(x_px: f32, y_px: f32, buttons: NativeMouseButtons) -> bool {
    let Some(position) = NativeBridge::command_bar_local_position(x_px, y_px) else {
        return false;
    };
    BRIDGE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .pointer_events
        .push(CommandBarPointerEvent::Move { position, buttons });
    true
}

pub fn queue_command_bar_pointer_button(x_px: f32, y_px: f32, button: u8, released: bool) -> bool {
    let Some(position) = NativeBridge::command_bar_local_position(x_px, y_px) else {
        return false;
    };
    let button = match button {
        1 => PointerButton::Secondary,
        2 => PointerButton::Middle,
        _ => PointerButton::Primary,
    };
    BRIDGE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .pointer_events
        .push(CommandBarPointerEvent::Button {
            position,
            button,
            released,
        });
    true
}
